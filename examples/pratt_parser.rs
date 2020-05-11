//! As described by @matklad in Simple but Powerful Pratt Parsing
//! https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
//! and implemented at https://github.com/matklad/minipratt/blob/master/src/bin/pratt.rs,
//! converted to use sorbus as the parsed tree.

use {
    sorbus::{green, Kind, NodeOrToken},
    std::{collections::VecDeque, fmt, str, sync::Arc},
};

// NB: only constructs a green tree at this time.
// Will construct a syntax (red) tree once those are available.

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct Token<'src> {
    kind: Kind,
    src: &'src str,
}

// token kinds
const ATOM: Kind = Kind(0);
const OP: Kind = Kind(1);
const WS: Kind = Kind(2);
// node kinds
const EXPR: Kind = Kind(3);

#[derive(Debug)]
struct Lexer<'src> {
    tokens: VecDeque<Token<'src>>,
    builder: green::TreeBuilder,
}

impl<'src> Lexer<'src> {
    fn new(input: &'src str) -> Self {
        assert!(input.is_ascii()); // sorry, but this is a simple example
        Lexer {
            tokens: input
                .as_bytes()
                .chunks(1)
                .map(|b| {
                    assert_eq!(b.len(), 1);
                    let c = b[0] as char;
                    let src = str::from_utf8(b).unwrap();
                    match c {
                        c if c.is_whitespace() => Token { kind: WS, src },
                        c if c.is_ascii_alphanumeric() => Token { kind: ATOM, src },
                        _ => Token { kind: OP, src },
                    }
                })
                .collect(),
            builder: green::TreeBuilder::new(),
        }
    }

    fn next(&mut self) -> Option<Token<'src>> {
        self.tokens.pop_front()
    }
    fn peek(&self) -> Option<Token<'src>> {
        self.tokens.front().copied()
    }
}

// It's no s-expressions, but it will do.
fn dump(node: &green::Node) -> String {
    fn display<'a>(
        el: NodeOrToken<&'a green::Node, &'a green::Token>,
        f: &'a mut dyn fmt::Write,
    ) -> fmt::Result {
        match el {
            NodeOrToken::Token(token) => write!(f, "{}", token.text())?,
            NodeOrToken::Node(node) => {
                assert_eq!(node.kind(), EXPR);
                let complex = node.children().any(|child| child.is_node());
                if complex {
                    write!(f, "⟪")?;
                }
                for child in node.children() {
                    display(child.as_deref(), f)?;
                }
                if complex {
                    write!(f, "⟫")?;
                }
            }
        }
        Ok(())
    }
    let mut s = String::new();
    display(NodeOrToken::Node(node), &mut s).unwrap();
    s
}

fn expr(input: &str) -> Arc<green::Node> {
    eprintln!();
    let mut lexer = Lexer::new(dbg!(input));
    expr_bp(&mut lexer, 0);
    lexer.builder.finish()
}

fn eat_ws(lexer: &mut Lexer) {
    // only emit a single normalized whitespace for simplicity of testing
    if let Some(Token { kind: WS, .. }) = lexer.peek() {
        lexer.next();
        lexer.builder.token(WS, " ");
    }
    while let Some(Token { kind: WS, .. }) = lexer.peek() {
        lexer.next();
    }
}

