# GUI System — Developer Reference

The authority for Starfall's GUI code: what exists, where it lives, its
public API, and where it is inconsistent today. Written 2026-09-05 as the
research/documentation half of the GUI alignment pass tracked in
[editor_roadmap.md](editor_roadmap.md)'s M5 milestone ("Extensible editor
platform and scale").

This document covers the **authoring/creator GUI** — Project Hub, the Forge
screens, and the World Kit Forge's in-game level editor: the "first-class GUI
system for our users" where "users" means people building games with
Starfall, not players of a finished game. The consumer-facing game menus and
HUD (`plugins/ui_plugin.rs`) are covered only where they share infrastructure
with the creator side. A future in-game **UI graph** — letting a designer
author their own game's HUD/menus as a Forge-compiled document, the way World,
Object, and Dialogue graphs already work — is named in
[editor_roadmap.md](editor_roadmap.md)'s target architecture but does not
exist yet; the "Where this is heading" section below records the seam it
would need.

## Two separate GUI worlds

There are two independent GUI systems in this codebase, and they share almost
nothing beyond `UiFoundationPlugin` (theme/i18n/scale) and the `AppState`
enum:

| | Consumer game chrome | Creator/authoring chrome |
|---|---|---|
| File | `src/plugins/ui_plugin.rs` (11,200+ lines) | `src/engine_tools/` + `src/plugins/*_forge_plugin.rs` + `src/character_studio/` |
| Screens | Main menu, pause, chapter select, player select, HUD, game over/victory | Project Hub, World Kit Forge (Outliner/Inspector/Registry), Weapon/Vehicle/Spaceship/Creature Forge, Character Studio, Imported Character Forge |
| Layout model | Fixed full-screen panels per `AppState`, `OnEnter`/`OnExit` | Floating, movable/resizable/minimizable tool windows (`spawn_tool_window`) |
| Public API | Only `UiPlugin` itself — everything else is private by design | `tool_windows`/`forge_widgets` are genuinely reusable; each Forge plugin still exposes only its `Plugin` marker |
| Feathers adoption | None | One `bsn!` scene, 5 text fields, in the World Kit Forge only |

`ui_plugin.rs` is documented here only for orientation — it is intentionally
a closed module (see "Consumer UI: `ui_plugin.rs`" below). The rest of this
document is about the creator/authoring half, where the reusable surface
actually lives.

One structural oddity worth knowing before you go looking for it: **the
Project Hub launcher screen lives inside `ui_plugin.rs`** (`setup_project_hub`,
`ProjectHubAction`), not next to the tools it launches. Its only job is to
route into `AppState`/Forge-plugin destinations
(`project_hub_forge_destination`). If you add a new Forge, its entry point in
the Hub is a `ProjectHubAction` variant in `ui_plugin.rs`, not a change in
`engine_tools`.

## The spacing scale — `src/engine_tools/gui_spacing.rs`

Added in this pass. Four constants — `XS = Val::Px(3.0)`, `SM = Val::Px(5.0)`,
`MD = Val::Px(8.0)`, `LG = Val::Px(12.0)` — replacing the near-identical but
independently-chosen gap/padding literals `tool_windows.rs` and
`forge_widgets.rs` used to carry separately (`6.0`/`5.0`/`8.0`/`3.0`/`12.0`
across the two files). Both now import it (`use super::gui_spacing;`); no
other screen does yet — see "Known inconsistencies" and "Suggested next
slices" below for what adopting it further would look like. There is
deliberately no `XL` step: add one when a real screen needs a wider gap, not
speculatively.

One value actually changed rather than just being renamed:
`forge_widgets::widget_row`'s `column_gap` moved from `6.0` to `SM` (`5.0`),
matching the row gap it already used — a 1px, imperceptible-in-practice
consolidation, and exactly the kind of homogenization this scale exists to
make possible. Everything else (`action_button`'s padding, `widget_row`'s
bottom margin and row gap, `tool_windows`'s content padding and row gap) maps
onto the new constants at its exact prior value.

## The tool-window shell — `src/engine_tools/tool_windows.rs`

The one genuinely mature, tested, documented piece of the creator GUI. Every
floating panel in every Forge screen and the World Kit Forge is built through
this module. It is screen-agnostic: there is no trait to implement and no
registry to join, just a function you call under your own UI root.

> "A tool window is an absolutely-positioned panel with a drag handle (its
> title bar), a minimize/restore button, a corner resize grip, and a
> scrollable content container. The interaction model follows professional
> DCC tools (Blender is the reference)." — the module's own doc comment

