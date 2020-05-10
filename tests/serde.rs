use {
    insta::{assert_json_snapshot, assert_ron_snapshot, assert_yaml_snapshot, with_settings},
    serde::{de::DeserializeSeed, Deserialize, Deserializer, Serialize, Serializer},
    serde_test::{assert_de_tokens, assert_tokens, Token as T},
    sorbus::*,
    std::{ptr, sync::Arc},
};

/// `green::Node` wrapper that implements Deserialize without DeserializeSeed
/// so that it's testable with `serde_test`
#[derive(Debug, Eq, PartialEq)]
struct Node {
    raw: Arc<green::Node>,
}

#[derive(Debug, Eq, PartialEq)]
struct Token {
    raw: Arc<green::Token>,
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

impl Serialize for Token {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Token {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Token { raw: green::Builder::new().deserialize_token().deserialize(deserializer)? })
    }
}

#[test]
fn deserializes_from_format_without_zero_copy() -> serde_json::Result<()> {
    let mut tree_builder = green::TreeBuilder::new();
    let tree = make_tree_with(&mut tree_builder);
    let value = serde_json::to_value(&tree)?;

    // Deserializing from Value by-move can take string ownership but not borrow the strings
    let deserialized = tree_builder.builder().deserialize_node().deserialize(value)?;

    assert!(Arc::ptr_eq(&tree.raw, &deserialized));
    Ok(())
}

fn make_tree_with(builder: &mut green::TreeBuilder) -> Node {
    #[rustfmt::skip]
    let tree = builder
        .start_node(Kind(2))
            .token(Kind(0), "0")
            .token(Kind(1), "1")
        .finish_node()
        .finish();
    Node { raw: tree }
}

fn make_tree() -> Node {
    make_tree_with(&mut green::TreeBuilder::new())
}

#[rustfmt::skip]
const TREE_SER: &[T] = &[
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
];

#[rustfmt::skip]
const TREE_SEQ: &[T] = &[
    T::Seq { len: Some(2) },
        T::NewtypeStruct { name: "Kind" },
            T::U16(2),
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
    T::SeqEnd,
];

#[rustfmt::skip]
const YODA_SER: &[T] = &[
    T::Struct { name: "Node", len: 2 },
        T::Str("children"),
            T::Seq { len: Some(2) },
                T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                    T::Str("text"),
                        T::Str("0"),
                    T::Str("kind"),
                        T::NewtypeStruct { name: "Kind" },
                            T::U16(0),
                T::StructVariantEnd,
                T::StructVariant { name: "NodeOrToken", variant: "Token", len: 2 },
                    T::Str("text"),
                        T::Str("1"),
                    T::Str("kind"),
                        T::NewtypeStruct { name: "Kind" },
                            T::U16(1),
                T::StructVariantEnd,
            T::SeqEnd,
        T::Str("kind"),
            T::NewtypeStruct { name: "Kind" },
                T::U16(2),
    T::StructEnd,
];

#[test]
fn tree_de_serialization() {
    assert_tokens(&make_tree(), TREE_SER);
    assert_de_tokens(&make_tree(), TREE_SEQ);
    assert_de_tokens(&make_tree(), YODA_SER);
}

fn make_token() -> Token {
    Token { raw: green::Builder::new().token(Kind(!0), "no-this-is-patrick") }
}

#[rustfmt::skip]
const TOKEN_SER: &[T] = &[
    T::Struct { name: "Token", len: 2 },
        T::Str("kind"),
            T::NewtypeStruct { name: "Kind" },
                T::U16(!0),
        T::Str("text"),
            T::Str("no-this-is-patrick"),
    T::StructEnd,
];

#[rustfmt::skip]
const TOKEN_SEQ: &[T] = &[
    T::Seq { len: Some(2) },
        T::NewtypeStruct { name: "Kind" },
            T::U16(!0),
        T::Str("no-this-is-patrick"),
    T::SeqEnd,
];

#[test]
fn token_de_serialization() {
    assert_tokens(&make_token(), TOKEN_SER);
    assert_de_tokens(&make_token(), TOKEN_SEQ);
}

#[test]
#[cfg_attr(miri, ignore)]
fn assert_serialization_formats() {
    let node = make_tree();
    with_settings!({snapshot_suffix => "json"}, {
        assert_json_snapshot!(node);
    });
    with_settings!({snapshot_suffix => "yaml"}, {
        assert_yaml_snapshot!(node);
    });
    with_settings!({snapshot_suffix => "ron"}, {
        assert_ron_snapshot!(node);
    });
}

#[test]
fn deduplication_of_nodes_happens() {
    let tree_json = r##"{
        "kind": 2,
        "children": [
            {"Node":{
                "kind": 1,
                "children": [
                    {"Token":{
                        "kind": 0,
                        "text": " "
                    }}
                ]
            }},
            {"Node":{
                "kind": 1,
                "children": [
                    {"Token":{
                        "kind": 0,
                        "text": " "
                    }}
                ]
            }}
        ]
    }"##;

    // without SeqAccess::size_hint
    let node: Node = serde_json::from_str(tree_json).unwrap();
    let mut children = node.raw.children();
    assert!(ptr::eq(
        &*children.next().unwrap().unwrap_node(),
        &*children.next().unwrap().unwrap_node(),
    ));

    // with SeqAccess::size_hint
    let node: Node = serde_json::from_value(tree_json.parse().unwrap()).unwrap();
    let mut children = node.raw.children();
    assert!(ptr::eq(
        &*children.next().unwrap().unwrap_node(),
        &*children.next().unwrap().unwrap_node(),
    ));
}
