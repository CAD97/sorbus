use {
    crate::{
        green::{Node, Token},
        Kind, NodeOrToken, TextSize,
    },
    hashbrown::{hash_map::RawEntryMut, HashMap},
    slice_dst::{AllocSliceDst, TryAllocSliceDst},
    std::{
        hash::{BuildHasher, Hash, Hasher},
        ptr,
        sync::Arc,
    },
};

#[derive(Debug, Clone)]
struct ThinEqNode(Arc<Node>);

// SAFETY: pass-through implementation
unsafe impl AllocSliceDst<Node> for ThinEqNode {
    unsafe fn new_slice_dst<I>(len: usize, init: I) -> Self
    where
        I: FnOnce(ptr::NonNull<Node>),
    {
        ThinEqNode(Arc::new_slice_dst(len, init))
    }
}
// SAFETY: pass-through implementation
unsafe impl TryAllocSliceDst<Node> for ThinEqNode {
    unsafe fn try_new_slice_dst<I, E>(len: usize, init: I) -> Result<Self, E>
    where
        I: FnOnce(ptr::NonNull<Node>) -> Result<(), E>,
    {
        Arc::try_new_slice_dst(len, init).map(ThinEqNode)
    }
}

impl From<Arc<Node>> for ThinEqNode {
    fn from(this: Arc<Node>) -> Self {
        ThinEqNode(this)
    }
}

impl Eq for ThinEqNode {}
impl PartialEq for ThinEqNode {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
            && self.0.len() == other.0.len()
            && self.0.children().zip(other.0.children()).all(|pair| match pair {
                (NodeOrToken::Node(lhs), NodeOrToken::Node(rhs)) => ptr::eq(&*lhs, &*rhs),
                (NodeOrToken::Token(lhs), NodeOrToken::Token(rhs)) => lhs == rhs,
                _ => false,
            })
    }
}

impl Hash for ThinEqNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.kind().hash(state);
        self.0.len().hash(state);
        for child in self.0.children() {
            match child {
                NodeOrToken::Node(node) => ptr::hash(&*node, state),
                NodeOrToken::Token(token) => token.hash(state),
            }
        }
    }
}

/// Construction cache for green tree elements.
///
/// As the green tree is immutable, identical nodes can be deduplicated.
/// For example, all nodes representing the `#[inline]` attribute can
/// be deduplicated and refer to the same green node in memory,
/// despite their distribution throughout the source code.
#[derive(Debug, Default, Clone)]
pub struct Builder {
    nodes: HashMap<ThinEqNode, ()>,
    tokens: HashMap<Arc<Token>, ()>,
}

impl Builder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new node or clone a new Arc to an existing equivalent one.
    ///
    /// This checks children for identity equivalence, not structural,
    /// so it is `O(children.len())` and only caches higher-level nodes
    /// if the lower-level nodes have also been cached.
    pub fn node<I>(&mut self, kind: Kind, children: I) -> Arc<Node>
    where
        I: IntoIterator,
        I::Item: Into<NodeOrToken<Arc<Node>, Arc<Token>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let node = Node::new(kind, children.into_iter().map(Into::into));
        self.insert_node(node)
    }

    pub(super) fn node_from_vec(
        &mut self,
        kind: Kind,
        children: Vec<NodeOrToken<Arc<Node>, Arc<Token>>>,
    ) -> Arc<Node> {
        let text_len: TextSize =
            children.iter().map(|el| el.as_deref().map(Node::len, Token::len).flatten()).sum();
        let hash = {
            // spoof the hash
            let mut h = self.nodes.hasher().build_hasher();
            kind.hash(&mut h);
            text_len.hash(&mut h);
            for child in &children {
                match child {
                    NodeOrToken::Node(node) => ptr::hash(&*node, &mut h),
                    NodeOrToken::Token(token) => token.hash(&mut h),
                }
            }
            h.finish()
        };
        self.nodes
            .raw_entry_mut()
            .from_hash(hash, |node| {
                node.0.kind() == kind
                    && node.0.len() == text_len
                    && node.0.children().zip(children.iter()).all(|pair| match pair {
                        (NodeOrToken::Node(lhs), NodeOrToken::Node(rhs)) => ptr::eq(&*lhs, &**rhs),
                        (NodeOrToken::Token(lhs), NodeOrToken::Token(rhs)) => lhs == *rhs,
                        _ => false,
                    })
            })
            .or_insert_with(|| (Node::new(kind, children), ()))
            .0
             .0
            .clone()
    }

    pub(super) fn insert_node(&mut self, node: Arc<Node>) -> Arc<Node> {
        let node = ThinEqNode(node);
        self.nodes.raw_entry_mut().from_key(&node).or_insert(node, ()).0 .0.clone()
    }

    /// Create a new token or clone a new Arc to an existing equivalent one.
    pub fn token(&mut self, kind: Kind, text: &str) -> Arc<Token> {
        let hash = {
            // spoof Token's hash impl
            let mut hasher = self.tokens.hasher().build_hasher();
            kind.hash(&mut hasher);
            text.hash(&mut hasher);
            hasher.finish()
        };

        let entry = self
            .tokens
            .raw_entry_mut()
            .from_hash(hash, |token| token.kind() == kind && token.text() == text);

        match entry {
            RawEntryMut::Occupied(entry) => entry.key().clone(),
            RawEntryMut::Vacant(entry) => {
                let (token, ()) = entry.insert_hashed_nocheck(hash, Token::new(kind, text), ());
                token.clone()
            }
        }
    }
}