### Building a new tool window

```rust
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};

spawn_tool_window(
    &mut parent,                 // a ChildSpawnerCommands under your screen's own UI root
    "My Panel",                  // title bar text
    Vec2::new(12.0, 96.0),       // initial logical-pixel position
    ToolWindowStyle {
        accent: Color::srgb(0.14, 0.42, 0.62),   // tints the title bar / border
        ..Default::default()                      // 286px wide, 540px content, not minimized
    },
    (),                           // extra components/bundle on the content entity
    |content| {
        // Build your panel's contents here, same as any other ChildSpawnerCommands.
    },
);
```

That single call gives you: drag-to-move, drag-the-◢-grip-to-resize,
double-click-header-to-collapse (Blender-style), a `—` minimize/restore
button, mouse-wheel routed to whichever tool window is under the cursor
(`ComputedStackIndex`-based, so overlapping windows scroll independently),
automatic raise-to-front on any interaction inside the window (including
plain content buttons — the raise system walks `ChildOf` ancestry up to 64
levels to find the owning window), and viewport-fit protection: if the host
application window shrinks, every tool window shrinks/repositions rather than
stranding off-screen. All of it works in logical coordinates — drag/resize
math divides out `UiScale` — so it behaves the same at every UI scale setting.

### Public API

**Components** (attach automatically via `spawn_tool_window`; read them if a
system needs to react to window state):
- `ToolWindow { pub minimized: bool }` — the window root's chrome state.
- `ToolWindowTitleBar { pub window: Entity }` — the draggable header.
- `ToolWindowMinimizeButton { pub window: Entity }` — the `—`/restore button.
- `ToolWindowContent { pub window: Entity }` — the scrollable content column
  (this is the entity your `content` closure is called with).
- `ToolWindowResizeGrip { pub window: Entity }` — the corner ◢ handle.
- `ToolWindowChromeControl` — marks pointer-only chrome (title bar, grip) so
  it's excluded from `MenuFocus` controller/keyboard navigation; the minimize
  button deliberately does **not** carry this, since it's a real focusable
  action.

**Resource:**
- `ToolWindowPointerState { pub fn captures_viewport(&self) -> bool }` —
  **check this before starting any 3D viewport gesture** (camera orbit,
  gizmo drag, object picking). It's `true` whenever the pointer is over any
  tool-window chrome or content, so the viewport and the UI never fight over
  the same drag.

**System ordering:**
- `ToolWindowSystemSet { PointerState, Interaction, FitAndState }` — declare
  `.after(ToolWindowSystemSet::PointerState)` on any system that reads
  `ToolWindowPointerState` this frame (viewport picking/camera control in
  `engine_tools::mod` and `character_studio::mod` already do this).

**Configuration and the entry point:**
- `ToolWindowStyle { pub accent, pub background, pub width, pub content_height, pub initially_minimized }`
  — `Default` gives a blue accent, near-black translucent background, 286px
  width, 540px content height, not minimized. Each Forge screen derives its
  own `accent`/`background` from its theme color (see the per-screen table
  below) but otherwise mostly takes the defaults.
- `pub fn spawn_tool_window(parent, title, position, style, content_extras, content) -> Entity`
  — the one entry point; returns the window root entity.

**Constants:** `MIN_WINDOW_SIZE = (200.0, 140.0)`, `TITLE_BAR_HEIGHT = 36.0`,
`DOUBLE_CLICK_SECONDS = 0.35` — not public, but worth knowing if a panel looks
clipped: it can never shrink below `MIN_WINDOW_SIZE`.

