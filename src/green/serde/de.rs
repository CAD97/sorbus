#![allow(clippy::try_err)]

extern crate serde; // this line required to workaround rust-lang/rust#55779

use {
    crate::{
        green::{pack_element, Builder, Element, Node, Token},
        Kind, NodeOrToken,
    },
    rc_box::ArcBox,
    serde::{de::*, Deserialize},
    std::{borrow::Cow, fmt, marker::PhantomData, ops::Deref, str, sync::Arc},
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
#[derive(Deserialize)]
#[serde(transparent)]
struct Str<'a>(#[serde(borrow)] Cow<'a, str>);
impl Deref for Str<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
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
        const FIELDS: &[&str] = &["kind", "text"];
        deserializer.deserialize_struct("Token", FIELDS, self)
    }
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
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kind,
            Text,
        }

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

                (Field::Kind, WithText(text)) => Finish(self.0.token(map.next_value()?, &text)),
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
            Start | WithText(_) => Err(Error::missing_field("kind")),
            WithKind(_) => Err(Error::missing_field("text")),
            Finish(token) => Ok(token),
        }
    }
}

struct TokenSeedKind<'a>(&'a mut Builder, Kind);
impl<'de> DeserializeSeed<'de> for TokenSeedKind<'_> {
    type Value = Arc<Token>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for TokenSeedKind<'_> {
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

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: Error,
    {
        match str::from_utf8(v) {
            Ok(v) => self.visit_str(v),
            Err(_) => Err(Error::invalid_value(Unexpected::Bytes(v), &self)),
        }
    }
}

struct NodeSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for NodeSeed<'_> {
    type Value = Arc<Node>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        const FIELDS: &[&str] = &["kind", "children"];
        deserializer.deserialize_struct("Node", FIELDS, self)
    }
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
        let node = seq
            .next_element_seed(NodeSeedKind(self.0, kind))?
            .ok_or_else(|| Error::invalid_length(1, &self))?;
        Ok(node)
    }

    fn visit_map<Map>(self, mut map: Map) -> Result<Self::Value, Map::Error>
    where
        Map: MapAccess<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kind,
            Children,
        }

        use VisitState::*;
        enum VisitState {
            Start,
            WithKind(Kind),
            WithChildren(ArcBox<Node>),
            Finish(Arc<Node>),
        }

        let mut state = Start;
        while let Some(key) = map.next_key()? {
            state = match (key, state) {
                (Field::Kind, Start) => WithKind(map.next_value()?),
                (Field::Children, Start) => {
                    WithChildren(map.next_value_seed(NodeChildrenSeed(self.0))?)
                }

                (Field::Kind, WithChildren(mut node)) => {
                    node.set_kind(map.next_value()?);
                    Finish(self.0.cache_node(node.into()))
                }
                (Field::Children, WithKind(kind)) => {
                    Finish(map.next_value_seed(NodeSeedKind(self.0, kind))?)
                }

                (Field::Kind, WithKind(_)) => Err(Error::duplicate_field("kind"))?,
                (Field::Kind, Finish(_)) => Err(Error::duplicate_field("kind"))?,
                (Field::Children, WithChildren(_)) | (Field::Children, Finish(_)) => {
                    Err(Error::duplicate_field("children"))?
                }
            }
        }

        match state {
            Start | WithChildren(_) => Err(Error::missing_field("kind")),
            WithKind(_) => Err(Error::missing_field("children")),
            Finish(node) => Ok(node),
        }
    }
}

struct NodeSeedKind<'a>(&'a mut Builder, Kind);
impl<'de> DeserializeSeed<'de> for NodeSeedKind<'_> {
    type Value = Arc<Node>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut node = NodeChildrenSeed(self.0).deserialize(deserializer)?;
        node.set_kind(self.1);
        Ok(self.0.cache_node(node.into()))
    }
}

/// Deserialize node children without knowing the kind.
/// Uses a kind of `Kind(0)`; fix it and then dedupe the node!
struct NodeChildrenSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for NodeChildrenSeed<'_> {
    type Value = ArcBox<Node>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for NodeChildrenSeed<'_> {
    type Value = ArcBox<Node>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a sequence of sorbus green elements")
    }

    fn visit_seq<Seq>(self, mut seq: Seq) -> Result<Self::Value, Seq::Error>
    where
        Seq: SeqAccess<'de>,
    {
        if seq.size_hint().is_some() {
            let node =
                Node::try_new(Kind(0), SeqAccessExactSizeIterator(self.0, seq, PhantomData))?;
            Ok(node)
        } else {
            let mut children = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(element) = seq.next_element_seed(ElementSeed(self.0))? {
                children.push(element);
            }
            Ok(Node::new(Kind(0), children.into_iter()))
        }
    }
}

struct SeqAccessExactSizeIterator<'a, 'de, Seq: SeqAccess<'de>>(
    &'a mut Builder,
    Seq,
    PhantomData<&'de ()>,
);
impl<'de, Seq: SeqAccess<'de>> Iterator for SeqAccessExactSizeIterator<'_, 'de, Seq> {
    type Item = Result<Element, Seq::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.1.next_element_seed(ElementSeed(self.0)).transpose()
    }

    #[cfg(not(tarpaulin_ignore))] // `len` is used instead, and this method is obviously correct
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}
impl<'de, Seq: SeqAccess<'de>> ExactSizeIterator for SeqAccessExactSizeIterator<'_, 'de, Seq> {
    fn len(&self) -> usize {
        self.1.size_hint().unwrap()
    }
}

struct ElementSeed<'a>(&'a mut Builder);
impl<'de> DeserializeSeed<'de> for ElementSeed<'_> {
    type Value = Element;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        const VARIANTS: &[&str] = &["Node", "Token"];
        deserializer.deserialize_enum("NodeOrToken", VARIANTS, self)
    }
}
impl<'de> Visitor<'de> for ElementSeed<'_> {
    type Value = Element;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a sorbus green node or token")
    }

    fn visit_enum<Data>(self, data: Data) -> Result<Self::Value, Data::Error>
    where
        Data: EnumAccess<'de>,
    {
        #[derive(Deserialize)]
        #[serde(variant_identifier)]
        enum Variant {
            Node,
            Token,
        }

        Ok(pack_element(match data.variant()? {
            (Variant::Node, variant) => {
                NodeOrToken::Node(variant.struct_variant(&["kind", "children"], NodeSeed(self.0))?)
            }
            (Variant::Token, variant) => {
                NodeOrToken::Token(variant.struct_variant(&["kind", "text"], TokenSeed(self.0))?)
            }
        }))
    }
}
