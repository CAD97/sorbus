//! The green tree is an immutable, persistent, atomically reference counted tree.

mod token;
mod node;
mod element;
mod builder;
mod children;

#[doc(inline)]
pub use self::{
    builder::{Builder, Checkpoint, TreeBuilder},
    children::{Children, ChildrenWithOffsets},
    node::Node,
    token::Token,
};
pub(self) use element::{Element, FullAlignedElement, HalfAlignedElement};
