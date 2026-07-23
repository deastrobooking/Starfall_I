# Advanced Character Studio

The Character Studio is Starfall I's player-facing humanoid authoring tool. It
uses a constrained, non-destructive workflow inspired by RPG character makers
and Blender's turntable/model-inspection tools. Players edit a compact
`CharacterSpec`; procedural generators rebuild the preview. They never edit
runtime mesh entities directly.

## Player workflow

1. Choose a male/female base or a gameplay archetype: STAR HERO,
   SHADOW RAIDER, MANA ADVENTURER, STREET RUNNER, or MECHA ROBOT. Presets are
   editable seeds, never locked character classes.
2. Sculpt body and face proportions with sliders or precise `-` / `+` steps.
3. Use `R` on a row to restore only that field, or RESET ALL to return to a
   neutral model. Both are protected by the 64-step undo history.
4. Inspect the design with FRONT, PROFILE, BACK VIEW, FULL BODY, and FACE
   CLOSE-UP. Right-drag/right-stick provides free orbit; wheel/triggers zoom.
5. Mix wardrobe slots and colors. Palette fields show their actual color next
   to the value instead of only a numeric index.
6. SAVE VERSION writes a new JSON preset. Existing versions remain available
   through the scrollable library. USE IN GAME assigns the exact generated
   model to the active local-player slot.

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

## MVP visual library

The generated base deliberately targets a broad 1980s cel-anime language:
larger cranium, tapered lower face, wide eyes with upper lashes and catchlights,
small readable nose, warm cheek marks, and graphic hair silhouettes. Eye
spacing, tilt, and depth; brow angle; nose bridge; chin width; and lip fullness
join the original face controls. The mouth has separate cavity and
upper/lower-lip geometry. Generated
eyes, brows, and mouth carry semantic feature tags so runtime animation can
blink, emote, breathe, and open the mouth during attacks independently of the
head. These are original procedural forms and are not tied to one show or
artist.

