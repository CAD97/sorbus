use {
    serde::{de::DeserializeSeed, Deserialize, Deserializer, Serialize, Serializer},
    serde_json::json,
    serde_test::{assert_de_tokens, assert_tokens, Token as T, Configure},
    sorbus::*,
    std::sync::Arc,
};

/// `green::Node` wrapper that implements Deserialize without DeserializeSeed
/// so that it's testable with `serde_test`
#[derive(Debug, Eq, PartialEq)]
struct Node {
    raw: Arc<green::Node>,
}

impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Node { raw: green::Builder::new().deserialize_node().deserialize(deserializer)? })
    }
}

#[test]
fn deserializes_from_format_without_zero_copy() -> serde_json::Result<()> {
    let mut builder = green::Builder::new();
    let json = json! {{
        "kind": 2,
        "children": [
            { "kind": 0, "text": "0" },
            { "kind": 1, "text": "1" },
        ]
    }};
    // Deserializing from Value by-move can take string ownership but not borrow the strings
    builder.deserialize_element().deserialize(json).map(drop)
}

fn make_tree() -> Node {
    #[rustfmt::skip]
    let tree = green::TreeBuilder::new()
        .start_node(Kind(2))
            .token(Kind(0), "0")
            .token(Kind(1), "1")
        .finish_node()
        .finish();
    Node { raw: tree }
}

#[test]
fn tree_de_serialization() {
    #[rustfmt::skip]
    assert_tokens(
        &make_tree().readable(),
        &[
            T::Struct { name: "Node", len: 2 },
                T::Str("kind"),
                    T::NewtypeStruct { name: "Kind" },
                        T::U16(2),
                T::Str("children"),
                    T::Seq { len: Some(2) },
                        T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(0),
                            T::Str("text"),
                                T::Str("0"),
                        T::StructVariantEnd,
                        T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(1),
                            T::Str("text"),
                                T::Str("1"),
                        T::StructVariantEnd,
                    T::SeqEnd,
            T::StructEnd,
        ]
    );
    #[rustfmt::skip]
    assert_tokens(
        &make_tree().compact(),
        &[
            T::Struct { name: "Node", len: 2 },
                T::Str("kind"),
                    T::NewtypeStruct { name: "Kind" },
                        T::U16(2),
                T::Str("children"),
                    T::Seq { len: Some(2) },
                        T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(0),
                            T::Str("text"),
                                T::Str("0"),
                        T::StructVariantEnd,
                        T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(1),
                            T::Str("text"),
                                T::Str("1"),
                        T::StructVariantEnd,
                    T::SeqEnd,
            T::StructEnd,
        ]
    );
}

#[test]
fn sloppy_tree_deserialization() {
    #[rustfmt::skip]
    assert_de_tokens(
        &make_tree().readable(),
        &[
            T::Struct { name: "Node", len: 2 },
                T::Str("kind"),
                    T::NewtypeStruct { name: "Kind" },
                        T::U16(2),
                T::Str("children"),
                    T::Seq { len: Some(2) },
                        T::Struct { name: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(0),
                            T::Str("text"),
                                T::Str("0"),
                        T::StructEnd,
                        T::Struct { name: "Token", len: 2 },
                            T::Str("kind"),
                                T::NewtypeStruct { name: "Kind" },
                                    T::U16(1),
                            T::Str("text"),
                                T::Str("1"),
                        T::StructEnd,
                    T::SeqEnd,
            T::StructEnd,
        ]
    );
}
