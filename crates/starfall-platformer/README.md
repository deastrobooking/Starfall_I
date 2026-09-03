# starfall-platformer

`starfall-platformer` is the reusable, renderer-neutral platforming contract.
It owns prefab gameplay intent, movement-envelope math, chunk sockets and
pieces, validation diagnostics, and deterministic route assembly.

It does not own meshes, materials, ECS entities, game enemies, rewards, or
Heavy Water's themed chunk catalogue. Consumers translate their movement
controller into `MovementProfile`, validate authored chunks, and compile the
result into their own runtime representation.

Stock prefab and theme values are conveniences, not closed registries. Native
extensions may use `PlatformerPrefabKind::Custom` and `ChunkTheme::Custom`,
then supply their own rendering and runtime adapters.