The current face pass adds Face Length, Eye Shape, and Nose Tip controls. Eye
whites now use an original shallow parametric lens whose signed-power outline
interpolates from rounded to pointed almond shapes. Skin-toned lower lids,
short outer lashes, broad shallow cheek planes, a narrower bridge/tip nose, and
restrained lip volumes keep the face cohesive in front and three-quarter view.
This follows the same compact-parametric principle as Alan Barr's
[superquadric formulation](https://authors.library.caltech.edu/records/rtr62-f2882/latest)
and the low-control-cage/smooth-limit-surface rationale documented by
[OpenSubdiv](https://graphics.pixar.com/opensubdiv/overview.html), while all
forms and code remain original to Starfall.

The MECHA ROBOT preset preserves the generator's successful armored silhouette
as a first-class character seed and now combines Crystal Mecha armor with mecha
wings. STAR HERO and SHADOW RAIDER provide readable good-guy/bad-guy contrast;
MANA ADVENTURER uses an arcane coat and moon gem for compact shared-camera
fantasy play; STREET RUNNER demonstrates the city clothing stack.

Wardrobe options currently include twelve top silhouettes, eight bottoms, nine
footwear silhouettes, nine hairstyles, gloves/gauntlets, and four full armor
overrides. The Arcane Coat and Star Knight Tunic add fantasy layering to the
existing modern wardrobe. Mecha Armor, Crystal Mecha, and Dragon Mecha provide
distinct armor suites. An independent fantasy-flair field offers None, Star
Gem, Moon Gem, Royal Mantle, Arcane Halo, and Mecha Wings without consuming a
wardrobe slot. Clothing uses separate cloth, denim, leather, sole, metal, and
emissive material responses.

Chest Shape is an explicit neutral morph shared by both bases. It drives fitted
clothing depth and sex-aware procedural chest definition: broader pectoral
planes on the male base and softer paired forms on the female base. The control
remains editable on either body and does not change gameplay collision.

Every non-armor character receives a fitted coverage garment around the pelvis
and upper thighs before outer clothing is generated. This is an invariant of
the mesh builder, not a UI convention, so randomization and old presets cannot
create an accidentally uncovered model.

## Unified procedural/imported rig MVP

The studio has two visual backends feeding one animation contract. **GENERATED**
builds the editable procedural character. **RIG TEST** loads
`Characters/AMP.glb`, discovers its bone hierarchy after scene instantiation,
and maps it atomically into the same 17-joint `SkeletonRig` used by procedural
characters. Imported joints preserve authored translation, rotation, and scale;
the runtime builds rest-pose-adjusted MVP clips for that rig and enables the
same `CartoonAnimator`, gameplay pose selection, hand state, IK, and sockets.

Character Studio reports `Pending`, `Ready`, or `Invalid` import status. A rig
is rejected rather than partially animated when required joints are missing,
multiple bones map to one canonical joint, or the hierarchy cannot be resolved.
If the scene asset itself fails to load, the generated preview is restored.

The repository asset audit found that AMP has one 40-bone skin and covers all
17 Starfall gameplay joints, but contains no animations and no morph targets.
Antonio, Chroma, Daria, and Vincenzo currently contain static meshes without a
skin. RIG TEST is therefore a diagnostic integration seam, not a replacement
for the editable generated model yet.

Production Blender/export bone names:

```text
SF_PELVIS  SF_SPINE  SF_CHEST  SF_NECK  SF_HEAD
SF_SHOULDER_L  SF_ELBOW_L  SF_WRIST_L
SF_SHOULDER_R  SF_ELBOW_R  SF_WRIST_R
SF_HIP_L  SF_KNEE_L  SF_ANKLE_L
SF_HIP_R  SF_KNEE_R  SF_ANKLE_R
```

`rig_bridge.rs` accepts AMP's existing names plus common Mixamo, Blender
Rigify, and Unreal-style upper-arm/forearm/hand/thigh/calf/foot aliases. Saved
gameplay data references canonical joints only, never vendor bone names.

Production shape keys must use the existing patch names:

```text
body_height body_muscle body_weight shoulders_wide waist_width hips_wide limb_length
body_chest_shape face_length face_jaw_wide face_chin_long face_chin_wide
face_nose_long face_nose_wide face_nose_bridge face_nose_tip face_brow_heavy
face_cheek_full face_eye_large face_eye_shape face_eye_spacing face_eye_tilt
face_eye_depth face_brow_angle face_mouth_wide face_lip_full
```

This contract means a future Blender model can replace the preview renderer
without changing `CharacterSpec` JSON or invalidating saved player designs.

## Review and next milestones

The studio now has a viable modeling loop, but it is not yet a Blender-style
mesh editor. The next upgrades should preserve constrained output while adding
more visual authoring power:

1. Add collapsible Body, Face, Hair, Wardrobe, and Materials tabs with preset
   presets and thumbnail grids. This is the largest remaining RPG-maker UX gap.
2. Author a production humanoid base using the documented 17-bone and
   shape-key contract. Runtime rig discovery, validation, rest-pose retargeting,
   shared animation, and fallback are complete; AMP still lacks editable shape
   keys and authored animation clips.
3. Add region selection in the viewport: selecting the head, torso, arm, or leg
   should open the matching parameter group and outline that region.
4. Add paired/asymmetric controls for ear shape, hand/foot scale, arm/leg length
   separately, and optional left/right details.
5. Expand semantic facial animation from procedural blink/shout to expression
   channels (joy, fear, anger, pain, phonemes), then add animation preview poses
   and equipment-clearance checks before acceptance.
6. Add named character projects, duplicate/rename, thumbnail capture, and an
   explicit APPLY TO PLAYER SLOT action. Keep version history for recovery.
7. Upgrade generated GLB export from rigid preview geometry to a skinned
   canonical skeleton with weights, sockets, morphs, and optional animations.

The next rig-tooling phase is an import wizard that persists one reusable rig
profile per asset: detected convention, manual mapping overrides, axis/unit
normalization, T/A-pose calibration, root-motion policy, socket offsets, and
validation results. After that come animation layers/masks, blend trees,
timeline events, IK gizmos, retarget comparison, and equipment-clearance tests.

Free vertex sculpting is deliberately not the immediate target. Unrestricted
topology edits would make animation, collision, clothing, and multiplayer asset
budgets unreliable; shape-key and part-based modeling provides the useful
creative range while keeping authored characters game-ready.
