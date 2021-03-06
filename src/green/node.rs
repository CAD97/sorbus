#[cfg(feature = "de")]
use slice_dst::TryAllocSliceDst;
use {
    crate::{
        green::{
            unpack_node_or_token, Children, Element, FullAlignedElement, HalfAlignedElement,
            PackedNodeOrToken,
        },
        Kind, TextSize,
    },
    erasable::{Erasable, ErasedPtr},
    ptr_union::Enum2,
    slice_dst::{AllocSliceDst, SliceDst},
    std::{alloc::Layout, hash, mem::ManuallyDrop, ptr, sync::Arc, u16},
};

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

// Element is a union, so we have to make sure to drop them manually here.
impl Drop for Node {
    #[inline]
    fn drop(&mut self) {
        /// Queue this node's children to be dropped if this is the last handle,
        /// then drop the reference counted handle (freeing the node itself),
        /// without recursing into the node's `Drop` implementation.
        ///
        /// Note: this is a best-effort flattening of dropping the tree.
        /// If this function is used concurrently on two handles to the same node,
        /// it is possible that neither will observe being the last outstanding handle
        /// (before the synchronization in `Arc::drop`) and drop the node handle normally.
        /// The node will still be properly dropped, just by calling its destructor
        /// rather than taking its children into the iterative drop list.
        fn maybe_drop_into(mut this: Arc<Node>, stack: &mut Vec<Arc<Node>>) {
            if let Some(node) = Arc::get_mut(&mut this) {
                unsafe {
                    // Queue all of the children to be destructed, and
                    drop_into(node, stack);
                    // Skip running the node's destructor.
                    Arc::<ManuallyDrop<Node>>::from_raw(Arc::into_raw(this) as *const _);
                }
            } else {
                // NB: May actually be the last Arc, if above `Arc::get_mut` races with another thread.
                //  Thus, we only guarantee best-effort iterative drops, as some recursion may happen.
                //  I believe if only one thread drops at a time, drops will always be fully iterative.
                drop(this);
            }
        }

        /// Queue this node's children into a drop queue.
        ///
        /// # Safety
        ///
        /// This takes the children out of the node logically but not physically,
        /// like `ManuallyDrop::take`. The node must not be used (even to drop)
        /// after calling this function.
        unsafe fn drop_into(this: &mut Node, stack: &mut Vec<Arc<Node>>) {
            let mut children = this.children.iter_mut();
            let mut enqueue = |pack: PackedNodeOrToken| {
                unpack_node_or_token(pack).map(|node| stack.push(node), drop)
            };
            (|| -> Option<()> {
                loop {
                    enqueue(children.next()?.full_aligned_mut().take());
                    enqueue(children.next()?.half_aligned_mut().take());
                }
            })();
        }

        unsafe {
            let mut stack = vec![];
            drop_into(self, &mut stack);
            while let Some(element) = stack.pop() {
                maybe_drop_into(element, &mut stack);
            }
        }
    }
}

#[allow(clippy::len_without_is_empty)]
impl Node {
    #[cfg(feature = "de")]
    pub(super) fn set_kind(&mut self, kind: Kind) {
        self.kind = kind;
    }

    /// The kind of this node.
    #[inline]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The length of text at this node.
    #[inline]
    pub fn len(&self) -> TextSize {
        self.text_len
    }

    /// Child elements of this node.
    #[inline]
    pub fn children(&self) -> Children<'_> {
        unsafe { Children::new(&self.children) }
    }

    /// The index of the child that contains the given offset.
    ///
    /// If the offset is the start of a node, returns that node.
    ///
    /// # Panics
    ///
    /// Panics if the given offset is outside of this node.
    #[inline]
    pub fn index_of_offset(&self, offset: TextSize) -> usize {
        assert!(offset < self.len());
        self.children
            .binary_search_by_key(&offset, |el| el.offset())
            .unwrap_or_else(|index| index - 1)
    }
}

/// Helper for writing children during initialization of an element.
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
        ChildrenWriter { raw, len: 0, text_len: 0.into() }
    }

    unsafe fn push(&mut self, element: PackedNodeOrToken) {
        let offset = self.text_len;
        self.text_len += match element.as_deref_unchecked().unpack() {
            Enum2::A(node) => node.len(),
            Enum2::B(token) => token.len(),
        };
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

impl Node {
    // SAFETY: must accurately calculate the layout for length `len`
    fn layout(len: usize) -> (Layout, [usize; 4]) {
        let (layout, offset_0) = (Layout::new::<u16>(), 0);
        let (layout, offset_1) = layout.extend(Layout::new::<Kind>()).unwrap();
        let (layout, offset_2) = layout.extend(Layout::new::<TextSize>()).unwrap();
        let (layout, offset_3) = layout.extend(Layout::array::<Element>(len).unwrap()).unwrap();
        let layout = layout.align_to(8).unwrap();
        (layout.pad_to_align(), [offset_0, offset_1, offset_2, offset_3])
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new<A, I>(kind: Kind, mut children: I) -> A
    where
        A: AllocSliceDst<Self>,
        I: Iterator<Item = PackedNodeOrToken> + ExactSizeIterator,
    {
        let len = children.len();
        assert!(len <= u16::MAX as usize, "more children than fit in one node");
        let children_len = len as u16;
        let (layout, [children_len_offset, kind_offset, text_len_offset, children_offset]) =
            Self::layout(len);

        unsafe {
            // SAFETY: closure fully initializes the place
            A::new_slice_dst(len, |ptr| {
                let raw = ptr.as_ptr().cast::<u8>();

                ptr::write(raw.add(children_len_offset).cast(), children_len);
                ptr::write(raw.add(kind_offset).cast(), kind);

                let mut children_writer = ChildrenWriter::new(raw.add(children_offset).cast());
                for _ in 0..len {
                    let child = children.next().expect("children iterator over-reported length");
                    children_writer.push(child);
                }
                assert!(children.next().is_none(), "children iterator under-reported length");

                let text_len = children_writer.finish();
                ptr::write(raw.add(text_len_offset).cast(), text_len);
                debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
            })
        }
    }

    #[cfg(feature = "de")]
    #[allow(clippy::new_ret_no_self)]
    pub(super) fn try_new<A, I, E>(kind: Kind, mut children: I) -> Result<A, E>
    where
        A: TryAllocSliceDst<Self>,
        I: Iterator<Item = Result<PackedNodeOrToken, E>> + ExactSizeIterator,
    {
        let len = children.len();
        assert!(len <= u16::MAX as usize, "more children than fit in one node");
        let children_len = len as u16;
        let (layout, [children_len_offset, kind_offset, text_len_offset, children_offset]) =
            Self::layout(len);

        unsafe {
            // SAFETY: closure fully initializes the place
            A::try_new_slice_dst(len, |ptr| {
                let raw = ptr.as_ptr().cast::<u8>();

                ptr::write(raw.add(children_len_offset).cast(), children_len);
                ptr::write(raw.add(kind_offset).cast(), kind);

                let mut children_writer = ChildrenWriter::new(raw.add(children_offset).cast());
                for _ in 0..len {
                    let child = children.next().expect("children iterator over-reported length")?;
                    children_writer.push(child);
                }
                assert!(children.next().is_none(), "children iterator under-reported length");

                let text_len = children_writer.finish();
                ptr::write(raw.add(text_len_offset).cast(), text_len);
                debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
                Ok(())
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
