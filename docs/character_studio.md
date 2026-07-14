# Advanced Character Studio

The Character Studio is Starfall I's player-facing humanoid authoring tool. It
uses a constrained, non-destructive workflow inspired by RPG character makers
and Blender's turntable/model-inspection tools. Players edit a compact
`CharacterSpec`; procedural generators rebuild the preview. They never edit
runtime mesh entities directly.

## Player workflow

1. Choose a male/female base and optional build or face preset.
2. Sculpt body and face proportions with sliders or precise `-` / `+` steps.
3. Use `R` on a row to restore only that field, or RESET ALL to return to a
   neutral model. Both are protected by the 64-step undo history.
4. Inspect the design with FRONT, PROFILE, BACK VIEW, FULL BODY, and FACE
   CLOSE-UP. Right-drag/right-stick provides free orbit; wheel/triggers zoom.
5. Mix wardrobe slots and colors. Palette fields show their actual color next
   to the value instead of only a numeric index.
6. SAVE VERSION writes a new JSON preset. Existing versions remain available
   through the scrollable library.

Controller layout:

- D-pad or left stick: navigate rows; Left/Right changes the focused value.
- South / A / Cross: activate the focused button.
- West / X / Square: reset the focused sculpt or style field.
- Left/Right shoulder: jump between presets, body, face, wardrobe, and library.
- East / B / Circle: return to the standard character designer.
- Right stick: orbit; triggers: zoom.

## Architecture

The authoring path is:

```text
UI controls -> CharacterSpec -> preset generators -> CharacterPatch -> procedural meshes
```

- `spec.rs` is the serialized source of truth. Morph values are normalized to
  `0.0..=1.0`, with `0.5` as neutral.
- `generators.rs` turns stable named properties into morph, material, and
  wardrobe-slot patches.
- `human_mesh.rs` builds the current mathematical preview. The named patch
  boundary is also where future rigged GLB shape keys can be connected.
- `mod.rs` owns the editor workspace, input, undo, camera, library, and preview
  rebuild scheduling.

Keeping the saved format parametric is important: a player-created character
can remain animation-, equipment-, and gameplay-compatible after the rendering
implementation is upgraded.

## Review and next milestones

The studio now has a viable modeling loop, but it is not yet a Blender-style
mesh editor. The next upgrades should preserve constrained output while adding
more visual authoring power:

1. Add collapsible Body, Face, Hair, Wardrobe, and Materials tabs with named
   presets and thumbnail grids. This is the largest remaining RPG-maker UX gap.
2. Replace the procedural preview with a rigged humanoid base containing named
   shape keys. Bind the existing patch names first so old JSON presets load.
3. Add region selection in the viewport: selecting the head, torso, arm, or leg
   should open the matching parameter group and outline that region.
4. Add paired/asymmetric controls, including eye spacing, ear shape, hand/foot
   scale, arm/leg length separately, and optional left/right details.
5. Add animation preview poses (idle, run, jump, attack) and equipment clearance
   checks before a character can be accepted into the game.
6. Add named character projects, duplicate/rename, thumbnail capture, and an
   explicit APPLY TO PLAYER SLOT action. Keep version history for recovery.
7. Export generated characters through a tested glTF/GLB path only after rig,
   materials, skin weights, and animation compatibility are stable.

Free vertex sculpting is deliberately not the immediate target. Unrestricted
topology edits would make animation, collision, clothing, and multiplayer asset
budgets unreliable; shape-key and part-based modeling provides the useful
creative range while keeping authored characters game-ready.
