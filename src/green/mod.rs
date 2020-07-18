//! The green tree is an immutable, persistent, atomically reference counted tree.

mod builder;
mod children;
mod element;
mod node;
mod token;
mod tree_builder;

#[cfg(feature = "serde")]
mod serde;

pub(self) use self::element::{
    pack_node_or_token, unpack_node_or_token, Element, FullAlignedElement, HalfAlignedElement,
    PackedNodeOrToken,
};
#[doc(inline)]
pub use self::{
    builder::Builder,
    children::{Children, ChildrenWithOffsets},
    node::Node,
    token::Token,
    tree_builder::{Checkpoint, TreeBuilder},
};
