use {
    crate::{
        green::{Element, Token},
        ArcBorrow, Kind, NodeOrToken, TextSize,
    },
    erasable::{Erasable, ErasedPtr},
    slice_dst::{AllocSliceDst, SliceDst},
    std::{alloc::Layout, hash, iter::FusedIterator, mem, ptr, slice, sync::Arc},
    text_size::TextLen,
};
use std::mem::ManuallyDrop;
use crate::green::element::{FullAlignedElement, HalfAlignedElement};

/// A nonleaf node in the immutable green tree.
///
/// Nodes are crated using [`Builder::node`](crate::green::Builder::node).
#[repr(C, align(8))] // NB: align >= 8
#[derive(Debug, Eq)]
pub struct Node {
    // NB: This is optimal layout, as the order is (u16, u16, u32, [{see element.rs}])
    // SAFETY: Must be at offset 0 and accurate to trailing array length.
    children_len: u16,  // align 8 + 0, size 2
    kind: Kind,         // align 8 + 2, size 2
    text_len: TextSize, // align 8 + 4, size 4
    // SAFETY: Must be aligned to 8
    children: [Element], // align 8 + 0, dyn size
}

// Manually impl Eq/Hash to match Token
// Plus we can skip .children_len since it's derived from .children
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.text_len == other.text_len
            && self.children == other.children
    }
}

impl hash::Hash for Node {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.text_len.hash(state);
        self.children.hash(state);
    }
}

#[allow(clippy::len_without_is_empty)]
impl Node {
    /// The kind of this node.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The length of text at this node.
    pub fn len(&self) -> TextSize {
        self.text_len
    }

    /// Child elements of this node.
    pub fn children(&self) -> Children<'_> {
        Children { inner: self.children.iter() }
    }

    /// Child element containing the given offset from the start of this node.
    ///
    /// # Panics
    ///
    /// Panics if the given offset is outside of this node.
    pub fn child_at_offset(
        &self,
        offset: TextSize,
    ) -> NodeOrToken<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
        assert!(offset < self.len());
        let index = self
            .children
            .binary_search_by_key(&offset, |el| el.offset())
            .unwrap_or_else(|index| index - 1);
        let element = unsafe { self.children.get_unchecked(index) };
        element.into()
    }
}

impl TextLen for &'_ Node {
    fn text_len(self) -> TextSize {
        self.len()
    }
}

#[allow(clippy::len_without_is_empty)]
impl Node {
    // SAFETY: must accurately calculate the layout for length `len`
    fn layout(len: usize) -> (Layout, [usize; 4]) {
        let (layout, offset_0) = (Layout::new::<u16>(), 0);
        let (layout, offset_1) = layout.extend(Layout::new::<Kind>()).unwrap();
        let (layout, offset_2) = layout.extend(Layout::new::<TextSize>()).unwrap();
        let (layout, offset_3) = layout.extend(Layout::array::<Element>(len).unwrap()).unwrap();
        (layout.pad_to_align(), [offset_0, offset_1, offset_2, offset_3])
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new<A, I>(kind: Kind, children: I) -> A
    where
        A: AllocSliceDst<Self>,
        I: IntoIterator<Item = NodeOrToken<Arc<Node>, Arc<Token>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut children = children.into_iter();
        let len = children.len();
        assert!(len <= u16::MAX as usize, "more children than fit in one node");
        let children_len = len as u16;
        let (layout, [children_len_offset, kind_offset, text_len_offset, children_offset]) =
            Self::layout(len);

        unsafe {
            // SAFETY: closure fully initializes the place
            A::new_slice_dst(len, |ptr| {
                /// Helper to drop children on panic.
                struct ChildrenWriter {
                    raw: *mut Element,
                    len: usize,
                    text_len: TextSize,
                }

                impl Drop for ChildrenWriter {
                    fn drop(&mut self) {
                        unsafe {
                            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(self.raw, self.len));
                        }
                    }
                }

                impl ChildrenWriter {
                    fn new(raw: *mut Element) -> Self {
                        ChildrenWriter { raw, len: 0, text_len: TextSize::zero() }
                    }

                    unsafe fn push(&mut self, element: NodeOrToken<Arc<Node>, Arc<Token>>) {
                        let offset = self.text_len;
                        self.text_len += element.as_deref().map(Node::len, Token::len).flatten();
                        if self.len % 2 == 0 {
                            FullAlignedElement::write(self.raw.add(self.len), element, offset);
                        } else {
                            HalfAlignedElement::write(self.raw.add(self.len), element, offset);
                        }
                        self.len += 1;
                    }

                    fn finish(self) -> TextSize {
                        ManuallyDrop::new(self).text_len
                    }
                }

                let raw = ptr.as_ptr().cast::<u8>();

                ptr::write(raw.add(children_len_offset).cast(), children_len);
                ptr::write(raw.add(kind_offset).cast(), kind);

                let mut children_writer = ChildrenWriter::new(raw.add(children_offset).cast());
                for _ in 0..len {
                    let child: NodeOrToken<Arc<Node>, Arc<Token>> =
                        children.next().expect("children iterator over-reported length");
                    children_writer.push(child);
                }
                assert!(children.next().is_none(), "children iterator under-reported length");

                let text_len = children_writer.finish();
                ptr::write(raw.add(text_len_offset).cast(), text_len);
                debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
            })
        }
    }
}

