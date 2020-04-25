use sorbus::{green, Kind};

#[test]
fn whoops_linked_list() {
    // miri runs out of stack during drop at this size
    const RECURSION_FACTOR: usize = 16;
    const KIND: Kind = Kind(0);

    let mut builder = green::TreeBuilder::new();
    for _ in 0..RECURSION_FACTOR {
        builder.start_node(KIND);
    }
    builder.token(KIND, " ");
    for _ in 0..RECURSION_FACTOR {
        builder.finish_node();
    }
    let _tree = builder.finish();
}
