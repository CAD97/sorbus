//! A generic library for lossless syntax trees.
//!
//! This library is a reimplementation of ideas from [rowan],
//! with aggressive optimization for size.
//!
//! The name "sorbus" is the genus of the rowan tree.
//!
//!   [rowan]: <lib.rs/rowan>

#![feature(alloc_layout_extra)] // rust-lang/rust#69362, hopefully targeting 1.44
#![forbid(unconditional_recursion)]
#![warn(missing_debug_implementations, missing_docs)]

pub mod green;
mod utils;

#[doc(inline)]
pub use crate::utils::{Kind, NodeOrToken};
#[doc(no_inline)]
pub use {
    rc_borrow::ArcBorrow,
    text_size::{TextRange, TextSize},
};

/// Reexports of commonly used types.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        green::{Node as GreenNode, Token as GreenToken},
        Kind, NodeOrToken,
    };
}
