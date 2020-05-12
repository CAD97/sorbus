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
pub(super) type PackedNodeOrToken = Union2<Arc<Node>, Arc<Token>>;
pub(super) fn pack_node_or_token(el: NodeOrToken<Arc<Node>, Arc<Token>>) -> PackedNodeOrToken {
    match el {
        NodeOrToken::Node(node) => ARC_UNION_PROOF.a(node),
        NodeOrToken::Token(token) => ARC_UNION_PROOF.b(token),
    }
}
pub(super) fn unpack_node_or_token(el: PackedNodeOrToken) -> NodeOrToken<Arc<Node>, Arc<Token>> {
    match el.unpack() {
        Enum2::A(node) => NodeOrToken::Node(node),
        Enum2::B(token) => NodeOrToken::Token(token),
    }
}

/// # Safety
///
/// - On a 64 bit target:
///   - If aligned to 8 bytes, must be `.full_aligned`.
///   - If aligned to 8 bytes + 4, must be `.half_aligned`.
/// - On a 32 bit target:
///   - Must always be aligned to 8 bytes, even though this is overaligned.
///   - Both `.full_aligned` and `.half_aligned` alias the same implementation.
///
/// To avoid leaks, the proper member must be `take`n from.
#[repr(C, align(4))]
pub(super) union Element {
    full_aligned: FullAlignedElementRepr,
    half_aligned: HalfAlignedElementRepr,
}

// SAFETY: Element is logically a (TextSize, PackedNodeOrToken)
// ptr-union is a private dependency, so we assert Union2 send/sync separately
unsafe impl Send for Element {}
unsafe impl Sync for Element {}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<(TextSize, PackedNodeOrToken)>();
};

/// # Safety
///
/// - Must be aligned to 8 bytes (usize, u64)
/// - This is only Copy because of requirements for `union`;
///   logically this is a `(TextSize, PackedNodeOrToken)`,
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
///   logically this is a `(TextSize, PackedNodeOrToken)`,
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
        unsafe {
            match (self.is_full_aligned(), other.is_full_aligned()) {
                (true, true) => self.full_aligned() == other.full_aligned(),
                (true, false) => self.full_aligned() == other.half_aligned(),
                (false, true) => self.half_aligned() == other.half_aligned(),
                (false, false) => self.half_aligned() == other.half_aligned(),
            }
        }
    }
}

impl Hash for Element {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        if self.is_full_aligned() {
            unsafe { self.full_aligned().hash(state) }
        } else {
            unsafe { self.half_aligned().hash(state) }
        }
    }
}

macro_rules! impl_element {
    ($Element:ident) => {
        impl $Element {
            #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
            pub(super) fn ptr(&self) -> Union2<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
                unsafe { ErasablePtr::unerase(*&self.repr.ptr) }
            }

            #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
            pub(super) fn offset(&self) -> TextSize {
                unsafe { *&self.repr.offset }
            }

            #[allow(clippy::deref_addrof)] // tell rustc that it's aligned
            pub(super) unsafe fn take(&mut self) -> PackedNodeOrToken {
                ErasablePtr::unerase(*&self.repr.ptr)
            }
        }

        impl Eq for $Element {}
        impl PartialEq<FullAlignedElement> for $Element {
            fn eq(&self, other: &FullAlignedElement) -> bool {
                self.ptr() == other.ptr() && self.offset() == other.offset()
            }
        }

        #[cfg(target_pointer_width = "64")]
        impl PartialEq<HalfAlignedElement> for $Element {
            fn eq(&self, other: &HalfAlignedElement) -> bool {
                self.ptr() == other.ptr() && self.offset() == other.offset()
            }
        }

        impl Hash for $Element {
            fn hash<H: hash::Hasher>(&self, state: &mut H) {
                self.ptr().hash(state);
                self.offset().hash(state);
            }
        }

        impl<'a> From<&'a $Element> for NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
            fn from(this: &'a $Element) -> Self {
                let this = this.ptr();
                None.or_else(|| this.with_a(|&node| NodeOrToken::Node(node)))
                    .or_else(|| this.with_b(|&token| NodeOrToken::Token(token)))
                    .unwrap()
            }
        }

        impl<'a> From<&'a $Element>
            for (TextSize, NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>)
        {
            fn from(this: &'a $Element) -> Self {
                (this.offset(), this.into())
            }
        }
    };
}

impl_element!(FullAlignedElement);
#[cfg(target_pointer_width = "64")]
impl_element!(HalfAlignedElement);

impl FullAlignedElement {
    pub(super) unsafe fn write(ptr: *mut Element, element: PackedNodeOrToken, offset: TextSize) {
        debug_assert!(
            ptr as usize % 8 == 0,
            "attempted to write full-aligned element to half-aligned place; this is UB!",
        );
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, element);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, offset);
    }
}

#[cfg(target_pointer_width = "64")]
impl HalfAlignedElement {
    pub(super) unsafe fn write(ptr: *mut Element, element: PackedNodeOrToken, offset: TextSize) {
        debug_assert!(
            ptr as usize % 8 == 4,
            "attempted to write half-aligned element to full-aligned place; this is UB!",
        );
        let element = ErasablePtr::erase(element);
        let ptr = ptr.cast();
        ptr::write(ptr, offset);
        let ptr = ptr.add(1).cast();
        ptr::write(ptr, element);
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

impl<'a> From<&'a Element> for (TextSize, NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>) {
    fn from(this: &'a Element) -> Self {
        if this.is_full_aligned() {
            unsafe { this.full_aligned().into() }
        } else {
            unsafe { this.half_aligned().into() }
        }
    }
}
