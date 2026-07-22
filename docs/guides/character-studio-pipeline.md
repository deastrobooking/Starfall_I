# Character Studio — the generator pipeline

In-game human generator (`AppState::CharacterStudio`, `src/character_studio/`).
Boot straight in with `STARFALL_STUDIO=1`, or enter from Player Select
("ADVANCED") / the Robot Builder screen. Full mouse + gamepad GUI.

## One-way data flow

```
CharacterSpec  (spec.rs — the ONLY source of truth; serialized to JSON)
   │  UI buttons / presets / randomize edit this and nothing else
   ▼
build_character_patch()  (generators.rs — Body/Face/Skin/Hair/Outfit generators)
   ▼
CharacterPatch  (named morphs + named material colours + slot contents)
   ▼
spawn_human()  (human_mesh.rs — pure math meshes; no external assets)
   ▼
preview entity / PlayableStudioHuman (in-game player integration)
```

Never mutate entities from UI code; never serialize meshes — always the spec.
The named-morph layer is the forward-compat contract: a rigged, shape-keyed
`.glb` base uses the same names for Bevy `MorphWeights`, so editor and preset
data remain independent from the visual backend.

## Shared rig and animation path

Procedural skeletons and imported skinned GLBs both resolve to the canonical
17-joint `SkeletonRig`. Imported scenes are mapped after `WorldInstanceReady`;
binding is atomic and preserves each bone's authored local rest transform. The
MVP graph is rebuilt against imported rest rotations, while gameplay continues
to select the same `CartoonPose` states and apply the same hand and IK layers.

The imported adapter recognizes Starfall, AMP, Mixamo, Blender Rigify, and
Unreal-style naming. Character Studio exposes pending/ready/invalid diagnostics
for missing joints, duplicate targets, and broken hierarchy paths.

## Common tasks

**Add a body/face morph** — add the field to `BodySpec`/`FaceSpec` and
`MorphField` (spec.rs), emit it in `BodyGenerator`/`FaceGenerator` with a
`snake_case` name, consume `patch.morph("name")` in `human_mesh.rs`
proportions. The editor picks up `MorphField::ALL` automatically.

**Add a wardrobe style** — extend the enum (`TopStyle`/`BottomStyle`/
`FootStyle`/…) + its `ALL` + `label()`, then give it geometry in
`human_mesh.rs`'s outfit section. Style rows cycle via `StyleField` — no UI
work needed.

**Add a preset button** — a `fn preset_x() -> CharacterSpec` or
`apply_x(&mut spec)` in generators.rs, then one entry in the studio's
`ACTION_GROUPS` (mod.rs).

**Saves** — versioned, never overwriting: `save_new_version()` writes
`assets/presets/humans/human_vNNN.json`; the library panel lists LOAD/DELETE
rows live. Ship-ready presets are committed alongside (v003/v004).

**Sex differences** are baseline proportions inside `human_mesh.rs`
(shoulders/hips/waist/jaw/chest), *on top of* which all morphs apply — keep it
that way so every slider works for both bodies.

**Fantasy flair and mecha variants** — `StyleSpec::flair` is independent from
wardrobe slots, so gems, mantles, halos, and wings can layer over clothing or
armor. Mecha Armor, Crystal Mecha, and Dragon Mecha share the `Mecha` slot
contract and add variant geometry in `spawn_mecha_armor()`. New serialized body
or style fields must retain `serde(default)` compatibility for old presets.

## GLB export

The **EXPORT GLB** button (beside SAVE VERSION) walks the live preview
hierarchy and writes a hand-rolled glTF 2.0 binary to
`assets/presets/humans/human_vNNN.glb` (versioned, never overwrites; numbering
independent of the JSON specs). Exports exactly what's on screen: every part
as a node+mesh+`pbrMetallicRoughness` material with root-relative matrices.
Writer: `src/character_studio/glb_export.rs` (pure, 3 unit tests — container
layout, JSON round-trip, empty-input). Files open in Quick Look/Blender/Bevy.

## Gaps to know about

- Generated GLB export is still rigid preview geometry; it does not yet emit
  skin weights, the canonical skeleton, morph targets, sockets, or clips.
- Imported rig mapping is automatic in the MVP. Persistent manual overrides,
  T/A-pose calibration, axis/unit correction, and an import wizard are next.
- AMP validates and animates through the shared runtime but has no morph targets
  or embedded authored animation library.
