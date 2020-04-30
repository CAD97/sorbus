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

    /// Deserialize a node or a token using this cache.
    ///
    /// This deserializes the "untagged representation", or in other words,
    /// this can deserialize the result of serializing a node or a token.
    /// (This only works for self-describing formats.)
    ///
    /// Child elements of any internal nodes _must_ still used the "tagged
    /// representation" that is emitted by serialization. This _may_ be
    /// relaxed in the future, but serialization will always use the tag.
    pub fn deserialize_element(
        &mut self,
    ) -> impl for<'de> DeserializeSeed<'de, Value = NodeOrToken<Arc<Node>, Arc<Token>>> + '_ {
        ElementSeed(self)
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

        struct TokenVisitor<'a>(&'a mut Builder);
        impl<'de> Visitor<'de> for TokenVisitor<'_> {
            type Value = Arc<Token>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sorbus green token")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let kind = seq.next_element()?.ok_or_else(|| Error::invalid_length(0, &self))?;

                struct NerdSnipeToAvoidThisPotentialCopy<'a>(&'a mut Builder, Kind);
                impl<'de> DeserializeSeed<'de> for NerdSnipeToAvoidThisPotentialCopy<'_> {
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

                seq.next_element_seed(NerdSnipeToAvoidThisPotentialCopy(self.0, kind))
                    .transpose()
                    .ok_or_else(|| Error::invalid_length(1, &self))?
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut kind = None;
                let mut text = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Kind if kind.is_some() => Err(Error::duplicate_field("kind"))?,
                        Field::Kind => kind = Some(map.next_value()?),
                        Field::Text if text.is_some() => Err(Error::duplicate_field("text"))?,
                        Field::Text => text = Some(map.next_value()?),
                    }
                }
                let kind = kind.ok_or_else(|| Error::missing_field("kind"))?;
                // FUTURE: eliminate this copy in the ideal case
                let text: Str = text.ok_or_else(|| Error::missing_field("text"))?;
                Ok(self.0.token(kind, &text))
            }
        }

        const FIELDS: &[&str] = &["kind", "text"];
        deserializer.deserialize_struct("Token", FIELDS, TokenVisitor(self.0))
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

        struct NodeVisitor<'a>(&'a mut Builder);
        impl<'de> Visitor<'de> for NodeVisitor<'_> {
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
        deserializer.deserialize_struct("Node", FIELDS, NodeVisitor(self.0))
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

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kind,
            Text,
            Children,
        }

        #[derive(Deserialize)]
        #[serde(field_identifier)]
        #[allow(non_camel_case_types)]
        enum FieldOrVariant {
            Node,
            Token,
            kind,
            text,
            children,
        }

        struct ElementVisitor<'a>(&'a mut Builder);
        impl<'de> Visitor<'de> for ElementVisitor<'_> {
            type Value = NodeOrToken<Arc<Node>, Arc<Token>>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sorbus green node or token")
            }

            fn visit_seq<Seq>(self, mut seq: Seq) -> Result<Self::Value, Seq::Error>
            where
                Seq: SeqAccess<'de>,
            {
                // RON's newtype `deserialize_any`s as a single-element seq
                seq.next_element_seed(ElementSeed(self.0))?
                    .ok_or_else(|| Error::invalid_length(0, &self))
            }

            fn visit_map<Map>(self, mut map: Map) -> Result<Self::Value, Map::Error>
            where
                Map: MapAccess<'de>,
            {
                let mut kind = None;
                // FUTURE: eliminate this copy in the ideal case
                let mut text = None::<Str>;
                let mut children = None;

                if let Some(key) = map.next_key()? {
                    match key {
                        FieldOrVariant::Node => {
                            return Ok(NodeOrToken::Node(map.next_value_seed(NodeSeed(self.0))?))
                        }
                        FieldOrVariant::Token => {
                            return Ok(NodeOrToken::Token(map.next_value_seed(TokenSeed(self.0))?))
                        }
                        FieldOrVariant::kind => kind = Some(map.next_value()?),
                        FieldOrVariant::text => text = Some(map.next_value()?),
                        FieldOrVariant::children => {
                            children = Some(map.next_value_seed(NodeChildrenSeed(self.0))?)
                        }
                    }
                }
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Kind if kind.is_some() => Err(Error::duplicate_field("kind"))?,
                        Field::Kind => kind = Some(map.next_value()?),
                        Field::Text if text.is_some() => Err(Error::duplicate_field("text"))?,
                        Field::Text => text = Some(map.next_value()?),
                        Field::Children if children.is_some() => {
                            Err(Error::duplicate_field("children"))?
                        }
                        Field::Children => {
                            children = Some(map.next_value_seed(NodeChildrenSeed(self.0))?)
                        }
                    }
                }

                let kind = kind.ok_or_else(|| Error::invalid_type(Unexpected::Map, &self))?;
                match (text, children) {
                    (None, Some(children)) => Ok(NodeOrToken::Node(self.0.node(kind, children))),
                    (Some(text), None) => Ok(NodeOrToken::Token(self.0.token(kind, &text))),
                    _ => Err(Error::invalid_type(Unexpected::Map, &self)),
                }
            }

            fn visit_enum<Data>(self, data: Data) -> Result<Self::Value, Data::Error>
            where
                Data: EnumAccess<'de>,
            {
                match data.variant()? {
                    (Variant::Node, variant) => {
                        Ok(NodeOrToken::Node(variant.newtype_variant_seed(NodeSeed(self.0))?))
                    }
                    (Variant::Token, variant) => {
                        Ok(NodeOrToken::Token(variant.newtype_variant_seed(TokenSeed(self.0))?))
                    }
                }
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_any(ElementVisitor(self.0))
        } else {
            const VARIANTS: &[&str] = &["Node", "Token"];
            deserializer.deserialize_enum("NodeOrToken", VARIANTS, ElementVisitor(self.0))
        }
    }
}
