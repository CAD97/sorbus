//! The green tree is an immutable, persistent, atomically reference counted tree.

mod builder;
mod children;
mod element;
mod node;
mod token;
mod tree_builder;

#[cfg(feature = "serde")]
mod serde;

pub(self) use self::element::{borrow_element, pack_element, unpack_element, Element};
#[doc(inline)]
pub use self::{
    builder::Builder,
    children::{Children, ChildrenWithOffsets},
    node::Node,
    token::Token,
    tree_builder::{Checkpoint, TreeBuilder},
};
