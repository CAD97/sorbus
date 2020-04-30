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

        impl<'de, 'a> Visitor<'de> for TokenVisitor<'a> {
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

    fn deserialize<D>(self, _deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        todo!()
    }
}

struct ElementSeed<'a>(&'a mut Builder);

impl<'de> DeserializeSeed<'de> for ElementSeed<'_> {
    type Value = NodeOrToken<Arc<Node>, Arc<Token>>;

    fn deserialize<D>(self, _deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        todo!()
    }
}
