//! These tests don't necessarily assert anything. Instead,
//! they exist primarily to exercise the API under Miri as a sanitizer.

use sorbus::*;

#[test]
/// This test creates a tree for the sexpr
///
/// ```lisp
/// (+ (* 15 2) 62)
/// ```
fn make_sexpr_tree() {
    const WS: Kind = Kind(0);
    const L_PAREN: Kind = Kind(1);
    const R_PAREN: Kind = Kind(2);
    const ATOM: Kind = Kind(3);
    const LIST: Kind = Kind(4);

    #[rustfmt::skip]
    let tree = green::TreeBuilder::new()
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

    tree.child_at_offset(5.into());
    tree.children().for_each(drop);

    if cfg!(miri) {
        dbg!(tree);
    } else {
        insta::assert_debug_snapshot!(tree);
    }
}
