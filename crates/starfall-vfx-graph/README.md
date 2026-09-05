# starfall-vfx-graph

Typed authoring documents and a module-registry compiler for Starfall's
data-driven VFX systems ("Niagara-lite"): emitters, spawn rules, and
per-particle update modules that ship as data and run without an engine
recompile.

This crate has no renderer and no simulation loop — it defines
`VfxSystemDocument` (the serialized authoring format), a `ModuleRegistry` of
known module kinds with their parameter layouts, and
`VfxSystemDocument::compile_runtime` which validates a document against a
registry and produces a `CompiledVfxSystem` that a runtime (CPU simulation
today, optionally GPU compute later) can execute directly.

See `docs/guides/vfx-forge-plan.md` in the main repository for the full
architecture and phased rollout this crate belongs to.
