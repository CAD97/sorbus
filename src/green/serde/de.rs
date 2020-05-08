#![allow(clippy::try_err)]

extern crate serde; // this line required to workaround rust-lang/rust#55779

use {
    crate::{
        green::{Builder, Node, Token},
        Kind, NodeOrToken,
    },
    serde::{de::*, Deserialize},
    std::{borrow::Cow, fmt, ops::Deref, sync::Arc},
};

/// Helper type to maybe borrow a string from the deserializer.
///
/// There are three possible cases:
///
/// - We hint the deserializer that we can take ownership of the string, by
///   calling `deserialize_string`. If it has an owned string that it's
///   otherwise going to throw away, it can give it to us, and we take it.
/// - Otherwise, we get a borrowed string.
///   - If the string can be zero-copy deserialized, we borrow it from the deserializer.
///   - Otherwise, we copy it into an owned string just such that we can continue deserialization.
struct Str<'a>(Cow<'a, str>);

impl Deref for Str<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Str<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrVisitor;

        impl<'de> Visitor<'de> for StrVisitor {
            type Value = Str<'de>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Str(Cow::Owned(v.into())))
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Str(Cow::Borrowed(v)))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Str(Cow::Owned(v)))
            }
        }

        deserializer.deserialize_string(StrVisitor)
    }
}

impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename = "Kind")]
        struct Repr(u16);

        let Repr(raw) = Repr::deserialize(deserializer)?;
        Ok(Kind(raw))
    }
}

impl Builder {
    /// Deserialize a token using this cache.
    pub fn deserialize_token(
        &mut self,
    ) -> impl for<'de> DeserializeSeed<'de, Value = Arc<Token>> + '_ {
        TokenSeed(self)
    }

    /// Deserialize a node using this cache.
    pub fn deserialize_node(
        &mut self,
    ) -> impl for<'de> DeserializeSeed<'de, Value = Arc<Node>> + '_ {
        NodeSeed(self)
    }
}

struct TokenSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for TokenSeed<'_> {
    type Value = Arc<Token>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kind,
            Text,
        }

        impl<'de> Visitor<'de> for TokenSeed<'_> {
            type Value = Arc<Token>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sorbus green token")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let kind = seq.next_element()?.ok_or_else(|| Error::invalid_length(0, &self))?;
                let token = seq
                    .next_element_seed(TokenSeedKind(self.0, kind))?
                    .ok_or_else(|| Error::invalid_length(1, &self))?;
                Ok(token)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                use VisitState::*;
                enum VisitState<'de> {
                    Start,
                    WithKind(Kind),
                    WithText(Str<'de>),
                    Finish(Arc<Token>),
                }

                let mut state = Start;
                while let Some(key) = map.next_key()? {
                    state = match (key, state) {
                        (Field::Kind, Start) => WithKind(map.next_value()?),
                        (Field::Text, Start) => WithText(map.next_value()?),

                        (Field::Kind, WithText(text)) => {
                            Finish(self.0.token(map.next_value()?, &text))
                        }
                        (Field::Text, WithKind(kind)) => {
                            Finish(map.next_value_seed(TokenSeedKind(self.0, kind))?)
                        }

                        (Field::Kind, WithKind(_)) => Err(Error::duplicate_field("kind"))?,
                        (Field::Kind, Finish(_)) => Err(Error::duplicate_field("kind"))?,
                        (Field::Text, WithText(_)) => Err(Error::duplicate_field("text"))?,
                        (Field::Text, Finish(_)) => Err(Error::duplicate_field("text"))?,
                    }
                }

                match state {
                    VisitState::Start => Err(Error::missing_field("kind")),
                    VisitState::WithText(_) => Err(Error::missing_field("kind")),
                    VisitState::WithKind(_) => Err(Error::missing_field("text")),
                    VisitState::Finish(token) => Ok(token),
                }
            }
        }

        const FIELDS: &[&str] = &["kind", "text"];
        deserializer.deserialize_struct("Token", FIELDS, self)
    }
}

