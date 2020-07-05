# sorbus

A generic "green" syntax tree implementation.
Extracted from [Rust-analyzer]'s [rowan], inspired by Swift's [libSyntax] and .NET's [Roslyn].

Rowan currently still uses its own green tree implementation, but is expected to eventually re-export this one.

Sorbus is the [genus and subgenus of the Rowan tree](https://en.wikipedia.org/wiki/Sorbus).

  [rust-analyzer]: <https://github.com/rust-analyzer/rust-analyzer/>
  [rowan]: <https://github.com/rust-analyzer/rowan>
  [libSyntax]: <https://github.com/apple/swift/tree/swift-5.2.4-RELEASE/lib/Syntax>
  [Roslyn]: <https://github.com/dotnet/roslyn>
  [ericlippert-red-green-trees]: <https://ericlippert.com/2012/06/08/red-green-trees/>
  [oilshell-lst]: <https://www.oilshell.org/blog/2017/02/11.html>

## Red/Green Trees

As [made popular by Roslyn][ericlippert-red-green-trees].

The "green" tree is an immutable, persistent, [_lossless_][oilshell-lst] syntax tree.
It is also uniformly typed; any language semantics are layered on top of the uniformly typed tree.

Importantly for the IDE use case, any source file, even incorrect ones, can be represented.
Additionally, due to persistence, incremental changes can typically reuse much of the tree.

Conceptually, the green tree is isomorphic to the below definition:

```rust
#[derive(Copy, Eq)]
struct Kind(pub u16);

#[derive(Clone, Eq)]
struct Node {
    kind: Kind,
    text_len: usize,
    children: Vec<Either<Arc<Node>, Arc<Token>>>,
}

#[derive(Clone, Eq)]
struct Token {
    kind: SyntaxKind,
    text: String,
}
```

The "red" tree is a transient view of the green tree that remembers parent offsets and absolute textual position within the tree.
It is also at this level that language semantics are typically layered on top of the uniformly typed green tree. 
Rowan provides the red tree built on top of sorbus's green tree.

## Implementation Tricks

  [DSTs]: <https://doc.rust-lang.org/reference/dynamically-sized-types.html>
  [miri]: <https://github.com/rust-lang/miri>
  [serde]: <lib.rs/serde>
  [`ArcSwap`]: <https://docs.rs/arc-swap/0.4/arc_swap/type.ArcSwap.html>
  [`ArcBorrow`]: <https://docs.rs/rc-borrow/1/rc_borrow/struct.ArcBorrow.html>
  [`Thin`]: <https://docs.rs/erasable/1/erasable/struct.Thin.html>
  [green::Builder]: <https://cad97.github.io/sorbus/sorbus/green/struct.Builder.html>
  [green::TreeBuilder]: <https://cad97.github.io/sorbus/sorbus/green/struct.TreeBuilder.html>
  [TreeBuilder::checkpoint]: <https://cad97.github.io/sorbus/sorbus/green/struct.TreeBuilder.html#method.checkpoint>
  [Builder::gc]: <https://cad97.github.io/sorbus/sorbus/green/struct.Builder.html#method.gc>

The green tree is immutable and persistent, so nodes can be deduplicated.
This is achieved in sorbus by proxying all creation of green tree elements through
[a builder cache][green::Builder], at which point they are deduplicated.

To reduce the number of allocations required for a green tree and increase the locallity,
the green nodes and tokens are [DSTs] laid out linearly in memory, roughly as the following:

```text
Node
+----------+------+----------+----------------+--------+--------+-----+--------+
| refcount | kind | text len | children count | child1 | child2 | ... | childN |
+----------+------+----------+----------------+--------+--------+-----+--------+

Token
+----------+------+----------+---------+
| refcount | kind | text len | text... |
+----------+------+----------+---------+
```

(though not necessarily in that order). As a result, green tree elements are only usable behind
an indirection: `Arc` for owned nodes, and `&` for borrowed nodes. Additionally, we use the
[`ArcBorrow`] type as a "+0" reference counted pointer to reduce reference counting overhead.

Because `Node` and `Token` are DSTs, the indirections to them are fat pointers, taking two words.
To mitigate this, the length of the trailing arrays are kept inline. This enables use of [`Thin`]
to create thin pointers to the green element types. This (and more generally type erasure) are
used liberally within the library, but can also be used externally if the pointer size is an issue.

Note that `Thin` can be used for (most) any pointer type, including `Thin<Arc<_>>` for owned,
`Thin<&_>` for borrowed, and `Thin<ArcBorrow<'_, _>>` for "+0" pointers.

Pointers that may be to a node or a token are packed into a single word using alignment tagging.
However, the child stored in each node's children array is not just the tagged pointer — instead,
it also includes the cumulative offset of that child from the parent node. This array is then
packed tightly by alternating the alignment of each child — one is `(u32, usize)` and the next
is `(usize, u32)` — such that everything stays nicely aligned and without padding.

This extra somewhat redundant storage of node offsets allows asymptotically faster tree traversal —
each node's children can be binary searched, allowing top-down finding of the node at some offset in
just ***O*(*d* log *w*)** time rather than ***O*(*d* *w*)** time (for tree depth *d* and node width
*w*). Syntax trees don't have specific bounds on tree depth, but in practice they roughly resemble a
"well balanced" tree with **log *n*** tree depth, so this (for well balanced trees) is a reduction
from ***O*(log *n* × log *n*)** to ***O*(log *n* × log log *n*)**. This is about as much better as
***O*(log *n*)** is than ***O*(*n*)**.

Caching is done in linear time on the number of direct children by caching based on identity.
This is possible due to pervasive caching. Additionaly, the only allocation involved in creating
a tree element is if the node is a newly seen node (both to create the node itself, and to store
it in the cache (the latter of which is amortized)); cache hits are allocation-free.

## Cool Things

While the main [builder cache][green::Builder] is bottom-up, we also provide a convenience top-down
[tree builder][green::TreeBuilder]. It handles the stack of elements required to build the tree, and
provides an API [specifically for pratt parsing][TreeBuilder::checkpoint].

The green tree supports full serialization and deserialization via [serde], even in
non-self-describing formats. (No deduplication is done in the serialized form, however.
Deduplication is done again at deserialization time.)

Complicated tree structures often suffer from recursive destructors. Sorbus implements destruction
with an explicit loop and stack, and thus doesn't risk overflowing a small stack with a large tree.
As a result, all of sorbus's tests can (and are 🎉) run under Rust's const evaluator, [miri],
which sanitizes for most forms of UB but has a restricted stack size compared to runtime.

## Future Work

While the builder does provide [simplistic garbage collection][Builder::gc], it's _very_ simple:
it just removes any elements that aren't referenced outside the cache itself. Nodes higher in the
tree are very unlikely to have identical duplicates, so we could skip caching them. We should
explore more interesting [cache strategies] alongside the current "cache everything" appraoch.

  [cache strategies]: <https://medium.com/@bparli/64dc973d5857>

> **Author Note:**  
> Although, perfect deduplication does make the "duplicated code" inspection trivial!
> Of the extra-information cache strategies, I expect Greedy Dual-Size to outperform a
> Least Frequently Used strategy (even with aging), as the node's textual length is a very
> decent predictor of duplication chance. But also because of that, I think a strategy of just
> not caching nodes with a textual length above some threshold (1024 bytes?) to do just as well.
> Plus, not have to store all the addtional state for the dynamic cache eviction. We just have
> to be careful that this doesn't hurt incrementality, as higher nodes _do_ get duplicated when
> a single file is reparsed with only minor edits.

Further incrementallity: while sorbus is a persistent, cached, and deduplicated data structure,
and thus inherently shares state when trees are reconstructed, it could better support one specific
form of incrementallity: partial or multi-pass parsing. Specifically, it'd be nice to be able to
parse a source file but only the "root level," leaving items unparsed until a query requires it.

> **Author Note:**  
> As of current, I'm yet unsure whether this needs specific support at the green (sorbus) level, or
> just the red (rowan) level. Either way, this is adding back in a form of mutability into the
> immutable tree, and the red tree pointers have very _interesting_ semantics (that I should write
> up a design document for once I've given the red tree the same rework and polish that I've given
> the green tree in sorbus), so it needs to be done carefully. I suspect the best design will end up
> being to store the root as [`ArcSwap`], using the already existing "replacement" API, and swapping
> the new node back in. This doesn't require any support at the green level. Using `ArcSwap`
> directly in the green tree is likely impossible because erasure uses unsynronized access.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE/APACHE](LICENSE/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE/MIT](LICENSE/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
