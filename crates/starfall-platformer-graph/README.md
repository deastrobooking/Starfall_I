# Starfall Platformer Graph

This optional adapter connects the renderer-neutral `starfall-graph` kernel to
the renderer-neutral `starfall-platformer` kit. It provides typed route nodes,
a versioned JSON route document, deterministic compilation, and catalog-backed
runtime validation.

Games that author platformer routes in code can depend on
`starfall-platformer` alone. Graph-based tools add this crate; neither lower
level crate depends on it.

Run the complete code-authored graph → JSON document → catalog resolution →
validated runtime geometry example:

```sh
cargo run -p starfall-platformer-graph --example route_graph
```
