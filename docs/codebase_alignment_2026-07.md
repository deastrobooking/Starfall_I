# Codebase Alignment Review — July 17, 2026

> Dated review snapshot. Its accepted changes are reflected in the living
> `current_state.md`, architecture, systems reference, and roadmaps.

This snapshot reconciles the broad code-review suggestions gathered from
smaller agents with the repository as it exists now. It is evidence for the
living roadmaps, not a second backlog.

## Review outcome

The suggestions correctly identified maintainability and reliability themes,
but several high-priority claims described older code or counted test-only
patterns as production defects. The safe immediate work was reliability at
well-defined boundaries; large file moves remain staged behind tests.

### Delivered in this alignment pass

- Crash reports now use the same platform-owned writable data root as saves,
  have timestamped filenames, retain the newest five reports, and remove local
  home/install prefixes before a report is written.
- Terrain-height and sky-road corridor caches now retain at most four world
  seeds with least-recently-used eviction. Both cache paths recover a poisoned
  lock instead of terminating the game.
- Empty L-system rule symbols are ignored instead of reaching a constructor
  panic. A regression test covers malformed rule input.

### Findings already resolved or not reproducible

- Terrain is not rendered as one monolithic mesh. It uses 64 high/low render
  patches over the unchanged full-resolution terrain collider.
- Building skyline clustering is not rebuilt every frame. It is constructed by
  the chained world setup systems on entry to `AppState::Playing`.
- No production `get_single` or `get_single_mut` calls remain. A raw count of
  `unwrap`, `expect`, `panic`, and similarly named APIs is dominated by tests
  and cannot be treated as a defect count without reachability review.
- Save loading already rejects unsupported future schemas, skips corrupt
  rotating slots, selects the newest valid generation, migrates legacy data,
  and clamps restored per-player selections and runtime stats.
- World-plugin extraction has started: the speed-road subsystem already lives
  in `src/plugins/world_plugin/roads.rs`.

## Aligned next work

1. The app-construction seam and headless construction/Startup/steady-frame
   smoke tests are delivered. Use them before and during broad module moves.
   The project remains a binary crate, so root integration tests still require
   a deliberate library boundary.
2. Continue mechanical hotspot extraction in reviewable slices: terrain,
   settlements, and dungeons from the world plugin; screens and HUD sections
   from UI; then isolated Forge editor panels. Each move must preserve system
   ordering and pass the full verification gate.
3. Expand `GameSet` assignment incrementally as fixed-tick gameplay migrates.
   The ordering scaffold exists; pretending all systems already participate
   would provide false determinism.
4. Separate macOS raw-controller acquisition from safe input translation, then
   unit-test the translation layer and complete four-controller hardware
   acceptance.
5. Measure N11 large-world entity, simulation, and split-screen render costs
   before adding further streaming or cache complexity.
6. Revisit derived health and typed persisted identifiers only through an
   explicit save-schema migration. These are data-ownership changes, not safe
   cleanup edits.

## Change policy

New cleanup work belongs under an existing roadmap milestone or a named review
slice. Avoid repository-wide rewrites and unaffiliated TODO comments. Prefer a
small behavior-preserving extraction, a regression test, and one measured
acceptance criterion at a time.
