//! These tests don't necessarily assert anything. Instead,
//! they exist primarily to exercise the API under Miri as a sanitizer.

use {
    sorbus::{green, Kind, NodeOrToken},
    std::{ptr, sync::Arc},
};

/// This test creates a tree for the sexpr
///
/// ```lisp
/// (+ (* 15 2) 62)
/// ```
///
/// This test shows an example of using the top-down `TreeBuilder`.
#[test]
fn make_sexpr_tree() {
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

    // Save this node for test below.
    // This produces a node with the same identity as above,
    // because the builder dedupes nodes with an internal cache.
    #[rustfmt::skip]
    let inner_mul = builder
        .start_node(LIST)
            .token(L_PAREN, "(")
            .token(ATOM, "*")
            .token(WS, " ")
            .token(ATOM, "15")
            .token(WS, " ")
            .token(ATOM, "2")
            .token(R_PAREN, ")")
        .finish_node()
        .finish();

    // Some random operations to make sure they work:
    let (index, offset, el) = tree.child_with_offset(5.into());
    assert_eq!(index, 3);
    assert_eq!(offset, 3.into());
    assert!(ptr::eq(&*el.unwrap_node(), &*inner_mul));
    tree.children().for_each(drop);

    if cfg!(miri) {
        dbg!(tree);
    } else {
        insta::assert_debug_snapshot!(tree);
    }
}

/// This test creates a tree for the math
///
/// ```math
/// 1 + 2 * 3 + 4
/// ```
///
/// For clarity, this groups as:
///
/// ```math
/// ((1 + (2 * 3)) + 4)
/// ```
///
/// This test shows an example of using the bottom-up `Builder`.
#[test]
fn make_math_tree() {
    const WS: Kind = Kind(0);
    const OP: Kind = Kind(1);
    const NUM: Kind = Kind(2);
    const EXPR: Kind = Kind(3);

    let mut builder = green::Builder::new();

    let ws = builder.token(WS, " ");
    let n1 = builder.token(NUM, "1");
    let n2 = builder.token(NUM, "2");
    let n3 = builder.token(NUM, "3");
    let n4 = builder.token(NUM, "4");
    let add = builder.token(OP, "+");
    let mul = builder.token(OP, "*");

    // Invocations of the builder with the same (id) arguments produces the same (id) results
    assert!(Arc::ptr_eq(&ws, &builder.token(WS, " ")));

    // builder.node accepts iterator of Arc<Node>, Arc<Token>, or NodeOrToken<Arc<Node>, Arc<Token>>
    // so if you're mixing nodes and tokens, you need to include the type changing boilerplate.
    // You'll know if you need the bottom-up builder (LR or such). Use TreeBuilder otherwise.
    let n = |node: &Arc<green::Node>| NodeOrToken::from(node.clone());
    let t = |token: &Arc<green::Token>| NodeOrToken::from(token.clone());

    // We use vec![] as a quick and easy ExactSizeIterator.
    // Particular implementations may use specialized iterators for known child array lengths.
    // (Please, const-generic angels, give us `[_; N]: IntoIterator` sooner rather than later!)
    let inner_mul = builder.node(EXPR, vec![n2, ws.clone(), mul, ws.clone(), n3]);
    let left_add = builder.node(EXPR, vec![t(&n1), t(&ws), t(&add), t(&ws), n(&inner_mul)]);
    let right_add = builder.node(EXPR, vec![n(&left_add), t(&ws), t(&add), t(&ws), t(&n4)]);

    let tree = right_add;

    // Some random operations to make sure they work:
    let (index, offset, el) = tree.child_with_offset(5.into());
    assert_eq!(index, 0);
    assert_eq!(offset, 0.into());
    assert!(ptr::eq(&*el.unwrap_node(), &*left_add));
    tree.children().for_each(drop);

    if cfg!(miri) {
        dbg!(tree);
    } else {
        insta::assert_debug_snapshot!(tree);
    }
}
