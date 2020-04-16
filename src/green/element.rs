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
//!
//! On 32-bit platforms, where pointers have the same size as `TextSize`,
//! this half-alignment trick is not required (and doesn't work). Instead,
//! we support 32-bit platforms by silently aliasing the half aligned element
//! to a full aligned element, because it is always fully aligned.

use {
    crate::{
        green::{Node, Token},
        ArcBorrow, NodeOrToken, TextSize,
    },
    erasable::{ErasablePtr, ErasedPtr},
    ptr_union::{Builder2, Enum2, Union2},
    std::{
        fmt::{self, Debug},
        hash::{self, Hash},
        ptr,
        sync::Arc,
    },
};

// SAFETY: align of Node and Token are >= 2
const ARC_UNION_PROOF: Builder2<Arc<Node>, Arc<Token>> = unsafe { Builder2::new_unchecked() };

/// # Safety
///
/// - On a 64 bit target
///   - If aligned to 8 bytes, must be `.full_aligned`
///   - If aligned to 8 bytes + 4, must be `.half_aligned`
/// - On a 32 bit target
///   - Must always be aligned to 8 bytes, even though this strictly overaligns
///   - Both `.full_aligned` and `.half_aligned` alias the same implementation
///
/// To avoid leaks, must be dropped via drop_in_place-ing the correct member.
#[repr(C, align(4))]
pub(super) union Element {
    full_aligned: FullAlignedElementRepr,
    half_aligned: HalfAlignedElementRepr,
}

/// # Safety
///
/// - Must be aligned to 8 bytes (usize, u64)
/// - This is only Copy because of requirements for `union`;
///   logically this is a `(Union2<Arc<Node>, Arc<Token>>, TextSize)`,
///   and must be treated as such.
#[derive(Copy, Clone)] // required for union
#[repr(C, packed)]
struct FullAlignedElementRepr {
    ptr: ErasedPtr,
    offset: TextSize,
}

/// # Safety
///
/// Must be aligned to 8 bytes (usize, u64).
#[repr(transparent)]
pub(super) struct FullAlignedElement {
    repr: FullAlignedElementRepr,
}

/// # Safety
///
/// - Must be aligned to 8 bytes + 4 (usize, u64 + 1/2).
///   (That is, aligned to 4 but not 8.)
/// - This is only Copy because of requirements for `union`;
///   logically this is a `(Union2<Arc<Node>, Arc<Token>>, TextSize)`,
///   and must be treated as such.
#[derive(Copy, Clone)] // required for union
#[repr(C, packed)]
#[cfg(target_pointer_width = "64")]
struct HalfAlignedElementRepr {
    offset: TextSize,
    ptr: ErasedPtr,
}

#[cfg(target_pointer_width = "32")]
type HalfAlignedElementRepr = FullAlignedElementRepr;

/// # Safety
///
/// Must be aligned to 8 bytes + 4 (usize + 1/2).
/// (That is, aligned to 4 but not 8.)
#[repr(transparent)]
#[cfg(target_pointer_width = "64")]
pub(super) struct HalfAlignedElement {
    repr: HalfAlignedElementRepr,
}

#[cfg(target_pointer_width = "32")]
pub(super) type HalfAlignedElement = FullAlignedElement;

impl Element {
    #[cfg(target_pointer_width = "64")]
    pub(super) fn is_full_aligned(&self) -> bool {
        self as *const Self as usize % 8 == 0
    }

    #[cfg(target_pointer_width = "32")]
    pub(super) fn is_full_aligned(&self) -> bool {
        true
    }

    pub(super) unsafe fn full_aligned(&self) -> &FullAlignedElement {
        debug_assert!(
            self.is_full_aligned(),
            "called Element::full_aligned on half-aligned element; this is UB!",
        );
        &*(&self.full_aligned as *const FullAlignedElementRepr as *const FullAlignedElement)
    }

