# Starfall Runtime

Dependency-free runtime services, also exposed as `starfall_i::runtime`:

- `budget::ResourceBudget`: synchronized admission, coherent counters, RAII release.
- `history::TickHistory<T>`: ordered rolling history with count and payload budgets.
- `stats::FrameSamples`: bounded wall-frame samples and nearest-rank percentiles.

The Heavy Water adapters use these for one-shot sound admission, fixed-input
diagnostics, and the F11 frame overlay. No renderer, native audio, game content,
or Bevy dependency enters the minimal facade. Byte limits in `TickHistory` use
caller-supplied payload sizes; they do not measure allocator overhead.

These are Starfall implementations. The crate remains unpublished and follows
Starfall's existing pending package-licensing policy. See
[Runtime foundations](../../docs/RUNTIME_FOUNDATIONS.md) for design requirements,
reference history, and integration limits.
