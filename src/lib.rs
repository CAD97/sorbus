//! A generic library for lossless syntax trees.
//!
//! This library is a reimplementation of ideas from [rowan],
//! with aggressive optimization for size.
//!
//! The name "sorbus" is the genus of the rowan tree.
//!
//!   [rowan]: <lib.rs/rowan>

#![feature(vec_drain_as_slice)]
#![forbid(unconditional_recursion)]
#![warn(missing_debug_implementations, missing_docs)]

#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("sorbus only works when sizeof(*const ()) is u32 or u64");

#[allow(unused)]
const ASSERT_TEXTSIZE_IS_U32: fn() = || {
    let _ = std::mem::transmute::<u32, text_size::TextSize>;
};

pub mod green;
mod utils;
mod layout_polyfill;

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

#[test]
fn test_send_sync() {
    use std::sync::Arc;
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Arc<green::Node>>();
    assert_send_sync::<Arc<green::Token>>();
}

pub(crate) use as_slice_trait::AsSlice;
mod as_slice_trait {
    use std::vec;

    pub trait AsSlice<T> {
        fn as_slice(&self) -> &[T];
    }

    impl<T> AsSlice<T> for vec::IntoIter<T> {
        fn as_slice(&self) -> &[T] {
            self.as_slice()
        }
    }

    impl<T> AsSlice<T> for vec::Drain<'_, T> {
        fn as_slice(&self) -> &[T] {
            self.as_slice()
        }
    }
}