    pub(super) unsafe fn full_aligned_mut(&mut self) -> &mut FullAlignedElement {
        debug_assert!(
            self.is_full_aligned(),
            "called Element::full_aligned on half-aligned element; this is UB!",
        );
        &mut *(&mut self.full_aligned as *mut FullAlignedElementRepr as *mut FullAlignedElement)
    }

    #[cfg(target_pointer_width = "64")]
    pub(super) fn is_half_aligned(&self) -> bool {
        self as *const Self as usize % 8 == 4
    }

    #[cfg(target_pointer_width = "32")]
    pub(super) fn is_half_aligned(&self) -> bool {
        self.is_full_aligned()
    }

    pub(super) unsafe fn half_aligned(&self) -> &HalfAlignedElement {
        debug_assert!(
            self.is_half_aligned(),
            "called Element::half_aligned on full-aligned element; this is UB!",
        );
        &*(&self.half_aligned as *const HalfAlignedElementRepr as *const HalfAlignedElement)
    }

    pub(super) unsafe fn half_aligned_mut(&mut self) -> &mut HalfAlignedElement {
        debug_assert!(
            self.is_half_aligned(),
            "called Element::half_aligned on full-aligned element; this is UB!",
        );
        &mut *(&mut self.half_aligned as *mut HalfAlignedElementRepr as *mut HalfAlignedElement)
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
        debug_assert!(
            ptr as usize % 8 == 0,
            "attempted to write full-aligned element to half-aligned place; this is UB!",
        );
        let element =
            element.map(|node| ARC_UNION_PROOF.a(node), |token| ARC_UNION_PROOF.b(token)).flatten();
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, element);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, offset);
    }
}

#[cfg(target_pointer_width = "64")]
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
        debug_assert!(
            ptr as usize % 8 == 4,
            "attempted to write half-aligned element to full-aligned place; this is UB!",
        );
        let element =
            element.map(|node| ARC_UNION_PROOF.a(node), |token| ARC_UNION_PROOF.b(token)).flatten();
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, offset);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, element);
    }
}

impl Drop for FullAlignedElement {
    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    fn drop(&mut self) {
        debug_assert!(
            self as *const _ as usize % 8 == 0,
            "dropped a half-aligned element as a full-aligned element; this is UB!",
        );
        unsafe {
            <Union2<Arc<Node>, Arc<Token>> as ErasablePtr>::unerase(*&self.repr.ptr);
        }
    }
}

#[cfg(target_pointer_width = "64")]
impl Drop for HalfAlignedElement {
    #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
    fn drop(&mut self) {
        debug_assert!(
            self as *const _ as usize % 8 == 4,
            "dropped a full-aligned element as a half-aligned element; this is UB!",
        );
        unsafe {
            <Union2<Arc<Node>, Arc<Token>> as ErasablePtr>::unerase(*&self.repr.ptr);
        }
    }
}

impl Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(offset {:?}) ", &self.offset())?;
        match self.ptr().unpack() {
            Enum2::A(node) => Debug::fmt(&node, f),
            Enum2::B(token) => Debug::fmt(&token, f),
        }
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
#[cfg(target_pointer_width = "64")]
impl PartialEq<HalfAlignedElement> for FullAlignedElement {
    fn eq(&self, other: &HalfAlignedElement) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}

#[cfg(target_pointer_width = "64")]
impl PartialEq for HalfAlignedElement {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr() && self.offset() == other.offset()
    }
}
#[cfg(target_pointer_width = "64")]
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

#[cfg(target_pointer_width = "64")]
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

#[cfg(target_pointer_width = "64")]
impl<'a> From<&'a HalfAlignedElement> for NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
    fn from(this: &'a HalfAlignedElement) -> Self {
        let this = this.ptr();
        None.or_else(|| this.with_a(|&node| NodeOrToken::Node(node)))
            .or_else(|| this.with_b(|&token| NodeOrToken::Token(token)))
            .unwrap()
    }
}
