use sorbus::*;

#[test]
fn works_properly() {
    let kind0 = Kind(0);
    let kind1 = Kind(1);
    let kind2 = Kind(2);
    let mut builder = green::TreeBuilder::new();

    #[rustfmt::skip]
    let inner = builder
        .start_node(kind1)
            .token(kind0, "kind")
        .finish_node()
    .finish();

    #[rustfmt::skip]
    let outer = builder
        .start_node(kind2)
            .add(inner.clone())
        .finish_node()
    .finish();

    assert_eq!(builder.builder().size(), 3);

    drop(outer);
    builder.builder().gc();
    assert_eq!(builder.builder().size(), 2);

    drop(inner);
    builder.builder().gc();
    assert_eq!(builder.builder().size(), 0);
}

#[test]
fn degenerate() {
    let kind = Kind(0);
    let mut builder = green::TreeBuilder::new();

    for _ in 0..999 {
        builder.start_node(kind);
    }
    builder.token(kind, "wow");
    for _ in 0..999 {
        builder.finish_node();
    }

    let _ = builder.finish();

    assert_eq!(builder.builder().size(), 1000);
    builder.builder().gc();
    assert_eq!(builder.builder().size(), 0);
}
