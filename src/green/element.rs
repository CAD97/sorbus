//! Support routines for working with packed element references.
//!
//! We can pack an enumeration of `Arc<Node>` and `Arc<Token>` into a single `usize`;
//! this is what is done here, along with functions to pack and unpack the pointers.

use {
    crate::{
        green::{Node, Token},
        ArcBorrow, NodeOrToken,
    },
    ptr_union::{Builder2, Enum2, Union2},
    std::{mem, sync::Arc},
};

// SAFETY: align of Node and Token are >= 2
const ARC_UNION_PROOF: Builder2<Arc<Node>, Arc<Token>> = unsafe { Builder2::new_unchecked() };

pub(super) type Element = Union2<Arc<Node>, Arc<Token>>;

pub(super) fn pack_element(el: NodeOrToken<Arc<Node>, Arc<Token>>) -> Element {
    match el {
        NodeOrToken::Node(node) => ARC_UNION_PROOF.a(node),
        NodeOrToken::Token(token) => ARC_UNION_PROOF.b(token),
    }
}

pub(super) fn unpack_element(el: Element) -> NodeOrToken<Arc<Node>, Arc<Token>> {
    match el.unpack() {
        Enum2::A(node) => NodeOrToken::Node(node),
        Enum2::B(token) => NodeOrToken::Token(token),
    }
}

pub(super) fn borrow_element<'a>(
    el: &'a Element,
) -> NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> {
    // SAFETY: @CAD97: I wrote these libraries; Arc/ArcBorrow neccesarily erase to the same ptr;
    //                 Union2 stores the tagged erased ptr; Arc->Borrow is just a ptr erase.
    let borrow: Union2<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>> =
        unsafe { mem::transmute_copy(el) };
    match borrow.unpack() {
        Enum2::A(node) => NodeOrToken::Node(node),
        Enum2::B(token) => NodeOrToken::Token(token),
    }
}
