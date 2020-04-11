//! Child element of a green node, being a textual offset from the parent and
//! either `Arc<green::Node>` or `Arc<green::Token>`.
//!
//! To achieve optimal packing, the order of the `TextSize` and the `Arc` is
//! determined by the alignment of the `Element`. A naive implementation would
//! have `Element` as `(NodeOrToken<Arc<Node>, Arc<Token>>, TextSize)`.
//!
//! The first optimization is by using [`Union2`]. This erases the node/token
//! pointer to be thin (a single `usize` small) and packs the union of both
//! pointers into a single pointer's space by tagging in the alignment bits.
//!
//! The second optimization is noting that `(Ptr, TextSize)` has a `u32` of
//! padding. By representing `Element` as padding-free `(usize, u32)` or
//! `(u32, usize)` depending on alignment, we can eliminate this padding
//! without sacrificing alignment of any member of the `Element` pair.

// Yes, some of this complexity could be turned off on 32 bit platforms, as the
// alignment requirements of usize and u32 are the same. However, it is simpler
// to maintain the same layout algorithm on 32 bit and 64 bit platforms.

use {
    crate::{
        green::{Node, Token},
        ArcBorrow, NodeOrToken, TextSize,
    },
    erasable::{ErasablePtr, ErasedPtr},
    ptr_union::{Enum2, Union2, UnionBuilder},
    std::{
        fmt,
        ptr,
        hash::{self, Hash},
        mem::{self, ManuallyDrop},
        sync::Arc,
    },
    text_size::TextLen,
};

// SAFETY: align of Node and Token are >= 2
const ARC_UNION_PROOF: UnionBuilder<Union2<Arc<Node>, Arc<Token>>> =
    unsafe { UnionBuilder::new2() };

/// # Safety
///
/// - If aligned to 8 bytes, must be `.full_aligned`
/// - If aligned to 8 bytes + 4, must be `.half_aligned`
#[repr(align(4))]
pub(super) union Element {
    full_aligned: FullAlignedElementRepr,
    half_aligned: HalfAlignedElementRepr,
}

/// # Safety
///
/// - Must be aligned to 8 bytes (usize)
/// - This is only Copy because of requirements for `union`;
///   logically this is a `(Union2<Arc<Node>, Arc<Token>>, TextSize)`,
///   and must be cloned as such.
#[derive(Copy, Clone)] // required for union
#[repr(C, packed)]
struct FullAlignedElementRepr {
    ptr: ErasedPtr,
    offset: TextSize,
}

/// # Safety
///
/// Must be aligned to 8 bytes (usize).
///
/// An improperly aligned element may only exist as long as is required to
/// write it to an aligned location; no methods may be used until the element
/// has been properly aligned.
#[repr(transparent)]
pub(super) struct FullAlignedElement {
    repr: FullAlignedElementRepr,
}

/// # Safety
///
/// - Must be aligned to 8 bytes + 4 (usize + 1/2).
///   (That is, aligned to 4 but not 8.)
/// - This is only Copy because of requirements for `union`;
///   logically this is a `(Union2<Arc<Node>, Arc<Token>>, TextSize)`,
///   and must be cloned as such.
#[derive(Copy, Clone)] // required for union
#[repr(C, packed)]
struct HalfAlignedElementRepr {
    offset: TextSize,
    ptr: ErasedPtr,
}

/// # Safety
///
/// Must be aligned to 8 bytes + 4 (usize + 1/2).
/// (That is, aligned to 4 but not 8.)
///
/// An improperly aligned element may only exist as long as is required to
/// write it to an aligned location; no methods may be used until the element
/// has been properly aligned.
#[repr(transparent)]
pub(super) struct HalfAlignedElement {
    repr: HalfAlignedElementRepr,
}

impl Element {
    pub(super) fn is_full_aligned(&self) -> bool {
        self as *const Self as usize % 8 == 0
    }

    pub(super) unsafe fn full_aligned(&self) -> &FullAlignedElement {
        debug_assert!(self.is_full_aligned());
        &*(&self.full_aligned as *const FullAlignedElementRepr as *const FullAlignedElement)
    }