/// Checkpoint for maybe wrapping a node. See [`TreeBuilder::checkpoint`].
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Checkpoint(usize);

/// Top-down builder context for a green tree.
#[derive(Debug, Default)]
pub struct TreeBuilder {
    cache: Builder,
    stack: Vec<(Kind, usize)>,
    children: Vec<NodeOrToken<Arc<Node>, Arc<Token>>>,
}

impl TreeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new builder, reusing a `Builder` cache.
    pub fn new_with(cache: Builder) -> Self {
        TreeBuilder { cache, ..Self::default() }
    }

    /// The `Builder` used to create and deduplicate nodes.
    pub fn builder(&mut self) -> &mut Builder {
        &mut self.cache
    }

    /// Add an element to the current branch.
    pub fn add(&mut self, element: impl Into<NodeOrToken<Arc<Node>, Arc<Token>>>) -> &mut Self {
        self.children.push(element.into());
        self
    }

    /// Add a new token to the current branch.
    pub fn token(&mut self, kind: Kind, text: &str) -> &mut Self {
        let token = self.cache.token(kind, text);
        self.add(token)
    }

    /// Add a new node to the current branch.
    pub fn node<I>(&mut self, kind: Kind, children: I) -> &mut Self
    where
        I: IntoIterator,
        I::Item: Into<NodeOrToken<Arc<Node>, Arc<Token>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let node = self.cache.node(kind, children);
        self.add(node)
    }

    /// Start a new child node and make it the current branch.
    pub fn start_node(&mut self, kind: Kind) -> &mut Self {
        self.stack.push((kind, self.children.len()));
        self
    }

    /// Finish the current branch and restore its parent as current.
    pub fn finish_node(&mut self) -> &mut Self {
        let (kind, first_child) = self.stack.pop().unwrap_or_else(|| {
            panic!("called `TreeBuilder::finish_node` without paired `start_node`")
        });
        let children = self.children.drain(first_child..);
        let node = self.cache.node(kind, children);
        self.add(node)
    }

    /// Prepare for maybe wrapping the next node.
    ///
    /// To potentially wrap elements into a node, first create a checkpoint,
    /// add all items that might be wrapped, then maybe call `start_node_at`.
    ///
    /// # Examples
    ///
    //  TODO: Example
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.children.len())
    }

    /// Wrap the elements added after `checkpoint` in a new node,
    /// and make the new node the current branch.
    pub fn start_node_at(&mut self, Checkpoint(checkpoint): Checkpoint, kind: Kind) -> &mut Self {
        assert!(
            checkpoint <= self.children.len(),
            "checkpoint no longer valid; was `finish_node` called early?",
        );

        if let Some(&(_, first_child)) = self.stack.last() {
            assert!(
                checkpoint >= first_child,
                "checkpoint no longer valid; was an unmatched `start_node` called?",
            )
        };

        self.stack.push((kind, checkpoint));
        self
    }

    /// Complete the current tree building.
    ///
    /// This `TreeBuilder` is reset and can be used to build a new tree.
    ///
    /// # Panics
    ///
    /// Panics if more nodes have been started than finished,
    /// or the current branch has more than one element.
    pub fn finish(&mut self) -> Arc<Node> {
        assert!(self.stack.is_empty());
        assert_eq!(self.children.len(), 1);
        self.children.pop().unwrap().into_node().unwrap()
    }

    /// Destroy this tree builder and recycle its build cache.
    pub fn recycle(self) -> Builder {
        self.cache
    }
}
