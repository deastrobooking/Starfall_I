# Imported Mesh Editor — selection and modular parts

Imported Character Forge is the non-destructive editor for external `.glb`
characters. Open it from Project Hub, then drop a GLB into the window or load
the AMP sample. Imports outside `assets/` are copied to
`assets/imported_characters/`; the original file is not modified.

## Select a mesh region

1. Choose a named mesh, then cycle to the required primitive.
2. Hold Alt and click a triangle in the 3D preview. Shift+Alt-click selects the
   clicked triangle's linked island. Face stepping and toggle provide a precise
   fallback.
3. Expand the selection with **LINKED**, **GROW**, **SIMILAR NORMAL**, or
   **MIRROR X**. Use **SHRINK**, **INVERT**, **ALL**, and **NONE** to refine it.
4. Check the orange overlay. Selection IDs always refer to triangles in the
   authored source primitive, not a modifier-generated preview.

**LINKED** follows faces sharing vertices, **GROW** adds the one-ring
neighborhood, **SHRINK** removes the boundary, **SIMILAR NORMAL** selects faces
within the editor's normal-angle tolerance, and **MIRROR X** adds the nearest
face to each reflected face center.

## Assign animation and gameplay meaning

Cycle **PART PRESET**, then press **ASSIGN SELECTION**. Presets cover the core
body, head, hair, limbs, hands, feet, shoulders, cape, accessory, weapon, and
hurtbox cases. A part assignment contains:

- a stable `part_id` and display name;
- the source GLB, mesh, primitive, and sorted source face indices;
- a modular character slot;
- a canonical animation joint;
- a gameplay role such as visual, weapon, or hurtbox;
- a local pivot calculated from the selected face centers.

Faces cannot overlap two assignments in the same source primitive. This makes
animation ownership and gameplay hit-region ownership deterministic.

## Reuse parts

**SAVE TO LIBRARY** writes the selected assignment as an
`imported_modular_part` record in the active Forge project. **ADD LIBRARY PART**
adds the next reusable record to the current imported-character spec. Library
assignments retain their own source GLB, so a character spec can mix parts
authored from different characters without copying mesh buffers.

Saving the character writes `imported_character_spec` schema v2. Older schema
v1 records still load because the assignment list defaults to empty. Imported
character browsing excludes standalone part-library records.

## Current boundary

This slice authors and validates selection/part metadata. It does not yet split
the source GLB, rewrite skin weights, or spawn a multi-source runtime character.
Skinned and morphed assets remain intact, and the editor uses static
display-only copies for safe highlighting. Runtime assembly and deform-aware
export should consume the saved source references, joints, pivots, and face
sets rather than changing this persistence contract.
