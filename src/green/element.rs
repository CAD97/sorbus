use {
    crate::{
        green::{Node, Token},
        ArcBorrow, NodeOrToken, TextSize,
    },
    ptr_union::{Enum2, Union2, UnionBuilder},
    std::{mem, sync::Arc},
    text_size::TextLen,
};

// // SAFETY: align of Node and Token are >= 2
// const ARC_UNION_PROOF: UnionBuilder<Union2<Arc<Node>, Arc<Token>>> =
//     unsafe { UnionBuilder::new2() };
// const REF_UNION_PROOF: UnionBuilder<Union2<&Node, &Token>> = unsafe { UnionBuilder::new2() };

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(super) struct Element {
    // raw: Union2<Arc<Node>, Arc<Token>>, // NB: Union2 does automatic thinning
}

impl Element {
    pub(super) fn node(node: Arc<Node>) -> Element {
        todo!()
        // Element { raw: ARC_UNION_PROOF.a(node) }
    }

    pub(super) fn into_node(self) -> Option<Arc<Node>> {
        todo!()
        // self.raw.into_a().ok()
    }

    pub(super) fn token(token: Arc<Token>) -> Element {
        todo!()
        // Element { raw: ARC_UNION_PROOF.b(token) }
    }

    pub(super) fn len(&self) -> TextSize {
        todo!()
        // match self.raw.as_deref(REF_UNION_PROOF).unpack() {
        //     Enum2::A(node) => node.len(),
        //     Enum2::B(token) => token.len(),
        // }
    }
}

impl TextLen for &'_ Element {
    fn text_len(self) -> TextSize {
        todo!()
        // self.len()
    }
}

impl From<&'_ Element> for NodeOrToken<ArcBorrow<'_, Node>, ArcBorrow<'_, Token>> {
    fn from(this: &'_ Element) -> Self {
        todo!()
        // // SAFETY: borrow lifetime is tied to heap lifetime we manage
        // unsafe {
        //     None.or_else(|| this.raw.with_a(|node| NodeOrToken::Node(erase_lt(node).into())))
        //         .or_else(|| this.raw.with_b(|token| NodeOrToken::Token(erase_lt(token).into())))
        //         .unwrap()
        // }
    }
}

impl From<Element> for NodeOrToken<Arc<Node>, Arc<Token>> {
    fn from(this: Element) -> Self {
        todo!()
        // Err(this.raw)
        //     .or_else(|this| this.into_a().map(NodeOrToken::Node))
        //     .or_else(|this| this.into_b().map(NodeOrToken::Token))
        //     .unwrap()
    }
}

impl From<NodeOrToken<Arc<Node>, Arc<Token>>> for Element {
    fn from(value: NodeOrToken<Arc<Node>, Arc<Token>>) -> Self {
        todo!()
        // match value {
        //     NodeOrToken::Node(node) => Self::node(node),
        //     NodeOrToken::Token(token) => Self::token(token),
        // }
    }
}

// /// # Safety
// ///
// /// References must not be misused per the Rust memory model.
// unsafe fn erase_lt<'input, 'output, T>(r: &'input T) -> &'output T {
//     mem::transmute(r)
// }
