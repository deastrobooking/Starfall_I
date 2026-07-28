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

Saving the character writes `imported_character_spec` schema v5. Older v1-v4
records still load because assignment, material, and UV lists default to empty.
Imported character browsing excludes standalone part-library records.

## Color and texture a primitive

The preview uses the GLB's authored material by default. In **COLOR + TEXTURE**:

1. Use **NEXT COLOR** to create an override and cycle the curated palette.
2. Cycle **PARAM**, then adjust metallic, roughness, or emission with `−`/`+`.
3. Cycle **TEXTURE CHANNEL** to base color, normal, packed metal/roughness, or
   emissive.
4. Drop a PNG or JPG anywhere in the editor. External files are safely copied
   to `assets/imported_textures/` and assigned to the armed channel.
5. **CLEAR MAP** explicitly removes that channel, including an authored GLB
   map. **RESTORE AUTHORED** deletes the complete primitive override.

Base-color maps are multiplied by the selected tint. Packed material maps use
glTF's convention: roughness in green and metallic in blue. Assigning one sets
both scalar multipliers to `1`; assigning an emissive map raises a zero
emission strength to `1` so the map is visible.

Material overrides use the same source GLB, mesh, and primitive key as region
assignments. Saving a reusable part includes its matching override, so adding
that library part to another character also carries its authored appearance.

## Project UVs and paint selected faces

The **UV + FACE PAINT** window provides authored UV preservation, planar XY/XZ/
YZ projection, cylindrical-Y projection, box projection with split hard seams,
U/V scale and offset, rotation, and bounded color strokes over the current face
selection.

Box projection duplicates indexed vertices only in the derived mesh so each
triangle can own seam-safe coordinates. Face paint uses vertex colors and
stable source face IDs; deselect the orange topology overlay to inspect it.
**RESTORE UV** removes projection, transforms, and paint.

**MARK BOUNDARY SEAM** records the outer edges of the selected face region,
switches to manual seam unwrap, separates connectivity at those edges, chooses
a projection per island, and packs islands into padded grid cells.
**CLEAR SEAMS** returns to authored UVs. **BAKE PAINT PNG** rasterizes all
ordered strokes through the current UVs into a new versioned 256×256 PNG and
assigns it as the primitive's base-color map.

## Assemble for runtime

**ASSEMBLE RUNTIME PREVIEW** loads every GLB referenced by the current spec and
spawns extracted components beside the regular preview using the production
runtime assembler.

Gameplay code calls
`request_imported_character(commands, character_root, spec)`. The root receives
an `ImportedAssemblyStatus`; completed children expose `ImportedRuntimePart`
and `ImportedGameplayRegion`. A canonical `SkeletonRig` present now or added
later binds rigid pieces to their assigned joints.

## Current boundary

Runtime assembly does not rewrite skin weights. Morph-only regions retain
source targets when their vertex stream stays one-to-one and expose named
runtime weights. Joint-bound accessories, armor, weapons, hands, feet, and
rigid limb regions are supported; continuously deforming torso/cloth regions
still need deform-aware rebinding. Paint has both a vertex-color preview and a
PNG bake; freehand pressure brushes and direct per-pixel editing remain future
work. Manual seam unwrap packs islands automatically but does not yet provide
per-island grab/rotate/scale controls.
