//! The green tree is an immutable, persistent, atomically reference counted tree.

mod token;
mod node;
mod element;
mod builder;
mod children;

#[cfg(feature = "serde")]
mod serde;

#[doc(inline)]
pub use self::{
    builder::{Builder, Checkpoint, TreeBuilder},
    children::{Children, ChildrenWithOffsets},
    node::Node,
    token::Token,
};
pub(self) use element::{
    pack_node_or_token, unpack_node_or_token, Element, FullAlignedElement, HalfAlignedElement,
    PackedNodeOrToken,
};
