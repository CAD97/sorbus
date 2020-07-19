use {
    crate::{
        green::{pack_node_or_token, Node, PackedNodeOrToken, Token},
        ArcBorrow, Kind, NodeOrToken,
    },
    erasable::{ErasablePtr, ErasedPtr},
    hashbrown::{hash_map::RawEntryMut, HashMap},
    std::{
        fmt,
        hash::{BuildHasher, Hash, Hasher},
        ptr,
        sync::Arc,
    },
};

fn erased_children<'a, I: 'a>(
    children: I,
) -> impl 'a + Iterator<Item = ErasedPtr> + ExactSizeIterator
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Into<NodeOrToken<&'a Node, &'a Token>>,
{
    children.into_iter().map(|el| el.into().map(ErasablePtr::erase, ErasablePtr::erase).flatten())
}

fn thin_node_eq(
    node: &Node,
    kind: Kind,
    children: impl Iterator<Item = ErasedPtr> + ExactSizeIterator,
) -> bool {
    node.kind() == kind && erased_children(node.children()).eq(children)
}

fn thin_node_hash(
    hasher: &impl BuildHasher,
    kind: Kind,
    children: impl Iterator<Item = ErasedPtr>,
) -> u64 {
    let state = &mut hasher.build_hasher();
    kind.hash(state);
    for child in children {
        ptr::hash(child.as_ptr(), state);
    }
    state.finish()
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
    nodes: HashMap<Arc<Node>, (), ()>,
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

fn do_hash(hasher: &impl BuildHasher, hashee: &(impl ?Sized + Hash)) -> u64 {
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
    pub fn node<I, R>(&mut self, kind: Kind, children: I) -> Arc<Node>
    where
        I: IntoIterator,
        I::Item: Into<NodeOrToken<Arc<Node>, Arc<Token>>>,
        I::IntoIter: ExactSizeIterator + AsRef<[R]>,
        for<'a> &'a R: Into<NodeOrToken<&'a Node, &'a Token>>,
    {
        let hasher = &self.hasher;
        let children = children.into_iter();

        let hash = thin_node_hash(hasher, kind, erased_children(children.as_ref()));

        let entry = self
            .nodes
            .raw_entry_mut()
            .from_hash(hash, |node| thin_node_eq(node, kind, erased_children(children.as_ref())));

        let (node, ()) = match entry {
            RawEntryMut::Occupied(entry) => entry.into_key_value(),
            RawEntryMut::Vacant(entry) => {
                let node = Node::new(kind, children.map(Into::into).map(pack_node_or_token));
                entry.insert_with_hasher(hash, node, (), |node| {
                    thin_node_hash(hasher, node.kind(), erased_children(node.children()))
                })
            }
        };

        Arc::clone(node)
    }

    /// Version of `Builder::node` taking a pre-packed child element iterator.
    pub(super) fn node_packed<I>(&mut self, kind: Kind, children: I) -> Arc<Node>
    where
        I: Iterator<Item = PackedNodeOrToken> + ExactSizeIterator + AsRef<[PackedNodeOrToken]>,
    {
        let hasher = &self.hasher;

        let hash = thin_node_hash(
            hasher,
            kind,
            children.as_ref().iter().map(PackedNodeOrToken::as_untagged_ptr),
        );

        let entry = self.nodes.raw_entry_mut().from_hash(hash, |node| {
            thin_node_eq(
                node,
                kind,
                children.as_ref().iter().map(PackedNodeOrToken::as_untagged_ptr),
            )
        });

        let (node, ()) = match entry {
            RawEntryMut::Occupied(entry) => entry.into_key_value(),
            RawEntryMut::Vacant(entry) => {
                let node = Node::new(kind, children);
                entry.insert_with_hasher(hash, node, (), |node| {
                    thin_node_hash(hasher, node.kind(), erased_children(node.children()))
                })
            }
        };

        Arc::clone(node)
    }

    /// Get a cached version of the input node.
    ///
    /// If the node is new to this cache, store it and return a clone.
    /// If it's already in the cache, return a clone of the cached version.
    #[cfg(feature = "de")]
    pub(super) fn cache_node(&mut self, node: Arc<Node>) -> Arc<Node> {
        let hasher = &self.hasher;

        let hash = thin_node_hash(hasher, node.kind(), erased_children(node.children()));

        let entry = self
            .nodes
            .raw_entry_mut()
            .from_hash(hash, |x| thin_node_eq(x, node.kind(), erased_children(node.children())));

        let (node, ()) = match entry {
            RawEntryMut::Occupied(entry) => entry.into_key_value(),
            RawEntryMut::Vacant(entry) => entry.insert_with_hasher(hash, node, (), |node| {
                thin_node_hash(hasher, node.kind(), erased_children(node.children()))
            }),
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
    fn collect_root_nodes(&mut self) -> Vec<Arc<Node>> {
        // NB: `drain_filter` is `retain` but with an iterator of the removed elements.
        // i.e.: elements where the predicate is FALSE are removed and iterated over.
        self.nodes
            .drain_filter(|node, ()| Arc::strong_count(node) > 1)
            .map(|(node, _)| node)
            .collect()
    }

    fn collect_tokens(&mut self) {
        self.tokens.retain(|token, ()| Arc::strong_count(token) > 1)
    }

    /// Collect all cached nodes that are no longer live outside the cache.
    pub fn gc(&mut self) {
        let mut to_drop = self.collect_root_nodes();
        let Builder { hasher, nodes, .. } = self;

        while let Some(node) = to_drop.pop() {
            if Arc::strong_count(&node) <= 2 {
                // queue children for (potential) removal from the cache
                for child in node.children() {
                    if let Some(node) = child.into_node() {
                        to_drop.push(ArcBorrow::upgrade(node));
                    }
                }

                // remove this node from the cache
                let hash = thin_node_hash(hasher, node.kind(), erased_children(node.children()));
                let entry = nodes.raw_entry_mut().from_hash(hash, |x| {
                    thin_node_eq(x, node.kind(), erased_children(node.children()))
                });
                if let RawEntryMut::Occupied(entry) = entry {
                    entry.remove();
                }
            }
        }
        self.collect_tokens();
    }
}
