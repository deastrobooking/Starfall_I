# Starfall Forge Editor Upgrade Roadmap

> Active roadmap for evolving Forge toward Unity/Unreal-class authoring
> workflows while keeping the implementation native to Bevy and Rust. Last
> reconciled August 12, 2026.

## Target architecture

Forge should author a durable document, compile that document into disposable
ECS preview worlds, and publish a versioned runtime bundle. The Game edition
must consume only neutral schema/runtime crates and published output; it must
not depend on editor implementation modules.

Forge is also the native front end for Starfall's typed graph framework. One
shared graph kernel supplies IDs, ports, connections, commands, migrations,
validation, and compilation; Object, Behavior, Animation, UI, Shader,
World/City, Dialogue, Mission, and Campaign graphs remain specialized graph
languages. Native plugins can register nodes, inspectors, widgets, importers,
animation processors, shaders, validators, and publish steps without expanding
a closed central enum.

```text
content schema/assets <- runtime catalog <- game
content schema/assets <- editor document <- editor tools/UI
                              |
                              +-> compiler/publisher -> runtime bundle
```

The governing rules are:

- The serialized editor document is authoritative; ECS is a preview/projection.
- Every authored mutation crosses one command boundary and is undoable.
- Edit, Simulate, and Play-In-Editor use explicit, reversible world lifetimes.
- Shipped assets, project sources, imports, cache, and published output have
  separate runtime-configurable roots.
- Panels, inspectors, importers, validators, and builders register through
  contracts instead of extending one closed enum or monolithic system chain.

## M0 — Correctness and contracts (landed)

- Replaced ambiguous numeric controller orders with one stable action registry,
  deterministic navigation, visibility skipping, and panel-aware focus.
- Extended scene validation, dependencies, rename/delete guards, diagnostics,
  and draft hashing across every level while preserving the active-scene source
  compatibility contract.
- Added exact primary-manifest revisions and an OS-backed per-project writer
  lock so stale editor or specialized-Forge sessions cannot overwrite external
  tool writes; clean re-entry reloads changed data and recovery promotion is
  explicit.
- Preserved authored names and modifier stacks during duplication, removed
  failed stale undo/redo entries, and blocked viewport input during text entry.
- Cached disk-backed diagnostics and stopped reparsing every record source on
  unchanged editor frames.
- Gave newly created projects distinct serialized identities and prevented
  unregistered project files from being overwritten.
- Unified editor/Project Hub publishing around immutable hashed generations and
  atomic `current.json` promotion, with locked hash rollback, one consumer
  generation snapshot, durability warnings, and legacy flat-file fallback.
- Added a background Project Hub exporter that republishes, builds the
  Game-only release feature set, stages runtime assets plus exactly the selected
  generation, writes a versioned bundle manifest, and never replaces an older
  export.
- Added locked Designer/Game check, strict-Clippy, and test jobs to CI. Updated
  vulnerable transitive lockfile versions without changing Bevy or Avian.

M0 intentionally does not claim editor/runtime isolation, universal undo,
complete cooking, cross-platform packaging, or installers. Publish currently
bakes weapons, creatures, vehicles, spacecraft, dialogue, and platformer
routes, while runtime code still imports types through the `engine_tools`
namespace.

## M1 — Establish architectural seams

- Library target and thin `main.rs` launcher landed; continue moving
  application/plugin composition behind public framework contracts.
- Define neutral `content_schema` and `runtime_catalog` ownership; move runtime
  readers and publish-manifest types out of `engine_tools`.
- Compose explicit Platform/Core, GameRuntime, Presentation, EditorCore, and
  EditorUI plugin groups, then Heavy Water Shared/Open World/Platformer/Racer/
  RPG/Campaign groups. Give every resource and schedule edge one owner.
- Split `engine_tools/mod.rs` first by document/session, command/history,
  viewport, hierarchy/selection, registry, validation, and UI concerns.
- Add one headless workflow test: open project, edit, undo/redo, save/reload,
  publish, enter playtest, and return without contaminating the document.

Acceptance: the Game edition has no dependency on editor modules, both feature
sets pass the locked matrix, and the workflow test proves the first stable seam.

## M2 — Authoritative documents and universal commands

- Introduce serializable `EditorDocument` and document IDs/revisions; treat ECS
  entities as compiled preview instances.
- Route create, duplicate, delete, hierarchy, component, recipe, and level
  changes through one command journal with complete undo/redo.
- Add transactions, dirty revision tracking, autosave/journal recovery, and
  explicit multi-document conflict handling.
- Build hierarchy and reflected-component inspection on those contracts rather
  than direct ECS mutation.

