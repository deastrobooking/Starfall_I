# starfall-graph

`starfall-graph` is Starfall's engine-neutral typed graph kernel. It owns stable
graph identity, node and port metadata, deterministic graph documents, native
node registration, and structural validation.

It deliberately does not own Forge UI or domain execution. Games and native
extensions can use this crate directly; the `starfall-i` facade also re-exports
it as `starfall_i::graph`.

```rust
use starfall_graph::{GraphDocument, GraphDomain, StableId};

let graph = GraphDocument::new(
    StableId::new("my_game.behavior.open_door")?,
    GraphDomain::Behavior,
);

assert!(graph.nodes.is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Graph schema compatibility is versioned independently from the all-in-one
application. Domain compilers belong in their owning engine or gameplay module.
