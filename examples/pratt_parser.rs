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
const EXPR: Kind = Kind(3); // normally you'd probably have different expression types

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
        loop {
            let token = self.tokens.pop_front()?;
            if let Token { kind: WS, src } = token {
                self.builder.token(WS, src);
            } else {
                return Some(token);
            }
        }
    }

    fn peek(&mut self) -> Option<Token<'src>> {
        self.tokens.iter().copied().find(|token| token.kind != WS)
    }

    fn eager_eat_ws(&mut self) {
        while let Some(Token { kind: WS, src }) = self.tokens.front() {
            self.builder.token(WS, src);
            self.tokens.pop_front();
        }
    }
}

fn to_sexpr(node: &green::Node) -> String {
    fn display<'a>(
        el: NodeOrToken<&'a green::Node, &'a green::Token>,
        f: &'a mut dyn fmt::Write,
    ) -> fmt::Result {
        match el {
            NodeOrToken::Token(token) => write!(f, "{}", token.text().unwrap())?,
            NodeOrToken::Node(node) => {
                let children_of_interest: Vec<_> =
                    node.children().filter(|el| el.kind() != WS).collect();
                if children_of_interest.len() == 1 {
                    display(children_of_interest[0].as_deref(), f)?;
                } else {
                    f.write_str("(")?;
                    for op in children_of_interest.iter().filter(|e| e.kind() == OP) {
                        display(op.as_deref(), f)?;
                    }
                    for expr in children_of_interest.iter().filter(|e| e.kind() == EXPR) {
                        f.write_str(" ")?;
                        display(expr.as_deref(), f)?;
                    }
                    f.write_str(")")?;
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
    eprintln!();
    let node = lexer.builder.finish();
    let display = to_sexpr(&node);
    eprintln!("{}", display);
    node
}

fn expr_bp(lexer: &mut Lexer, min_bp: u8) {
    let checkpoint = lexer.builder.checkpoint();
    lexer.builder.start_node(EXPR);

    // prefix operators
    match lexer.next() {
        Some(Token { kind: ATOM, src }) => {
            lexer.builder.token(ATOM, src);
            eprint!("{} ", src);
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
            eprint!("({}) ", src);
        }
        t => panic!("bad token: {:?}", t),
    };

    loop {
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
            lexer.builder.start_node_at(checkpoint, EXPR).finish_node();
            lexer.next();
            lexer.builder.token(OP, op);

            if op == "[" {
                expr_bp(lexer, 0);
                assert_eq!(lexer.next(), Some(Token { kind: OP, src: "]" }));
                lexer.builder.token(OP, "]");
                eprint!("[] ");
            } else {
                eprint!("({}) ", op);
            }

        // infix operators
        } else if let Some((l_bp, r_bp)) = infix_binding_power(op) {
            if l_bp < min_bp {
                break;
            }
            lexer.builder.start_node_at(checkpoint, EXPR).finish_node();
            lexer.next();
            lexer.builder.token(OP, op);
            lexer.eager_eat_ws();

            if op == "?" {
                expr_bp(lexer, 0);
                assert_eq!(lexer.next(), Some(Token { kind: OP, src: ":" }));
                lexer.builder.token(OP, ":");
                lexer.eager_eat_ws();
                expr_bp(lexer, r_bp);
                eprint!("?: ");
            } else {
                expr_bp(lexer, r_bp);
                eprint!("{} ", op);
            }

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
    assert_eq!(to_sexpr(&s), "1");

    let s = expr("1 + 2 * 3");
    assert_eq!(to_sexpr(&s), "(+ 1 (* 2 3))");

    let s = expr("a + b * c * d + e");
    assert_eq!(to_sexpr(&s), "(+ (+ a (* (* b c) d)) e)");

    let s = expr("f . g . h");
    assert_eq!(to_sexpr(&s), "(. f (. g h))");

    let s = expr("1 + 2 + f . g . h * 3 * 4");
    assert_eq!(to_sexpr(&s), "(+ (+ 1 2) (* (* (. f (. g h)) 3) 4))");

    let s = expr("--1 * 2");
    assert_eq!(to_sexpr(&s), "(* (- (- 1)) 2)");

    let s = expr("--f . g");
    assert_eq!(to_sexpr(&s), "(- (- (. f g)))");

    let s = expr("-9!");
    assert_eq!(to_sexpr(&s), "(- (! 9))");

    let s = expr("f . g !");
    assert_eq!(to_sexpr(&s), "(! (. f g))");

    let s = expr("(((0)))");
    assert_eq!(to_sexpr(&s), "(() (() (() 0)))");

    let s = expr("x[0][1]");
    assert_eq!(to_sexpr(&s), "([] ([] x 0) 1)");

    let s = expr("a ? b : c ? d : e");
    assert_eq!(to_sexpr(&s), "(?: a b (?: c d e))");

    let s = expr("a = 0 ? b : c = d");
    assert_eq!(to_sexpr(&s), "(= a (= (?: 0 b c) d))")
}

fn main() -> std::io::Result<()> {
    use std::io::BufRead;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let s = expr(&line);
        println!("{:#?}", s);
        println!();
    }
    Ok(())
}
