//! A generic library for lossless syntax trees.
//!
//! This library is a reimplementation of ideas from [rowan],
//! with aggressive optimization for size.
//!
//! The name "sorbus" is the genus of the rowan tree.
//!
//!   [rowan]: <lib.rs/rowan>

#![feature(alloc_layout_extra)] // rust-lang/rust#69362, hopefully targeting 1.43
#![feature(assoc_int_consts)] // rust-lang/rust#68952, hopefully targeting 1.43

#![forbid(unconditional_recursion)]
#![warn(missing_debug_implementations, missing_docs)]

use static_assertions::*;
assert_cfg!(not(target_pointer_width = "16"), "sorbus currently assumes u32 fits in usize");

pub mod green;
mod utils;

#[doc(inline)]
pub use crate::utils::{Kind, NodeOrToken};
#[doc(no_inline)]
pub use {
    rc_borrow::ArcBorrow,
    str_index::{StrIndex, StrRange},
};

/// Reexports of commonly used types.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        green::{Node as GreenNode, Token as GreenToken},
        Kind, NodeOrToken,
    };
}