struct TokenSeedKind<'a>(&'a mut Builder, Kind);
impl<'de> DeserializeSeed<'de> for TokenSeedKind<'_> {
    type Value = Arc<Token>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TokenTextVisitor<'a>(&'a mut Builder, Kind);
        impl<'de, 'a> Visitor<'de> for TokenTextVisitor<'a> {
            type Value = Arc<Token>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(self.0.token(self.1, v))
            }
        }
        deserializer.deserialize_str(TokenTextVisitor(self.0, self.1))
    }
}

struct NodeSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for NodeSeed<'_> {
    type Value = Arc<Node>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kind,
            Children,
        }

        impl<'de> Visitor<'de> for NodeSeed<'_> {
            type Value = Arc<Node>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sorbus green node")
            }

            fn visit_seq<Seq>(self, mut seq: Seq) -> Result<Self::Value, Seq::Error>
            where
                Seq: SeqAccess<'de>,
            {
                let kind = seq.next_element()?.ok_or_else(|| Error::invalid_length(0, &self))?;
                let children = seq
                    .next_element_seed(NodeChildrenSeed(self.0))?
                    .ok_or_else(|| Error::invalid_length(1, &self))?;
                Ok(self.0.node(kind, children))
            }

            fn visit_map<Map>(self, mut map: Map) -> Result<Self::Value, Map::Error>
            where
                Map: MapAccess<'de>,
            {
                let mut kind = None;
                let mut children = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Kind if kind.is_some() => Err(Error::duplicate_field("kind"))?,
                        Field::Kind => kind = Some(map.next_value()?),
                        Field::Children if children.is_some() => {
                            Err(Error::duplicate_field("children"))?
                        }
                        Field::Children => {
                            children = Some(map.next_value_seed(NodeChildrenSeed(self.0))?)
                        }
                    }
                }
                let kind = kind.ok_or_else(|| Error::missing_field("kind"))?;
                let children = children.ok_or_else(|| Error::missing_field("text"))?;
                Ok(self.0.node(kind, children))
            }
        }

        const FIELDS: &[&str] = &["kind", "children"];
        deserializer.deserialize_struct("Node", FIELDS, self)
    }
}

// FUTURE: Maybe construct the node in place for sequences of known length
struct NodeChildrenSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for NodeChildrenSeed<'_> {
    type Value = Vec<NodeOrToken<Arc<Node>, Arc<Token>>>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for NodeChildrenSeed<'_> {
    type Value = Vec<NodeOrToken<Arc<Node>, Arc<Token>>>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a sequence of sorbus green elements")
    }

    fn visit_seq<Seq>(self, mut seq: Seq) -> Result<Self::Value, Seq::Error>
    where
        Seq: SeqAccess<'de>,
    {
        let mut v =
            if let Some(size) = seq.size_hint() { Vec::with_capacity(size) } else { Vec::new() };
        while let Some(element) = seq.next_element_seed(ElementSeed(self.0))? {
            v.push(element);
        }
        Ok(v)
    }
}

struct ElementSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for ElementSeed<'_> {
    type Value = NodeOrToken<Arc<Node>, Arc<Token>>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(variant_identifier)]
        enum Variant {
            Node,
            Token,
        }

        struct ElementVisitor<'a>(&'a mut Builder);
        impl<'de> Visitor<'de> for ElementVisitor<'_> {
            type Value = NodeOrToken<Arc<Node>, Arc<Token>>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sorbus green node or token")
            }

            fn visit_enum<Data>(self, data: Data) -> Result<Self::Value, Data::Error>
            where
                Data: EnumAccess<'de>,
            {
                match data.variant()? {
                    (Variant::Node, variant) => Ok(NodeOrToken::Node(
                        variant.struct_variant(&["kind", "children"], NodeSeed(self.0))?,
                    )),
                    (Variant::Token, variant) => Ok(NodeOrToken::Token(
                        variant.struct_variant(&["kind", "text"], TokenSeed(self.0))?,
                    )),
                }
            }
        }

        const VARIANTS: &[&str] = &["Node", "Token"];
        deserializer.deserialize_enum("NodeOrToken", VARIANTS, ElementVisitor(self.0))
    }
}