**Plugin:** `ToolWindowsPlugin` — foundation-tier, registered in
`framework::StarfallFoundationPlugins` (loaded before any Forge screen, so
it's available even to code that only conditionally becomes reachable).

## The shared widget kit — `src/engine_tools/forge_widgets.rs`

The house look for panel *contents* (buttons, rows, labels), as opposed to
window *chrome* above. Deliberately generic over each tool's own action
component — "each forge keeps its own `Component` wrapping its own action
enum, so interaction systems stay per-tool and there is no shared action bus
to collide on" (the module's own doc comment).

```rust
use crate::engine_tools::forge_widgets::{action_button, widget_row, section_label, stepper_row, ForgeWidgetStyle};

fn widget_style() -> ForgeWidgetStyle {
    ForgeWidgetStyle { button_border: my_accent_color, text: Color::WHITE, ..Default::default() }
}

widget_row(parent, |row| {
    action_button(row, "SAVE", MyToolButton(MyAction::Save), &widget_style());
});
```

**`ForgeWidgetStyle`** — one instance per tool, usually derived from the
tool-window accent so buttons match their window's chrome:
- `button_background`, `button_border`, `text: Color`
- `font_size: f32` (default `13.0`)
- `min_width: f32` (default `104.0`), `min_height: f32` (default `36.0` — the
  accessibility-pass minimum touch/click target)
- `readout_text: Color` — muted tint for a [`stepper_row`] readout, dimmer
  than `text` so a displayed number reads as data rather than an available
  action (added in this pass; see "What changed in this pass" below)
- `readout_min_width: f32` (default `160.0`) — reserved width for a readout
  label so a stack of stepper rows keeps its +/− buttons aligned into a
  column

**Functions:**
- `pub fn action_button(parent, label, action: impl Bundle, style: &ForgeWidgetStyle)`
  — one button, carrying the caller's own action-marker bundle.
- `pub fn widget_row(parent, build: impl FnOnce(&mut ChildSpawnerCommands))`
  — a wrapping flex row; the standard layout unit inside a panel.
- `pub fn section_label(parent, label)` — a muted section heading.
- `pub fn stepper_row(parent, readout_marker: impl Bundle, decrement_action: impl Bundle, increment_action: impl Bundle, style: &ForgeWidgetStyle)`
  — a live numeric readout with `−`/`+` buttons: the standard "adjust one
  field" unit for spec-driven tools (Vehicle Forge, Spaceship Forge). Added in
  this pass to replace two near-identical hand-rolled copies (see below).

## Shared theming/i18n/scale — `src/plugins/ui_foundation.rs`

`UiFoundationPlugin` registers three cross-cutting resources, pulled in by
`UiPlugin::build` and available to any screen:

- `UiTheme` — the app's own semantic color palette (`canvas`, `panel`,
  `text_primary`, `text_muted`, `play`, `create`, `objective`,
  `health`/`armor`/`stamina`/`energy`/`climb`, `players: [Color; 4]` +
  `pub fn player_accent(&self, player_index: u8) -> Color`). Its doc comment
  calls it "shared by game screens and creator tools," but as of this
  writing it is used 11 times in `ui_plugin.rs` and **zero times** in any
  Forge/Character-Studio screen — every one of those screens hand-rolls its
  own `Color::srgb(...)` literals instead (9–31 per file). This is real,
  current drift, not a doc error to fix by rewriting the comment — see "Known
  inconsistencies" below.
- `UiTextKey` / `UiTextCatalog { pub fn text(&self, key) -> &str, pub fn set_override(&mut self, key, value) }`
  — i18n-ready copy indirection. English-only today; the indirection exists
  so a future locale swap doesn't need a source sweep.
- `UiPromptText(pub UiPromptKind)` + `UiPromptKind` — device-aware control
  prompts (keyboard vs. Xbox/PlayStation/Nintendo glyphs), refreshed by
  `track_ui_prompt_device`/`refresh_ui_prompt_text`.

