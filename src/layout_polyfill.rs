//! Polyfill for rust-lang/rust#69362
#![allow(unstable_name_collisions)] // LayoutPolyfill, be aware!

use std::{
    alloc::{Layout, LayoutErr},
    cmp, mem,
};

fn layout_err() -> LayoutErr {
    Layout::from_size_align(0, 0).unwrap_err()
}

pub(crate) trait LayoutPolyfill {
    fn align_to(&self, align: usize) -> Result<Layout, LayoutErr>;
    fn padding_needed_for(&self, align: usize) -> usize;
    fn pad_to_align(&self) -> Layout;
    fn repeat(&self, n: usize) -> Result<(Layout, usize), LayoutErr>;
    fn extend(&self, next: Layout) -> Result<(Layout, usize), LayoutErr>;
    fn array<T>(n: usize) -> Result<Layout, LayoutErr>;
}

impl LayoutPolyfill for Layout {
    fn align_to(&self, align: usize) -> Result<Self, LayoutErr> {
        Layout::from_size_align(self.size(), cmp::max(self.align(), align))
    }

    fn padding_needed_for(&self, align: usize) -> usize {
        let len = self.size();

        let len_rounded_up = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
        len_rounded_up.wrapping_sub(len)
    }

    fn pad_to_align(&self) -> Layout {
        let pad = self.padding_needed_for(self.align());
        let new_size = self.size() + pad;

        Layout::from_size_align(new_size, self.align()).unwrap()
    }

    fn repeat(&self, n: usize) -> Result<(Self, usize), LayoutErr> {
        let padded_size = self
            .size()
            .checked_add(self.padding_needed_for(self.align()))
            .ok_or_else(layout_err)?;
        let alloc_size = padded_size.checked_mul(n).ok_or_else(layout_err)?;

        unsafe { Ok((Layout::from_size_align_unchecked(alloc_size, self.align()), padded_size)) }
    }

    fn extend(&self, next: Self) -> Result<(Self, usize), LayoutErr> {
        let new_align = cmp::max(self.align(), next.align());
        let pad = self.padding_needed_for(next.align());

        let offset = self.size().checked_add(pad).ok_or_else(layout_err)?;
        let new_size = offset.checked_add(next.size()).ok_or_else(layout_err)?;

        let layout = Layout::from_size_align(new_size, new_align)?;
        Ok((layout, offset))
    }

    fn array<T>(n: usize) -> Result<Self, LayoutErr> {
        let (layout, offset) = Layout::new::<T>().repeat(n)?;
        debug_assert_eq!(offset, mem::size_of::<T>());
        Ok(layout.pad_to_align())
    }
}