// SAFETY: un/erase correctly round-trips a pointer
unsafe impl Erasable for Node {
    unsafe fn unerase(this: ErasedPtr) -> ptr::NonNull<Self> {
        // SAFETY: children_len is at 0 offset
        let children_len: u16 = ptr::read(this.cast().as_ptr());
        let ptr = ptr::slice_from_raw_parts_mut(this.as_ptr().cast(), children_len.into());
        // SAFETY: ptr comes from NonNull
        Self::retype(ptr::NonNull::new_unchecked(ptr))
    }

    const ACK_1_1_0: bool = true;
}

// SAFETY: layout is correct and retype is the trivial cast
unsafe impl SliceDst for Node {
    fn layout_for(len: usize) -> Layout {
        Self::layout(len).0
    }

    #[allow(clippy::cast_ptr_alignment)]
    fn retype(ptr: ptr::NonNull<[()]>) -> ptr::NonNull<Self> {
        ptr::NonNull::new(ptr.as_ptr() as *mut _).unwrap()
    }
}

/// Children elements of a node in the immutable green tree.
#[derive(Debug, Clone)]
pub struct Children<'a> {
    inner: slice::Iter<'a, Element>,
    // NB: Children can (and probably should) keep track of the alignment of
    // the inner slice iterator. That way, the compiler should be able to pick
    // up on the exclusively flip-flop pattern and optimize out checking for
    // alignment. This is most important for internal iteration (fold), where
    // we can guarantee this by unrolling to a two-stride manually. I've just
    // not done this yet to avoid the extra safety-critical code initially.
}

impl<'a> Children<'a> {
    /// Get the next item in the iterator without advancing it.
    pub fn peek(&self) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        let element = self.inner.as_slice().first()?;
        Some(element.into())
    }

    /// Get the nth item in the iterator without advancing it.
    pub fn peek_n(
        &self,
        n: usize,
    ) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        let element = self.inner.as_slice().get(n)?;
        Some(element.into())
    }

    /// Divide this iterator into two at an index.
    ///
    /// The first will contain all indices from `[0, mid)`,
    /// and the second will contain all indices from `[mid, len)`.
    ///
    /// # Panics
    ///
    /// Panics if `mid > len`.
    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let (left, right) = self.inner.as_slice().split_at(mid);
        (Children { inner: left.iter() }, Children { inner: right.iter() })
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>;

    fn next(&mut self) -> Option<Self::Item> {
        let element = self.inner.next()?;
        Some(element.into())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    fn count(self) -> usize {
        self.inner.count()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let element = self.inner.nth(n)?;
        Some(element.into())
    }
}

impl ExactSizeIterator for Children<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl DoubleEndedIterator for Children<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let element = self.inner.next_back()?;
        Some(element.into())
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let element = self.inner.nth_back(n)?;
        Some(element.into())
    }
}

impl FusedIterator for Children<'_> {}