fn expr_bp(lexer: &mut Lexer, min_bp: u8) {
    let checkpoint = lexer.builder.checkpoint();
    lexer.builder.start_node(EXPR);

    eat_ws(lexer);
    // prefix operators
    match lexer.next() {
        Some(Token { kind: ATOM, src }) => {
            lexer.builder.token(ATOM, src);
        }
        Some(Token { kind: OP, src: "(" }) => {
            lexer.builder.token(OP, "(");
            expr_bp(lexer, 0);
            assert_eq!(lexer.next(), Some(Token { kind: OP, src: ")" }));
            lexer.builder.token(OP, ")");
        }
        Some(Token { kind: OP, src }) => {
            lexer.builder.token(OP, src);
            let ((), r_bp) = prefix_binding_power(src)
                .unwrap_or_else(|| panic!("not a prefix token: {:?}", src));
            expr_bp(lexer, r_bp);
        }
        t => panic!("bad token: {:?}", t),
    };

    loop {
        eat_ws(lexer);
        let op = match lexer.peek() {
            None => break,
            Some(Token { kind: OP, src }) => src,
            t => panic!("bad token: {:?}", t),
        };

        // postfix operators
        if let Some((l_bp, ())) = postfix_binding_power(op) {
            if l_bp < min_bp {
                break;
            }
            lexer.builder.finish_node();
            lexer.builder.start_node_at(checkpoint, EXPR);
            lexer.next();
            lexer.builder.token(OP, op);

            if op == "[" {
                expr_bp(lexer, 0);
                assert_eq!(lexer.next(), Some(Token { kind: OP, src: "]" }));
                lexer.builder.token(OP, "]");
            }

        // infix operators
        } else if let Some((l_bp, r_bp)) = infix_binding_power(op) {
            if l_bp < min_bp {
                break;
            }
            lexer.builder.finish_node();
            lexer.builder.start_node_at(checkpoint, EXPR);
            lexer.next();
            lexer.builder.token(OP, op);
            eat_ws(lexer);

            if op == "?" {
                expr_bp(lexer, 0);
                assert_eq!(lexer.next(), Some(Token { kind: OP, src: ":" }));
                lexer.builder.token(OP, ":");
                eat_ws(lexer);
            }
            expr_bp(lexer, r_bp);

        // no more operators
        } else {
            break;
        }
    }

    lexer.builder.finish_node();
}

fn prefix_binding_power(op: &str) -> Option<((), u8)> {
    match op {
        "+" | "-" => Some(((), 9)),
        _ => None,
    }
}

fn postfix_binding_power(op: &str) -> Option<(u8, ())> {
    match op {
        "!" => Some((11, ())),
        "[" => Some((11, ())),
        _ => None,
    }
}

fn infix_binding_power(op: &str) -> Option<(u8, u8)> {
    match op {
        "=" => Some((2, 1)),
        "?" => Some((4, 3)),
        "+" | "-" => Some((5, 6)),
        "*" | "/" => Some((7, 8)),
        "." => Some((14, 13)),
        _ => None,
    }
}

#[test]
fn tests() {
    let s = expr("1");
    assert_eq!(dump(&s), "1");

    let s = expr("1 + 2 * 3");
    assert_eq!(dump(&s), "⟪1 + ⟪2 * 3⟫⟫");

    let s = expr("a + b * c * d + e");
    assert_eq!(dump(&s), "⟪⟪a + ⟪⟪b * c ⟫* d ⟫⟫+ e⟫");

    let s = expr("f . g . h");
    assert_eq!(dump(&s), "⟪f . ⟪g . h⟫⟫");

    let s = expr("1 + 2 + f . g . h * 3 * 4");
    assert_eq!(dump(&s), "⟪⟪1 + 2 ⟫+ ⟪⟪⟪f . ⟪g . h ⟫⟫* 3 ⟫* 4⟫⟫");

    let s = expr("--1 * 2");
    assert_eq!(dump(&s), "⟪⟪-⟪-1 ⟫⟫* 2⟫");

    let s = expr("--f . g");
    assert_eq!(dump(&s), "⟪-⟪-⟪f . g⟫⟫⟫");

    let s = expr("-9!");
    assert_eq!(dump(&s), "⟪-⟪9!⟫⟫");

    let s = expr("f . g !");
    assert_eq!(dump(&s), "⟪⟪f . g ⟫!⟫");

    let s = expr("(((0)))");
    assert_eq!(dump(&s), "⟪(⟪(⟪(0)⟫)⟫)⟫");

    let s = expr("x[0][1]");
    assert_eq!(dump(&s), "⟪⟪x[0]⟫[1]⟫");

    let s = expr("a ? b : c ? d : e");
    assert_eq!(dump(&s), "⟪a ? b : ⟪c ? d : e⟫⟫");

    let s = expr("a = 0 ? b : c = d");
    assert_eq!(dump(&s), "⟪a = ⟪⟪0 ? b : c ⟫= d⟫⟫")
}

fn main() -> std::io::Result<()> {
    use std::io::BufRead;
    #[cfg(not(miri))]
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let s = expr(&line);
        println!("{}", dump(&s));
        println!("{:#?}", s);
        println!();
    }
    Ok(())
}
