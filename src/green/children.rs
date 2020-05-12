use {
    crate::{
        green::{Element, Node, Token},
        ArcBorrow, NodeOrToken, TextSize,
    },
    std::{iter::FusedIterator, slice},
};

/// Children elements of a node in the immutable green tree.
///
/// This iterator is cheap to clone (basically a copy),
/// and random access (`get` or `nth`) is constant time.
#[derive(Debug, Clone)]
pub struct Children<'a> {
    inner: slice::Iter<'a, Element>,
    full_align: bool,
}

/// Children elements of a node in the immutable green tree,
/// with text offsets from the parent node.
///
/// This iterator is cheap to clone (basically a copy),
/// and random access (`get` or `nth`) is constant time.
#[derive(Debug, Clone)]
pub struct ChildrenWithOffsets<'a> {
    inner: slice::Iter<'a, Element>,
    full_align: bool,
}

impl<'a> Children<'a> {
    pub(super) unsafe fn new(elements: &'a [Element]) -> Self {
        assert!(elements.first().map(Element::is_full_aligned).unwrap_or(true));
        Children { inner: elements.iter(), full_align: true }
    }
}

macro_rules! impl_children_iter {
    ($T:ident of $Item:ty) => {
        impl<'a> $T<'a> {
            /// Get the next item in the iterator without advancing it.
            #[inline]
            pub fn peek(&self) -> Option<<Self as Iterator>::Item> {
                let element = self.inner.as_slice().first()?;
                if self.full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }

            /// Get the nth item in the iterator without advancing it.
            #[inline]
            pub fn get(&self, n: usize) -> Option<<Self as Iterator>::Item> {
                let element = self.inner.as_slice().get(n)?;
                let full_align = self.full_align ^ (n % 2 == 1);
                if full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }

            /// Divide this iterator into two at an index.
            ///
            /// The first will contain all indices from `[0, mid)`,
            /// and the second will contain all indices from `[mid, len)`.
            ///
            /// # Panics
            ///
            /// Panics if `mid > len`.
            #[inline]
            pub fn split_at(&self, mid: usize) -> (Self, Self) {
                let (left, right) = self.inner.as_slice().split_at(mid);
                let left_full_align = self.full_align;
                let right_full_align = self.full_align ^ (mid % 2 == 1);
                (
                    Self { inner: left.iter(), full_align: left_full_align },
                    Self { inner: right.iter(), full_align: right_full_align },
                )
            }
        }

        impl<'a> Iterator for $T<'a> {
            type Item = $Item;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                let element = self.inner.next()?;
                let full_align = self.full_align;
                self.full_align = !full_align;
                if full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.inner.size_hint()
            }

            #[inline]
            fn count(self) -> usize {
                self.inner.count()
            }

            #[inline]
            fn last(mut self) -> Option<Self::Item> {
                self.next_back()
            }

            #[inline]
            fn nth(&mut self, n: usize) -> Option<Self::Item> {
                let element = self.inner.nth(n)?;
                let full_align = self.full_align ^ (n % 2 == 1);
                self.full_align = !full_align;
                if full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }

            #[inline]
            fn fold<B, F>(mut self, init: B, mut f: F) -> B
            where
                F: FnMut(B, Self::Item) -> B,
            {
                let mut accum = init;

                macro_rules! next {
                    ($aligned:ident) => {
                        if let Some(element) = self.inner.next() {
                            unsafe { element.$aligned().into() }
                        } else {
                            break accum;
                        }
                    };
                }

                if self.full_align {
                    loop {
                        let el = next!(full_aligned);
                        accum = f(accum, el);
                        let el = next!(half_aligned);
                        accum = f(accum, el);
                    }
                } else {
                    loop {
                        let el = next!(half_aligned);
                        accum = f(accum, el);
                        let el = next!(full_aligned);
                        accum = f(accum, el);
                    }
                }
            }
        }

        impl ExactSizeIterator for $T<'_> {
            #[inline]
            fn len(&self) -> usize {
                self.inner.len()
            }
        }

        impl DoubleEndedIterator for $T<'_> {
            #[inline]
            fn next_back(&mut self) -> Option<Self::Item> {
                let element = self.inner.next_back()?;
                // self.len() is now the index of the element popped from the back
                let full_align = self.full_align ^ (self.len() % 2 == 1);
                // don't change self.full_align, the alignment of the head
                if full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }

            #[inline]
            fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
                let element = self.inner.nth_back(n)?;
                // self.len() is now the index of the element popped from the back
                let full_align = self.full_align ^ (self.len() % 2 == 1);
                // don't change self.full_align, the alignment of the head
                if full_align {
                    unsafe { Some(element.full_aligned().into()) }
                } else {
                    unsafe { Some(element.half_aligned().into()) }
                }
            }
        }

        impl FusedIterator for $T<'_> {}
    };
}

impl_children_iter!(Children of NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>);
impl_children_iter!(ChildrenWithOffsets of (TextSize, NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>));

impl<'a> Children<'a> {
    /// Iterate the children with their offsets from the parent node.
    #[inline]
    pub fn with_offsets(&self) -> ChildrenWithOffsets<'a> {
        ChildrenWithOffsets { inner: self.inner.clone(), full_align: self.full_align }
    }
}

impl<'a> ChildrenWithOffsets<'a> {
    /// Iterate the children without their offsets.
    #[inline]
    pub fn without_offsets(&self) -> Children<'a> {
        Children { inner: self.inner.clone(), full_align: self.full_align }
    }
}
