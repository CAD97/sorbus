use {
    crate::{
        green::{pack_node_or_token, Node, PackedNodeOrToken, Token},
        Kind, NodeOrToken,
    },
    hashbrown::{hash_map::RawEntryMut, HashMap},
    std::{
        fmt,
        hash::{BuildHasher, Hash, Hasher},
        ptr,
        sync::Arc,
    },
};

#[derive(Debug, Clone)]
struct ThinEqNode(Arc<Node>);

impl Eq for ThinEqNode {}
impl PartialEq for ThinEqNode {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
            // we can skip `len` as it is derived from `children`
            && self.0.children().zip(other.0.children()).all(|pair| match pair {
                (NodeOrToken::Node(lhs), NodeOrToken::Node(rhs)) => ptr::eq(&*lhs, &*rhs),
                (NodeOrToken::Token(lhs), NodeOrToken::Token(rhs)) => ptr::eq(&*lhs, &*rhs),
                _ => false,
            })
    }
}

impl Hash for ThinEqNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.kind().hash(state);
        // we can skip `len` as it is derived from `children`
        for child in self.0.children() {
            match child {
                NodeOrToken::Node(node) => ptr::hash(&*node, state),
                NodeOrToken::Token(token) => ptr::hash(&*token, state),
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
#[derive(Default, Clone)]
pub struct Builder {
    hasher: ahash::RandomState, // dedupe the 2×u64 hasher state and enforce custom hashing
    nodes: HashMap<ThinEqNode, (), ()>,
    tokens: HashMap<Arc<Token>, (), ()>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // save space in nonexpanded view
        if f.alternate() {
            f.debug_struct("Builder")
                .field("nodes", &self.nodes)
                .field("tokens", &self.tokens)
                .finish()
        } else {
            f.debug_struct("Builder")
                .field("nodes", &format_args!("{} cached", self.nodes.len()))
                .field("tokens", &format_args!("{} cached", self.tokens.len()))
                .finish()
        }
    }
}

fn do_hash(hasher: &impl BuildHasher, hashee: &impl Hash) -> u64 {
    let state = &mut hasher.build_hasher();
    hashee.hash(state);
    state.finish()
}

impl Builder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of cached elements.
    pub fn size(&self) -> usize {
        self.nodes.len() + self.tokens.len()
    }
}

impl Builder {
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
        self.node_packed(kind, children.into_iter().map(Into::into).map(pack_node_or_token))
    }

    /// Version of `Builder::node` taking a pre-packed child element iterator.
    pub(super) fn node_packed<I>(&mut self, kind: Kind, children: I) -> Arc<Node>
    where
        I: Iterator<Item = PackedNodeOrToken> + ExactSizeIterator,
    {
        let node = Node::new(kind, children);
        self.cache_node(node)
    }

    /// Get a cached version of the input node.
    ///
    /// If the node is new to this cache, store it and return a clone.
    /// If it's already in the cache, return a clone of the cached version.
    pub(super) fn cache_node(&mut self, node: Arc<Node>) -> Arc<Node> {
        let hasher = &self.hasher;
        let node = ThinEqNode(node);

        let entry =
            self.nodes.raw_entry_mut().from_key_hashed_nocheck(do_hash(hasher, &node), &node);
        let (ThinEqNode(node), ()) = match entry {
            RawEntryMut::Occupied(entry) => entry.into_key_value(),
            RawEntryMut::Vacant(entry) => {
                entry.insert_with_hasher(do_hash(hasher, &node), node, (), |x| do_hash(hasher, x))
            }
        };
        Arc::clone(node)
    }

    /// Create a new token or clone a new Arc to an existing equivalent one.
    pub fn token(&mut self, kind: Kind, text: &str) -> Arc<Token> {
        let hasher = &self.hasher;

        let hash = {
            // spoof Token's hash impl
            let state = &mut hasher.build_hasher();
            kind.hash(state);
            text.hash(state);
            state.finish()
        };

        let entry = self
            .tokens
            .raw_entry_mut()
            .from_hash(hash, |token| token.kind() == kind && token.text() == text);
        let (token, ()) = match entry {
            RawEntryMut::Occupied(entry) => entry.into_key_value(),
            RawEntryMut::Vacant(entry) => {
                entry.insert_with_hasher(hash, Token::new(kind, text), (), |x| do_hash(hasher, x))
            }
        };
        Arc::clone(token)
    }
}

impl Builder {
    fn turn_node_gc(&mut self) -> bool {
        // NB: `drain_filter` is `retain` but with an iterator of the removed elements.
        // i.e.: elements where the predicate is FALSE are removed and iterated over.
        self.nodes.drain_filter(|ThinEqNode(node), ()| Arc::strong_count(node) > 1).any(|_| true)
    }

    fn turn_token_gc(&mut self) -> bool {
        self.tokens.drain_filter(|token, ()| Arc::strong_count(token) > 1).any(|_| true)
    }

    /// Collect cached nodes that are no longer live outside the cache.
    ///
    /// This is a single turn of the GC, and may not GC all potentially unused
    /// nodes in the cache. To run this to a fixpoint, use [`Builder::gc`].
    pub fn turn_gc(&mut self) -> bool {
        let removed_nodes = self.turn_node_gc();
        let removed_tokens = self.turn_token_gc();
        removed_nodes || removed_tokens
    }

    /// Collect all cached nodes that are no longer live outside the cache.
    ///
    /// This is slightly more efficient than just running [`Builder::turn_gc`]
    /// to a fixpoint, as it knows more about the cache structure and can avoid
    /// re-GCing definitely clean sections.
    pub fn gc(&mut self) {
        // Nodes can keep other elements live, so GC them to a fixpoint
        while self.turn_node_gc() {
            continue;
        }
        // Tokens are guaranteed leaves, so only need a single GC turn
        self.turn_token_gc();
        debug_assert!(!self.turn_token_gc());
    }
}
