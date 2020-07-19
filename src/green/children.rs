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
///
/// # Performance note
///
/// Internal iteration (`fold`, `for_each`; the callback iteration APIs)
/// is much more performant than external iteration (`for`, `next`)
/// for this iterator: up to 20-30% better (depending on how you measure)
/// for straight-line children iteration, and approximately 10% better for a
/// more usual tree visitor pattern that descends into transitive children.
#[derive(Debug, Clone)]
pub struct Children<'a> {
    inner: slice::Iter<'a, Element>,
}

/// Children elements of a node in the immutable green tree,
/// with text offsets from the parent node.
///
/// This iterator is cheap to clone (basically a copy),
/// and random access (`get` or `nth`) is constant time.
///
/// # Performance note
///
/// Internal iteration (`fold`, `for_each`; the callback iteration APIs)
/// is much more performant than external iteration (`for`, `next`)
/// for this iterator: up to 20-30% better (depending on how you measure)
/// for straight-line children iteration, and approximately 10% better for a
/// more usual tree visitor pattern that descends into transitive children.
#[derive(Debug, Clone)]
pub struct ChildrenWithOffsets<'a> {
    inner: slice::Iter<'a, Element>,
}

impl<'a> Children<'a> {
    pub(super) unsafe fn new(elements: &'a [Element]) -> Self {
        Children { inner: elements.iter() }
    }
}

macro_rules! impl_children_iter {
    ($T:ident of $Item:ty) => {
        impl<'a> $T<'a> {
            /// Get the next item in the iterator without advancing it.
            #[inline]
            pub fn peek(&self) -> Option<<Self as Iterator>::Item> {
                let element = self.inner.as_slice().first()?;
                Some(element.into())
            }

            /// Get the nth item in the iterator without advancing it.
            #[inline]
            pub fn get(&self, n: usize) -> Option<<Self as Iterator>::Item> {
                let element = self.inner.as_slice().get(n)?;
                Some(element.into())
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
                (Self { inner: left.iter() }, Self { inner: right.iter() })
            }
        }

        impl<'a> Iterator for $T<'a> {
            type Item = $Item;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                let element = self.inner.next()?;
                Some(element.into())
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
                Some(element.into())
            }

            #[inline]
            fn fold<B, F>(mut self, init: B, mut f: F) -> B
            where
                F: FnMut(B, Self::Item) -> B,
            {
                // Nota Bene: this is performance-sensitive iteration code.
                // Seemingly minor refactorings can have large impacts on the
                // throughput of this code; make sure to run the benchmarks!

                // For example, changing the below loop from
                //    if condition { B(); }
                //    loop { A(); B(); }
                // to the current (as of PR#37, and this comment)
                //    if condition { loop { B(); A(); } }
                //    else         { loop { A(); B(); } }
                // led to a 10%(!) difference in benchmark throughput.

                let mut accum = init;

                let mut el;
                macro_rules! next {
                    () => {
                        if let Some(element) = self.inner.next() {
                            el = element;
                        } else {
                            return accum;
                        }
                    };
                }

                next!();
                unsafe {
                    if el.is_half_aligned() {
                        loop {
                            accum = f(accum, el.half_aligned().into());
                            next!();
                            accum = f(accum, el.full_aligned().into());
                            next!();
                        }
                    } else {
                        loop {
                            accum = f(accum, el.full_aligned().into());
                            next!();
                            accum = f(accum, el.half_aligned().into());
                            next!();
                        }
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
                Some(element.into())
            }

            #[inline]
            fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
                let element = self.inner.nth_back(n)?;
                Some(element.into())
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
        ChildrenWithOffsets { inner: self.inner.clone() }
    }
}

impl<'a> ChildrenWithOffsets<'a> {
    /// Iterate the children without their offsets.
    #[inline]
    pub fn without_offsets(&self) -> Children<'a> {
        Children { inner: self.inner.clone() }
    }
}
