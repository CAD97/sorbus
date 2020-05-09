use {
    crate::{
        green::{Node, Token},
        Kind, NodeOrToken,
    },
    serde::ser::*,
};

impl Serialize for Kind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("Kind", &self.0)
    }
}

impl Serialize for Token {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Token", 2)?;
        state.serialize_field("kind", &self.kind())?;
        state.serialize_field("text", &self.text())?;
        state.end()
    }
}

impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Node", 2)?;
        state.serialize_field("kind", &self.kind())?;
        state.serialize_field("children", &Children(self))?;
        state.end()
    }
}

struct Wrap<T>(T);

impl Serialize for Wrap<NodeOrToken<&Node, &Token>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            NodeOrToken::Node(node) => {
                let mut state = serializer.serialize_struct_variant("NodeOrToken", 0, "Node", 2)?;
                state.serialize_field("kind", &node.kind())?;
                state.serialize_field("children", &Children(node))?;
                state.end()
            }
            NodeOrToken::Token(token) => {
                let mut state =
                    serializer.serialize_struct_variant("NodeOrToken", 1, "Token", 2)?;
                state.serialize_field("kind", &token.kind())?;
                state.serialize_field("text", &token.text())?;
                state.end()
            }
        }
    }
}

struct Children<'a>(&'a Node);

impl Serialize for Children<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let children = self.0.children();
        let mut state = serializer.serialize_seq(Some(children.len()))?;
        for child in children {
            state.serialize_element(&Wrap(child.as_deref()))?;
        }
        state.end()
    }
}
