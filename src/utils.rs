use {
    crate::prelude::{GreenNode, GreenToken},
    std::{ops::Deref, sync::Arc},
    erasable::ErasablePtr,
};

/// Raw kind tag for each element in the tree.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Kind(pub u16);

/// Enum wrapping either a node or a token.
#[allow(missing_docs)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub enum NodeOrToken<Node, Token> {
    Node(Node),
    Token(Token),
}

#[allow(missing_docs)]
impl<Node, Token> NodeOrToken<Node, Token> {
    pub fn into_node(self) -> Option<Node> {
        self.map(Some, |_| None).flatten()
    }

    pub fn as_node(&self) -> Option<&Node> {
        self.as_ref().into_node()
    }

    pub fn is_node(&self) -> bool {
        self.as_node().is_some()
    }

    pub fn unwrap_node(self) -> Node {
        self.into_node().expect("called `unwrap_node` on token")
    }

    pub fn into_token(self) -> Option<Token> {
        self.map(|_| None, Some).flatten()
    }

    pub fn as_token(&self) -> Option<&Token> {
        self.as_ref().into_token()
    }

    pub fn is_token(&self) -> bool {
        self.as_token().is_some()
    }

    pub fn unwrap_token(self) -> Token {
        self.into_token().expect("called `unwrap_token` on node")
    }
}

#[allow(missing_docs)]
impl<Node, Token> NodeOrToken<Node, Token> {
    pub fn as_ref(&self) -> NodeOrToken<&Node, &Token> {
        match *self {
            NodeOrToken::Node(ref node) => NodeOrToken::Node(node),
            NodeOrToken::Token(ref token) => NodeOrToken::Token(token),
        }
    }

    pub(crate) fn map<N, T>(
        self,
        n: impl FnOnce(Node) -> N,
        t: impl FnOnce(Token) -> T,
    ) -> NodeOrToken<N, T> {
        match self {
            NodeOrToken::Node(node) => NodeOrToken::Node(n(node)),
            NodeOrToken::Token(token) => NodeOrToken::Token(t(token)),
        }
    }

    pub fn as_deref(&self) -> NodeOrToken<&Node::Target, &Token::Target>
    where
        Node: Deref,
        Token: Deref,
    {
        self.as_ref().map(Deref::deref, Deref::deref)
    }
}

impl<T> NodeOrToken<T, T> {
    pub(crate) fn flatten(self) -> T {
        match self {
            NodeOrToken::Node(node) => node,
            NodeOrToken::Token(token) => token,
        }
    }
}

impl From<Arc<GreenNode>> for NodeOrToken<Arc<GreenNode>, Arc<GreenToken>> {
    fn from(this: Arc<GreenNode>) -> Self {
        NodeOrToken::Node(this)
    }
}

impl From<Arc<GreenToken>> for NodeOrToken<Arc<GreenNode>, Arc<GreenToken>> {
    fn from(this: Arc<GreenToken>) -> Self {
        NodeOrToken::Token(this)
    }
}
