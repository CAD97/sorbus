use {
    crate::{
        green::{borrow_element, Element, Node, Token},
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
}

/// Children elements of a node in the immutable green tree,
/// with text offsets from the parent node.
///
/// This iterator is cheap to clone (basically a copy).
#[derive(Debug, Clone)]
pub struct ChildrenWithOffsets<'a> {
    cumulative_offset: TextSize,
    inner: Children<'a>,
}

impl<'a> Children<'a> {
    pub(super) fn new(elements: &'a [Element]) -> Self {
        Children { inner: elements.iter() }
    }
}

impl<'a> ChildrenWithOffsets<'a> {
    pub(super) fn new(elements: &'a [Element]) -> Self {
        ChildrenWithOffsets { cumulative_offset: 0.into(), inner: Children::new(elements) }
    }
}

impl<'a> Children<'a> {
    #[inline]
    /// Get the next value without advancing the iterator.
    pub fn peek(&self) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        let element = self.inner.as_slice().first()?;
        Some(borrow_element(element))
    }

    #[inline]
    /// Get the nth value without advancing the iterator.
    pub fn get(&self, n: usize) -> Option<NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>> {
        let element = self.inner.as_slice().get(n)?;
        Some(borrow_element(element))
    }

    #[inline]
    /// Split this iterator around some index.
    /// The first will contain all indices from `[0, mid)`
    /// and the second will contain all indices from `[mid, len)`.
    ///
    ///  # Panics
    ///
    /// Panics if `mid > len`.
    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let (left, right) = self.inner.as_slice().split_at(mid);
        (Self { inner: left.iter() }, Self { inner: right.iter() })
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let element = self.inner.next()?;
        Some(borrow_element(element))
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
        Some(borrow_element(element))
    }

    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.inner.fold(init, |b, item| f(b, borrow_element(item)))
    }
}

impl ExactSizeIterator for Children<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl DoubleEndedIterator for Children<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let element = self.inner.next_back()?;
        Some(borrow_element(element))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let element = self.inner.nth_back(n)?;
        Some(borrow_element(element))
    }
}

impl FusedIterator for Children<'_> {}

impl<'a> Iterator for ChildrenWithOffsets<'a> {
    type Item = (TextSize, NodeOrToken<ArcBorrow<'a, Node>, ArcBorrow<'a, Token>>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let element = self.inner.next()?;
        let offset = self.cumulative_offset;
        self.cumulative_offset += element.len();
        Some((offset, element))
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
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let mut cumulative_offset = self.cumulative_offset;
        self.inner.fold(init, move |b, item| {
            let offset = cumulative_offset;
            cumulative_offset += item.len();
            f(b, (offset, item))
        })
    }
}
