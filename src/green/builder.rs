use {
    crate::{
        green::{pack_node_or_token, Node, PackedNodeOrToken, Token},
        ArcBorrow, Kind, NodeOrToken,
    },
    hashbrown::{
        hash_map::{HashMap, RawEntryMut},
        raw::{Bucket, RawTable},
    },
    rc_box::ArcBox,
    scopeguard::{guard, ScopeGuard},
    std::{
        convert::TryFrom,
        fmt,
        hash::{BuildHasher, Hash, Hasher},
        mem, ptr,
        sync::Arc,
    },
};

fn thin_node_eq(this: &Node, that: &Node) -> bool {
    // we can skip `len` as it is derived from `children`
    this.kind() == that.kind()
        && this.children().zip(that.children()).all(|pair| match pair {
            (NodeOrToken::Node(lhs), NodeOrToken::Node(rhs)) => ptr::eq(&*lhs, &*rhs),
            (NodeOrToken::Token(lhs), NodeOrToken::Token(rhs)) => ptr::eq(&*lhs, &*rhs),
            _ => false,
        })
}

fn thin_node_hash(this: &Node, hasher: &impl BuildHasher) -> u64 {
    let state = &mut hasher.build_hasher();
    // we can skip `len` as it is derived from `children`
    this.kind().hash(state);
    for child in this.children() {
        match child {
            NodeOrToken::Node(node) => ptr::hash(&*node, state),
            NodeOrToken::Token(token) => ptr::hash(&*token, state),
        }
    }
    state.finish()
}

/// Construction cache for green tree elements.
///
/// As the green tree is immutable, identical nodes can be deduplicated.
/// For example, all nodes representing the `#[inline]` attribute can
/// be deduplicated and refer to the same green node in memory,
/// despite their distribution throughout the source code.
#[derive(Clone)]
pub struct Builder {
    hasher: ahash::RandomState, // dedupe the 2×u64 hasher state and enforce custom hashing
    nodes: RawTable<Arc<Node>>,
    tokens: HashMap<Arc<Token>, (), ()>,
}

impl Default for Builder {
    fn default() -> Self {
        Builder { hasher: Default::default(), nodes: RawTable::new(), tokens: Default::default() }
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // save space in nonexpanded view
        if f.alternate() {
            todo!()
        // f.debug_struct("Builder")
        //     .field("nodes", &self.nodes)
        //     .field("tokens", &self.tokens)
        //     .finish()
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
        let Builder { hasher, nodes, .. } = self;

        let hash = thin_node_hash(&node, hasher);
        let bucket = nodes
            .find(hash, |x| thin_node_eq(x, &node))
            .unwrap_or_else(|| nodes.insert(hash, node, |x| thin_node_hash(x, hasher)));
        unsafe { Arc::clone(bucket.as_ref()) }
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
    fn gc_nodes(&mut self) {
        // WARN: this is evil concurrent modification of the table while iterating it.
        // I'm not sure that this kind of concurrent modification/iteration is allowed, so
        // this should definitely should get reviewed by someone familiar with hashbrown.
        let Builder { hasher, nodes, .. } = self;

        let mut to_drop = vec![];
        let mut iter = unsafe { nodes.iter() };

        fn cleanup(
            bucket: Bucket<Arc<Node>>,
            nodes: &mut RawTable<Arc<Node>>,
            to_drop: &mut Vec<Arc<Node>>,
        ) {
            unsafe {
                let guard = guard((), |()| nodes.erase_no_drop(&bucket));
                match ArcBox::<Node>::try_from(bucket.read()) {
                    // cache is final owner, drop node and potentially drop its children
                    Ok(node) => {
                        for child in node.children() {
                            if let Some(node) = child.into_node() {
                                to_drop.push(ArcBorrow::upgrade(node));
                            }
                        }
                    }
                    // node is still live, keep it in the cache
                    Err(node) => {
                        mem::forget(node);
                        ScopeGuard::into_inner(guard);
                    }
                }
            }
        }

        #[allow(clippy::while_let_on_iterator)] // we're doing questionable things
        while let Some(bucket) = iter.next() {
            let index = unsafe { nodes.bucket_index(&bucket) };
            cleanup(bucket, nodes, &mut to_drop);

            while let Some(node) = to_drop.pop() {
                let hash = thin_node_hash(&node, hasher);
                if let Some(bucket) = nodes.find(hash, |x| thin_node_eq(x, &node)) {
                    // Only collect nodes "behind" the iterator head. Nodes "ahead"
                    // of the iterator head will be collected from the iterator.
                    // This allows us to not invalidate the iterator.
                    if unsafe { nodes.bucket_index(&bucket) < index } {
                        drop(node);
                        cleanup(bucket, nodes, &mut to_drop);
                    }
                }
            }
        }
    }

    fn gc_tokens(&mut self) {
        self.tokens.retain(|token, ()| Arc::strong_count(token) > 1)
    }

    /// Collect all cached elements that are no longer live outside the cache.
    pub fn gc(&mut self) {
        self.gc_nodes();
        self.gc_tokens();
    }
}
