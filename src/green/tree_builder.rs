use {
    crate::{
        green::{
            pack_node_or_token, unpack_node_or_token, Builder, Node, PackedNodeOrToken, Token,
        },
        Kind, NodeOrToken,
    },
    std::{hash::Hash, sync::Arc},
};

/// Checkpoint for maybe wrapping a node. See [`TreeBuilder::checkpoint`].
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Checkpoint(usize);

/// Top-down builder context for a green tree.
#[derive(Debug, Default)]
pub struct TreeBuilder {
    cache: Builder,
    stack: Vec<(Kind, usize)>,
    children: Vec<PackedNodeOrToken>,
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
        self.children.push(pack_node_or_token(element.into()));
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
        // NB: inline Self::node here because of borrow on `self.children`
        let node = self.cache.node_packed(kind, children);
        self.add(node)
    }

    /// Prepare for maybe wrapping the next node.
    ///
    /// To potentially wrap elements into a node, first create a checkpoint,
    /// add some items that might be wrapped, then maybe call `start_node_at`.
    /// Don't forget to still call [`finish_node`] for the newly started node!
    ///
    ///   [`finish_node`]: TreeBuilder::finish_node
    ///
    /// # Examples
    ///
    /// Checkpoints can be used to implement [pratt parsing]:
    ///
    ///   [pratt parsing]: <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    ///
    /// ```rust
    /// # use {sorbus::{green::*, Kind, Kind as Token}, std::iter::Peekable};
    /// # const ATOM: Kind = Kind(0); const EXPR: Kind = Kind(4);
    /// # const PLUS: Kind = Kind(1); const MUL: Kind = Kind(2);
    /// # fn binding_power(kind: &Token) -> f32 { kind.0 as f32 }
    /// # fn text_of(kind: Kind) -> &'static str { match kind {
    /// #     ATOM => "atom", PLUS => "+", MUL => "*", _ => panic!(),
    /// # } }
    /// # fn kind_of(kind: Kind) -> Kind { kind }
    /// fn parse_expr(b: &mut TreeBuilder, bind: f32, tts: &mut Peekable<impl Iterator<Item=Token>>) {
    ///     let start = b.checkpoint();
    ///     // just assume the tokens are correct for the example
    ///     let first_token = tts.next().unwrap();
    ///     b.token(kind_of(first_token), text_of(first_token));
    ///     loop {
    ///         let power = match tts.peek() {
    ///             None => break,
    ///             Some(op) => binding_power(op),
    ///         };
    ///         if power < bind { break; }
    ///         let op_token = tts.next().unwrap();
    ///         b.token(kind_of(op_token), text_of(op_token));
    ///         parse_expr(&mut *b, power, &mut *tts);
    ///         b.start_node_at(start, EXPR).finish_node();
    ///     }
    /// }
    ///
    /// let tokens = vec![ATOM, MUL, ATOM, PLUS, ATOM, MUL, ATOM]; // atom*atom+atom*atom
    /// # let mut builder = TreeBuilder::new();
    /// let expected_tree = builder
    ///     .start_node(EXPR)
    ///         .start_node(EXPR)
    ///             .token(ATOM, "atom")
    ///             .token(MUL, "*")
    ///             .token(ATOM, "atom")
    ///         .finish_node()
    ///         .token(PLUS, "+")
    ///         .start_node(EXPR)
    ///             .token(ATOM, "atom")
    ///             .token(MUL, "*")
    ///             .token(ATOM, "atom")
    ///         .finish_node()
    ///     .finish_node()
    ///     .finish();
    /// parse_expr(&mut builder, 0.0, &mut tokens.into_iter().peekable());
    /// let parsed_tree = builder.finish();
    /// assert_eq!(parsed_tree, expected_tree);
    /// ```
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

    /// Finish the current branch up to a given checkpoint,
    /// and restore its parent as current.
    ///
    /// Any nodes after the used checkpoint will be shifted from the
    /// current branch to its parent, after the newly finished node.
    ///
    /// Prefer using regular `finish_node` and delaying adding branches
    /// when possible, as its operations on the underlying buffer are
    /// marginally more efficient and involve less moving of elements.
    pub fn finish_node_at(&mut self, Checkpoint(checkpoint): Checkpoint) -> &mut Self {
        assert!(
            checkpoint <= self.children.len(),
            "checkpoint no longer valid; was `finish_node` called early?",
        );

        let (kind, first_child) = self.stack.pop().unwrap_or_else(|| {
            panic!("called `TreeBuilder::finish_node_at` without paired `start_node`")
        });
        assert!(
            checkpoint >= first_child,
            "checkpoint no longer valid; was an unmatched `start_node` called?",
        );
        let children = self.children.drain(first_child..checkpoint);
        // NB: inline Self::node here because of borrow on `self.children`
        let node = self.cache.node_packed(kind, children);
        self.add(node)
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
        unpack_node_or_token(self.children.pop().unwrap()).into_node().unwrap()
    }

    /// Destroy this tree builder and recycle its build cache.
    pub fn recycle(self) -> Builder {
        self.cache
    }
}
