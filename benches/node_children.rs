use sorbus::NodeOrToken;
use {
    criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput},
    sorbus::{green, ArcBorrow, Kind},
    std::sync::Arc,
};

fn black_hole<T>(t: T) {
    black_box(t);
}

/// Make a green tree containing some number of `(+ (* 15 2) 62)` nodes.
fn make_tree(scale: usize) -> Arc<green::Node> {
    const WS: Kind = Kind(0);
    const L_PAREN: Kind = Kind(1);
    const R_PAREN: Kind = Kind(2);
    const ATOM: Kind = Kind(3);
    const LIST: Kind = Kind(4);

    let mut builder = green::TreeBuilder::new();
    #[rustfmt::skip]
    let tree = builder
        .start_node(LIST)
            .token(L_PAREN, "(")
            .token(ATOM, "+")
            .token(WS, " ")
            .start_node(LIST)
                .token(L_PAREN, "(")
                .token(ATOM, "*")
                .token(WS, " ")
                .token(ATOM, "15")
                .token(WS, " ")
                .token(ATOM, "2")
                .token(R_PAREN, ")")
            .finish_node()
            .token(WS, " ")
            .token(ATOM, "62")
            .token(R_PAREN, ")")
        .finish_node()
        .finish();

    builder.start_node(LIST);
    for _ in 0..scale {
        builder.add(tree.clone());
    }
    builder.finish_node().finish()
}

fn flat_children_iterate(c: &mut Criterion) {
    const SCALE: usize = 256;
    let mut group = c.benchmark_group("flat_children");
    for &scale in [SCALE, 2 * SCALE, 4 * SCALE, 8 * SCALE, 16 * SCALE].iter() {
        group.throughput(Throughput::Elements(scale as u64));
        let tree = make_tree(scale);
        group.bench_with_input(BenchmarkId::from_parameter(scale), &tree, |b, tree| {
            b.iter(|| tree.children().for_each(black_hole));
        });
    }
    group.finish();
}

fn visit(el: NodeOrToken<ArcBorrow<'_, green::Node>, ArcBorrow<'_, green::Token>>) {
    match el {
        NodeOrToken::Node(node) => node.children().for_each(visit),
        NodeOrToken::Token(token) => black_hole(token),
    }
}

fn visit_children_iterate(c: &mut Criterion) {
    const SCALE: usize = 256;
    let mut group = c.benchmark_group("visit_children");
    for &scale in [SCALE, 2 * SCALE, 4 * SCALE, 8 * SCALE, 16 * SCALE].iter() {
        group.throughput(Throughput::Elements(scale as u64));
        let tree = make_tree(scale);
        group.bench_with_input(BenchmarkId::from_parameter(scale), &tree, |b, tree| {
            b.iter(|| tree.children().for_each(visit));
        });
    }
    group.finish();
}

criterion_group!(benches, flat_children_iterate, visit_children_iterate);
criterion_main!(benches);
