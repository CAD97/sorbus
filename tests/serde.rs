use sorbus::*;

#[test]
fn deserializes_from_format_without_zero_copy() {
    let mut builder = green::Builder::new();
    let node = serde_yaml::seed::from_str_seed(
        r#"
---
kind: 0
text: "0"
"#,
        builder.deserialize_token(),
    )
    .unwrap();
}
