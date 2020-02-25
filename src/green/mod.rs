//! The green tree is an immutable, persistent, atomically reference counted tree.

mod token;
mod node;
mod element;
mod builder;

#[doc(inline)]
pub use self::{
    builder::{Builder, Checkpoint, TreeBuilder},
    node::{Children, Node},
    token::Token,
};
pub(self) use element::Element;
