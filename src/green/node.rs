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

/// A nonleaf node in the immutable green tree.
///
/// Nodes are crated using [`Builder::node`](crate::green::Builder::node).
#[repr(C, align(2))] // NB: align >= 2
#[derive(Debug, Eq)]
pub struct Node {
    // NB: This is optimal layout, as the order is (u16, u16, u32, [usize])
    // SAFETY: Must be at offset 0 and accurate to trailing array length.
    children_len: u16,
    kind: Kind,
    text_len: TextSize,
    children: [Element],
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
        todo!()
        // Children { inner: self.children.iter() }
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
        let mut text_len = TextSize::zero();
        let (layout, [children_len_offset, kind_offset, text_len_offset, children_offset]) =
            Self::layout(len);

        todo!()
        // unsafe {
        //     // SAFETY: closure fully initializes the place
        //     A::new_slice_dst(len, |ptr| {
        //         /// Helper to drop children on panic.
        //         struct ChildrenWriter {
        //             raw: *mut Element,
        //             len: usize,
        //         }
        //
        //         impl Drop for ChildrenWriter {
        //             fn drop(&mut self) {
        //                 unsafe {
        //                     ptr::drop_in_place(ptr::slice_from_raw_parts_mut(self.raw, self.len));
        //                 }
        //             }
        //         }
        //
        //         impl ChildrenWriter {
        //             unsafe fn push(&mut self, element: Element) {
        //                 ptr::write(self.raw.add(self.len), element);
        //                 self.len += 1;
        //             }
        //
        //             fn finish(self) {
        //                 mem::forget(self)
        //             }
        //         }
        //
        //         let raw = ptr.as_ptr().cast::<u8>();
        //
        //         ptr::write(raw.add(children_len_offset).cast(), children_len);
        //         ptr::write(raw.add(kind_offset).cast(), kind);
        //
        //         let mut children_writer =
        //             ChildrenWriter { raw: raw.add(children_offset).cast(), len: 0 };
        //         for _ in 0..len {
        //             let child: Element =
        //                 children.next().expect("children iterator over-reported length");
        //             text_len = text_len.checked_add(child.len()).expect("TextSize overflow");
        //             children_writer.push(child);
        //         }
        //         assert!(children.next().is_none(), "children iterator under-reported length");
        //
        //         ptr::write(raw.add(text_len_offset).cast(), text_len);
        //         debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
        //
        //         children_writer.finish()
        //     })
        // }
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
}

impl<'a> Children<'a> {
    /// Get the next item in the iterator without advancing it.
    pub fn peek(&self) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        todo!()
        // self.inner.as_slice().first().map(Into::into)
    }

    /// Get the nth item in the iterator without advancing it.
    pub fn peek_n(
        &self,
        n: usize,
    ) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        todo!()
        // self.inner.as_slice().get(n).map(Into::into)
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
        todo!()
        // let (left, right) = self.inner.as_slice().split_at(mid);
        // (Children { inner: left.iter() }, Children { inner: right.iter() })
    }
}

// impl Children<'_> {
//     pub(crate) fn none() -> Self {
//         Children { inner: [].iter() }
//     }
// }

impl<'a> Iterator for Children<'a> {
    type Item = NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
        // self.inner.next().map(Into::into)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        todo!()
        // self.inner.size_hint()
    }

    fn count(self) -> usize {
        todo!()
        // self.inner.count()
    }

    fn last(self) -> Option<Self::Item> {
        todo!()
        // self.inner.last().map(Into::into)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        todo!()
        // self.inner.nth(n).map(Into::into)
    }
}

impl ExactSizeIterator for Children<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        todo!()
        // self.inner.len()
    }
}

impl DoubleEndedIterator for Children<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        todo!()
        // self.inner.next_back().map(Into::into)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        todo!()
        // self.inner.nth_back(n).map(Into::into)
    }
}

impl FusedIterator for Children<'_> {}