Acceptance: every persistent edit has a round-trip and undo/redo test, stale ECS
entities cannot block history, and recovery never replaces a project with a
default document.

## M3 — Assets, import, cook, and packaging

- Separate immutable shipped assets, project sources, imported originals,
  derived cache, and published bundles through platform/project paths.
- Add typed Bevy loaders, dependency tracking, hot reload, cancellable imports,
  and content-addressed cache identities using BLAKE3 or SHA-256.
- Publish an atomic versioned bundle covering every supported content category;
  validate that Game boots and plays using the bundle alone.
- Evolve the landed host-native folder exporter into dependency-aware cooking,
  cross-target builds, signing, installers, patches, and product branding.
- Add migration fixtures, deterministic cook tests, package budgets, and stale
  generation cleanup policy.

## M4 — Edit, Simulate, and Play-In-Editor

- Give the authoring shell its own document/preview world.
- Compile Edit into a disposable Simulate world; clone or launch an isolated
  snapshot for Play-In-Editor.
- Define apply/revert-by-command behavior and guarantee teardown restores the
  exact pre-run authoring state.
- Add scene hierarchy, prefabs/variants, and reflected component inspection on
  the isolated document model.

## M5 — Extensible editor platform and scale

- UX foundation already landed: one shared movable/resizable/minimizable window
  shell, logical-coordinate fitting, pointer-routed scrolling and viewport
  capture, responsive full-screen creator surfaces, progressive default layouts,
  and geometry-aware shared controller focus.
- The current GUI architecture, public widget/tool-window API, a per-Forge-
  screen inventory, and known styling inconsistencies are now documented in
  [GUI_SYSTEM.md](GUI_SYSTEM.md) (2026-09-05). First alignment slice landed
  alongside it: a shared `forge_widgets::stepper_row` replaced Vehicle and
  Spaceship Forge's near-duplicate hand-rolled field-row widgets, and the
  `bevy::feathers::theme::UiTheme` / `ui_foundation::UiTheme` naming collision
  was resolved. Remaining slices (Character Studio/Imported Character Forge
  widget adoption, a shared spacing scale, `UiTheme` adoption outside
  `ui_plugin.rs`, a dedicated Dialogue Forge screen) are tracked in that doc's
  "Suggested next slices," ordered before the registries work below.
- **Registries — first slice landed (2026-09-06):** `ui_plugin.rs`'s Project
  Hub used to keep a Forge's label/accent in one table
  (`project_hub_authoring_actions`) and its `AppState` destination in a
  separately hand-matched function (`project_hub_forge_destination`) — two
  places that had to agree for every entry, the exact "hand-wired... instead
  of registration" pattern this milestone exists to remove. `ForgeHubEntry`
  merges them into one table; a destination-less entry (Imported Character
  Forge, whose click handler also sets `ImportedForgeReturnTarget`) is now an
  explicit `None` rather than an implicit omission. This is a small, single-
  file slice, not the registry system this bullet describes — a Bevy
  `PluginGroupBuilder` composition in `framework.rs` is still how a Forge
  plugin itself gets registered, and that is compile-time by nature; what
  moved here is the *discovery/UI* metadata a Forge needs to show up in the
  Hub. The remaining kinds below (panels, inspectors, importers, builders,
  validators, commands, shortcuts, persisted docking/layout) are unstarted.
  **Second slice (2026-09-06):** `forge_back_destination` was a *third*
  independent hardcoded list of the same 4 Forge states, this time paired
  with "goes back to `AppState::ProjectHub`" — now derived from the same
  `ForgeHubEntry` table instead of re-enumerating them. Same pattern, same
  file, same low-risk shape as the first slice; adding a Forge to the table
  now also gets its Back button right by construction.
- Add registries for tools, panels, inspectors, importers, builders, validators,
  commands, shortcuts, and persisted docking/layout.
- Add named workspace presets/reset, task-grouped Registry navigation, action
  availability with disabled reasons, consistent destructive confirmations,
  semantic accessibility labels/live regions, and keyboard window management.
- Add asset browser, console/profiler, background cancellable work, source-control
  hooks, UI/controller integration tests, and performance budgets.
- Split remaining `world_plugin.rs` and `ui_plugin.rs` hotspots along the same
  ownership contracts before expanding major authoring features.

## Immediate next slice

1. Add the library/composition seam without behavior changes.
2. Extract publish-manifest and runtime-catalog types into a neutral module and
   enforce that Game does not import `engine_tools`.
3. Introduce an `EditorDocument` facade and command gateway, then migrate object
   create/duplicate/delete as the first universal-history vertical slice.
4. Land the headless end-to-end Forge workflow test before adding new panels.