    pub(super) fn is_half_aligned(&self) -> bool {
        self as *const Self as usize % 8 == 4
    }

    pub(super) unsafe fn half_aligned(&self) -> &HalfAlignedElement {
        debug_assert!(self.is_half_aligned());
        &*(&self.half_aligned as *const HalfAlignedElementRepr as *const HalfAlignedElement)
    }

    pub(super) fn ptr(&self) -> Union2<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
        if self.is_full_aligned() {
            unsafe { self.full_aligned().ptr() }
        } else {
            unsafe { self.half_aligned().ptr() }
        }
    }

    pub(super) fn offset(&self) -> TextSize {
        if self.is_full_aligned() {
            unsafe { self.full_aligned().offset() }
        } else {
            unsafe { self.half_aligned().offset() }
        }
    }
}

impl FullAlignedElement {
    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    pub(super) fn ptr(&self) -> Union2<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
        unsafe { ErasablePtr::unerase(*&self.repr.ptr) }
    }

    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    pub(super) fn offset(&self) -> TextSize {
        unsafe { *&self.repr.offset }
    }

    pub(super) unsafe fn write(
        ptr: *mut Element,
        element: NodeOrToken<Arc<Node>, Arc<Token>>,
        offset: TextSize,
    ) {
        debug_assert!(ptr as usize % 8 == 0);
        let element =
            element.map(|node| ARC_UNION_PROOF.a(node), |token| ARC_UNION_PROOF.b(token)).flatten();
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, element);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, offset);
    }
}

impl HalfAlignedElement {
    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    pub(super) fn ptr(&self) -> Union2<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
        unsafe { ErasablePtr::unerase(*&self.repr.ptr) }
    }

    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    pub(super) fn offset(&self) -> TextSize {
        unsafe { *&self.repr.offset }
    }

    pub(super) unsafe fn write(
        ptr: *mut Element,
        element: NodeOrToken<Arc<Node>, Arc<Token>>,
        offset: TextSize,
    ) {
        debug_assert!(ptr as usize % 8 == 4);
        let element =
            element.map(|node| ARC_UNION_PROOF.a(node), |token| ARC_UNION_PROOF.b(token)).flatten();
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, offset);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, element);
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{Element}")
    }
}

impl Eq for Element {}
impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}

impl PartialEq for FullAlignedElement {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}
impl PartialEq<HalfAlignedElement> for FullAlignedElement {
    fn eq(&self, other: &HalfAlignedElement) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}

impl PartialEq for HalfAlignedElement {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}
impl PartialEq<FullAlignedElement> for HalfAlignedElement {
    fn eq(&self, other: &FullAlignedElement) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}

impl Hash for Element {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
        self.offset().hash(state);
    }
}

impl Hash for FullAlignedElement {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
        self.offset().hash(state);
    }
}

impl Hash for HalfAlignedElement {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
        self.offset().hash(state);
    }
}

impl<'a> From<&'a Element> for NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
    fn from(this: &'a Element) -> Self {
        let this = this.ptr();
        None.or_else(|| this.with_a(|&node| NodeOrToken::Node(node)))
            .or_else(|| this.with_b(|&token| NodeOrToken::Token(token)))
            .unwrap()
    }
}

impl<'a> From<&'a FullAlignedElement> for NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
    fn from(this: &'a FullAlignedElement) -> Self {
        let this = this.ptr();
        None.or_else(|| this.with_a(|&node| NodeOrToken::Node(node)))
            .or_else(|| this.with_b(|&token| NodeOrToken::Token(token)))
            .unwrap()
    }
}

impl<'a> From<&'a HalfAlignedElement> for NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
    fn from(this: &'a HalfAlignedElement) -> Self {
        let this = this.ptr();
        None.or_else(|| this.with_a(|&node| NodeOrToken::Node(node)))
            .or_else(|| this.with_b(|&token| NodeOrToken::Token(token)))
            .unwrap()
    }
}