**Naming collision to know about:** `bevy::feathers::theme::UiTheme` (Bevy's
own Feathers theme resource) has the exact same name as
`crate::plugins::ui_foundation::UiTheme` (this app's palette) and they are
otherwise unrelated. `engine_tools/mod.rs` imports Bevy's version — used only
as an `Option<Res<...>>` presence probe to detect whether Feathers is
installed, never for styling — aliased as `FeathersUiTheme` as of this pass
specifically so this doesn't read as "the app's palette" to a future reader.

## Feathers adoption — one corner, in trial

`bevy_feathers` is enabled (`Cargo.toml`, `bevy` feature list) and
`bevy::feathers::FeathersPlugins` is added in `StarfallAppMode::Production`
(`src/app.rs`), but actual usage in `src/` is exactly one site:
`engine_tools::mod::spawn_forge_editable_fields` — a single `bsn!` scene
powering 5 text fields (Outliner filter, Registry search, Content ID, Project
Name, Level Name) via `bevy::text::EditableText`. Every other screen — every
Forge, `ui_plugin.rs`, `character_studio`, `tool_windows.rs`,
`forge_widgets.rs` — is 100% hand-rolled Bevy UI (`Node`, `BackgroundColor`,
`BorderColor`, manual `Interaction` queries).

This is a deliberate, staged trial, not an abandoned migration:
`docs/bevy-0.19-adoption-plan.md` records "one reusable Feathers/`EditableText`
adapter now drives Outliner filtering, Registry search... next UI trial: add
project description/tags through the same adapter, then evaluate a Feathers
property row before converting any larger panel." Don't assume Feathers is
the house style yet when reading or writing GUI code — hand-rolled `Node`
trees are still the norm everywhere except those 5 fields.

## Per-screen inventory

| Screen | File | Approx. lines | Public API | Uses `forge_widgets`? | Tool-window accent |
|---|---|---|---|---|---|
| World Kit Forge (Outliner/Inspector/Registry) | `src/engine_tools/mod.rs` | 13,600+ | `EngineToolsPlugin`, `EngineToolMode`, `EditorSelection`, `EditorCommand`/`EditorTransaction`/`EditorUndoStack`, published-content catalogs (see below) | No — its own `spawn_editor_button`/`spawn_editor_panel` | — (3 fixed docking positions: `(12,96)`, `(497,96)`, `(982,96)`) |
| Weapon Forge | `src/plugins/weapon_forge_plugin.rs` | 1,229 | `WeaponForgePlugin` only | Yes | — |
| Vehicle Forge | `src/plugins/vehicle_forge_plugin.rs` | 1,414 | `VehicleForgePlugin` only | Yes (incl. `stepper_row` as of this pass) | cyan `srgb(0.08, 0.62, 0.74)` |
| Spaceship Forge | `src/plugins/spaceship_forge_plugin.rs` | 1,421 | `SpaceshipForgePlugin` only | Yes (incl. `stepper_row` as of this pass) | purple `srgb(0.44, 0.30, 0.82)` |
| Creature Forge | `src/plugins/creature_forge_plugin.rs` | 1,100 | `CreatureForgePlugin` only | Yes | — |
| Imported Character Forge | `src/plugins/imported_character_forge_plugin.rs` | 2,617 | `ImportedCharacterForgePlugin` only | Yes as of this pass — `forge_button`/`forge_row` are now thin wrappers over `action_button`/`widget_row` | cyan `srgb(0.28, 0.72, 0.92)` |
| Character Studio | `src/character_studio/mod.rs` | 2,507 | `CharacterStudioPlugin`, `StudioState`, `pub fn studio_spec_to_blueprint` | Deliberately not — see below | — |
| Project Hub (launcher) | inside `src/plugins/ui_plugin.rs` | — | (private; part of `UiPlugin`) | No — fixed full-screen menu, not a tool window | n/a |
| Dialogue Forge | *(does not exist as a separate screen)* | — | Reachable only as `EditorAction::CreateDialogueGraph`/`DialogueAddNode`/`DialogueCycleMode` inside the World Kit Forge's Registry panel | — | — |

**Character Studio's widgets are not adoption debt — they're a different,
intentional design.** `spawn_action_button`/`spawn_small_button` have no
border at all (`forge_widgets::action_button` always draws one), and
`spawn_small_button` is a fixed 36×36 icon button rather than a
min-width/min-height label button; `spawn_morph_slider` is a drag-value
slider with no equivalent widget shape in `forge_widgets` at all. These read
as a deliberately borderless, compact control language for a
procedural-sculpting tool with dozens of morph sliders on screen at once, not
as accidental drift from the same intended pattern the way Vehicle/Spaceship
Forge's field rows were. Forcing them onto `action_button` as it stands today
would either visibly add borders that were never there or require growing
`ForgeWidgetStyle` with an optional-border/fixed-size mode used by exactly
one tool — not attempted in this pass. If Character Studio's look is ever
meant to unify with the other Forges, that is a design decision to make
explicitly, not a drive-by refactor.

Every Forge plugin except Character Studio exposes **only its `Plugin`
marker struct** as public API — everything else (action enums, field enums,
systems, state resources) is private to the module by design, matching
`forge_widgets`' own stated philosophy of no shared action bus. This means
the genuinely reusable, genuinely public creator-GUI surface is entirely
`tool_windows.rs` + `forge_widgets.rs` + the selection/undo infrastructure in
`engine_tools/mod.rs` — everything else in the table above is a leaf
consumer of those three.

### The World Kit Forge's reusable, non-GUI infrastructure

`src/engine_tools/mod.rs` is far more than shared GUI foundations despite its
doc comment — it's the in-game level editor itself (`EngineToolMode::Editing`,
toggled from `AppState::Playing`) plus genuinely reusable, ECS-independent
editing infrastructure any future document-backed tool should build on:

- `EditorEntityId(pub u64)`, `EditorIdAllocator` (`allocate`, `reserve`),
  `EditorSelection` (`replace`, `toggle`, `clear`, `contains`, `active`,
  `iter`) — stable identity and selection for authored content.
- `EditorCommand` / `EditorCommandError`, `EditorTransaction` (`transform`),
  `EditorUndoStack` (`execute`, `undo`, `redo`, `undo_description`,
  `redo_description`, `clear`) — the generic undo/redo transaction system
  every Designer edit goes through.
- `Authorable` — marker for world objects the level editor can select/edit.
- Read-only runtime catalogs of published content: `PublishedMaterialCatalog`,
  `PublishedCreatureCatalog`, `PublishedDialogueCatalog`,
  `PublishedProceduralRecipeCatalog`, `PublishedMapPoint`/`PublishedMapPointKind`.
- Re-exported data-layer submodules: `character_records`, `creature_records`,
  `dialogue_records`, `editable_mesh`, `forge_widgets`, `game_export`,
  `mesh_selection`, `mesh_uv`, `platformer_prefabs`,
  `platformer_route_records`, `project_registry`, `publish`, `tool_windows`,
  `vehicle_records`, `weapon_records`, and `persistence` (5,770 lines: the
  `ForgeProject`/`ContentRecord`/`ProjectStore` schema and atomic-write layer
  every Forge's Registry panel and Save button ultimately calls into).

If you're building a new Forge-style tool, this is what you compose against:
`spawn_tool_window` for chrome, `forge_widgets` for panel contents,
`EditorUndoStack`/`EditorCommand` if your edits should be undoable, and
`persistence`/`project_registry` for save/load.

## Consumer UI — `src/plugins/ui_plugin.rs`

Covered briefly for completeness; this is *not* part of the creator-GUI
alignment work. It is a single 11,000+-line plugin (`UiPlugin`) owning every
consumer-facing screen — main menu, Project Hub launcher, pause menu, chapter
select, player select, HUD, game over/victory, controller diagnostics — with
almost no public API (`UiPlugin` itself, plus `pub(crate) MenuScrollPanel` /
`MenuButtonDisabled` and re-exports of `ui_foundation`'s theme/text/prompt
types). Every setup/despawn function, resource, and action enum inside is
private by design: this module is meant to be added, not extended from
outside. It registers `MenuUiSet { ReleaseActivation, FocusDispatch,
ActionConsumers }` and `ChapterSelectUiSet { TravelActions, LoadingFeedback }`
— a parallel, independent controller/keyboard focus pipeline
(`MenuFocus`-based) from the tool-window shell's pointer-only chrome, since
gamepad players never touch the creator tools.

## Known inconsistencies (current state, not yet fixed)

Found by direct comparison across screens, not by searching for TODO/FIXME —
none exist in the GUI files; the drift lives in the code, not in comments.

1. **`UiTheme` (the app's real palette) is essentially unused by the creator
   half of the app.** Every Forge/Character-Studio screen invents its own
   `Color::srgb(...)` literals (9–31 per file) instead of drawing from the
   one shared semantic palette that already exists for exactly this purpose.
2. **A shared spacing scale now exists** (`engine_tools::gui_spacing`:
   `XS`/`SM`/`MD`/`LG`) **and every gap/margin/padding literal in
   `forge_widgets.rs`, `tool_windows.rs`, and all five Forge-family screens
   now draws from it** (added across this pass). `character_studio` and
   `ui_plugin.rs` (253 raw `Val::Px(...)` literals) still invent their own
   values — see "Suggested next slices." `stepper_row`'s
   `readout_min_width`/`readout_text` fields remain a narrower, opt-in
   styling step on top of this, for values that aren't pure spacing.
3. **`forge_widgets` is adopted by 5 of 6 Forge-family screens** (Imported
   Character Forge and Creature Forge's stray `forge_row` converted this
   pass — see below). Character Studio remains the one holdout, deliberately
   — its borderless, fixed-size icon buttons and slider are a different
   control language, not drift from the same pattern (see the per-screen
   inventory table's note).
4. **The Project Hub launcher lives inside `ui_plugin.rs`**, architecturally
   separated from the tools it launches into. Anyone adding a new Forge entry
   point needs to know to look there, not in `engine_tools`.
5. **Dialogue Forge has no dedicated screen** — it's a handful of actions
   inside the World Kit Forge's Registry panel, unlike every other content
   type, which gets its own Forge plugin and tool windows.

## What changed in this pass (2026-09-05 – 2026-09-06)

- Fixed the `UiTheme` naming collision: `engine_tools/mod.rs` now imports
  Bevy's Feathers theme resource as `FeathersUiTheme`, not `UiTheme`, so it
  can never be misread as this app's palette in code or docs.
- Added `ForgeWidgetStyle::{readout_text, readout_min_width}` and
  `forge_widgets::stepper_row` — a real, generic replacement for two
  near-identical hand-rolled implementations
  (`vehicle_forge_plugin.rs::spawn_field_row` and
  `spaceship_forge_plugin.rs::spawn_field_row`/`spawn_system_row`, which
  differed only in a 4px width and a barely-perceptible color tint). Both
  screens' local functions are now thin wrappers over the shared primitive;
  call sites and visuals are unchanged (verified: same widths/colors
  preserved per-tool via their `widget_style()`).
- Converted `imported_character_forge_plugin.rs`'s `forge_button`/`forge_row`
  into thin wrappers over `forge_widgets::action_button`/`widget_row` (its own
  `widget_style()`, `min_width: 0.0` to preserve content-hugging buttons). Two
  small, deliberate, and low-risk visual changes come with this: padding
  shifts from 4px to the shared 3px vertical inset, and inter-button spacing
  moves from explicit 2px button margins to `widget_row`'s 5px column-/row-gap
  — a slightly *larger* gap, and both changes make this screen's buttons
  match the spacing already shipped in Weapon/Creature Forge.
  `forge_row`'s `width: 100%` was dropped in favor of `widget_row`'s implicit
  flex-stretch, which is what the four already-converted Forge screens already
  rely on inside the same `ToolWindowContent` container — not independently
  screenshot-verified for this screen specifically, so give the Imported
  Character Forge a look after this lands.
- Investigated Character Studio and concluded its widgets should **not** be
  force-converted (see the per-screen table) — a design judgment, not a gap.
- Added `engine_tools::gui_spacing` (`XS`/`SM`/`MD`/`LG`) and migrated
  `forge_widgets.rs` and `tool_windows.rs` onto it — see "The spacing scale"
  section above for the one value that actually changed (`widget_row`'s
  column gap, 6px→5px).
- Adopted `gui_spacing` in the remaining Forge screens: found and converted
  `creature_forge_plugin.rs`'s own stray `forge_row` (identical shape to
  Imported Character Forge's pre-conversion `forge_row` — same fix, same
  reasoning) into a `widget_row` wrapper, and mapped `weapon_forge_plugin.rs`'s
  two remaining hand-rolled Nodes (a spec-text bottom margin, and a
  library-list row's gaps/margin) onto the scale, consolidating 6px→`SM`
  (5px) and 4px→`XS` (3px) in the process. Vehicle, Spaceship, and Imported
  Character Forge had no remaining local spacing literals to convert — this
  closes out "adopt `gui_spacing` in the Forge screens" for all five.
- **First M5 registry slice**: merged the Project Hub's two hand-kept-in-sync
  per-Forge facts (`project_hub_authoring_actions`'s label/accent,
  `project_hub_forge_destination`'s separate match on `AppState`) into one
  `ForgeHubEntry` table. `project_hub_forge_destination` is now a lookup
  against that table instead of an independently-maintained match — behavior
  identical (same 4 destinations, same `None` for Imported Character Forge).
  See `editor_roadmap.md`'s M5 section for what this does and doesn't cover.
- This document.

## Suggested next slices (not started)

Ranked by leverage, cheapest first — each is independently shippable:

1. **Push `gui_spacing` further into `character_studio` and eventually
   `ui_plugin.rs`.** The five Forge-family screens are done as of this pass
   (see "What changed" below) — every remaining hand-rolled gap/margin/
   padding literal in the GUI is in those two files (`ui_plugin.rs` alone
   still has 253 raw `Val::Px(...)`). Bottom-up, screen by screen.
2. **Adopt `UiTheme` in at least one Forge screen** as a proof that the
   palette generalizes beyond `ui_plugin.rs`, before mandating it everywhere.
3. **Give Dialogue Forge its own screen** using the same `spawn_tool_window`
   + `forge_widgets` pattern as Weapon/Vehicle/Spaceship/Creature Forge,
   closing the one content type that doesn't get first-class tool windows.
4. **If Character Studio's look should ever unify with the other Forges**,
   grow `ForgeWidgetStyle`/`action_button` with an explicit opt-in
   borderless/fixed-size mode first, as its own reviewed decision — don't
   retrofit it silently as part of an unrelated pass.
5. **Registries for tools/panels/inspectors** per `editor_roadmap.md` M5 —
   the actual extensibility jump (a new Forge screen becomes a registration,
   not a new hand-wired plugin-group entry in `framework.rs`). Substantially
   bigger than the above; treat as its own milestone.

## Where this is heading: a future UI-authoring graph

`editor_roadmap.md`'s target architecture already names **UI** as one of the
specialized graph languages Forge's shared graph kernel (`starfall-graph`,
the same crate `starfall-platformer-graph` and `starfall-vfx-graph` build on)
is meant to support, alongside Object, Behavior, Animation, Shader, World/City,
Dialogue, Mission, and Campaign graphs. That would let a designer author their
own game's in-game HUD/menus/dialogs as a compiled document — the in-game
analog of everything this document covers for the *authoring* GUI.

That doesn't exist yet, and nothing in this pass builds it. But two things
from today's survey are worth keeping in mind so the eventual UI graph has
somewhere sane to land:
- `forge_widgets`/`tool_windows` are Bevy-UI-shaped, not compiled-document
  shaped — a UI graph's *runtime* would need its own render adapter (likely
  living beside `ui_plugin.rs`'s HUD code, not replacing `forge_widgets`,
  which is specifically for the *authoring* chrome).
- `ui_foundation::UiTheme`/`UiTextCatalog` are exactly the kind of
  semantic-not-literal contract a UI graph's compiled output should target
  (a graph node picks `theme.health`, not a raw color) — getting Forge
  screens to actually use `UiTheme` (Suggested next slice #3) is a small
  step in the same direction a UI graph would need anyway.
