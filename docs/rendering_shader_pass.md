# Rendering and Shader Production Pass

## Review outcome

The July 2026 parallel shader review has good art-direction priorities, but its
sample WGSL is not directly compatible with Starfall's Bevy `0.19` renderer.
Production shaders must use `#{MATERIAL_BIND_GROUP}`, Bevy's current
`VertexOutput`, `globals`, `view`, and `lights` imports, and the actual mesh
vertex/instance helpers. Hardcoded `@group(1)` bindings, hand-written legacy
mesh matrices, manual time uniforms, and assumed screen sizes are rejected.

The cost numbers in the parallel review are hypotheses, not measurements. A
shader is not considered cheap until it is profiled in one-, two-, and
four-viewport scenes on target hardware.

## R1 delivered

- `toon.wgsl` uses all active Bevy directional lights plus ambient tint,
  branchless `step`-based bands, and a light-side mask for the view-angle rim.
  Its fragment-only contract preserves Bevy's default skinning, morph targets,
  instancing, visibility ranges, and mesh vertex output.
- `grass.wgsl` keeps wind deformation entirely on the GPU, anchors bend weight
  through Bevy Rectangle UVs instead of a fixed mesh height, supports material
  wind controls and root/tip gradients, adjusts normals with the bend, lights
  both sides, and follows the live directional-light buffer.
- `water.wgsl` supplies two low-frequency vertex waves, analytic top-surface
  normals, directional/ambient scene tint, Fresnel color/opacity, and cheap
  branchless crest highlights without normal-map texture fetches. Chapter 1's
  ocean and the world river now use it.
- `energy.wgsl` provides texture-free flow, pulse, view-angle edge color, and
  additive output for future beams, weapon cores, and magic projectiles.
- `shield.wgsl` provides tri-planar hex lines, pulse, Fresnel edges, and an
  impact parameter for future armor barriers.
- Forge spawns Toon, Water, Energy, and Shield preview geometry whenever the
  editor opens. This forces every registered pipeline to compile during the
  editor smoke route instead of allowing unused shader assets to decay.
- Static contract tests reject fixed material group 1 bindings, require scene
  lights for Toon/Grass/Water, preserve ordered toon bands, and bound default
  transparency controls.

## Deliberate limits

- R1 consumes directional and ambient lighting. Clustered point/spot-light
  evaluation, shadow sampling, and light cookies remain R2 work; duplicating
  Bevy's complete PBR lighting loop in every custom shader would increase
  maintenance and four-viewport fragment cost.
- Energy and Shield now have gameplay integrations, but GPU/entity counters and
  four-camera profiling are still required before raising effect density.
- Water is intentionally procedural and texture-free. Depth-buffer shoreline
  foam, refraction, screen-space reflection, and underwater rendering require a
  measured render-graph pass.
- Full-screen Sobel outlines are deferred. Four split-screen cameras multiply
  post-process cost and require per-view texture dimensions rather than the
  hardcoded 1920×1080 texel size suggested by the review.

## R2 delivered

1. Charge shots, Homing Star missiles/trails, magic tracking beams, Tri-Star
   bursts, Sprite shots, Sabre blades, and Sabre waves use a five-handle shared
   Energy palette. Fire rate cannot grow the material asset count, and elongated
   trails retain their authored shape while their bounded entities fade.
2. Primary and Sabre-wave projectiles now preserve their firing player entity.
   `PlayerParryEvent` carries `PlayerIndex`, matching damage feedback ownership.
3. Four persistent Shield material instances and meshes follow player slots
   P1-P4. Damage and successful parry events pulse only the owning player's
   shell; feedback changes uniforms rather than allocating per hit.
4. `ice.wgsl` blends directional-lit blue ice and slope/noise frost with
   branchless sparkles/Fresnel. `lava.wgsl` combines scene-lit cooled crust with
   texture-free animated emissive seams. Both are typed materials registered in
   Bevy and instantiated by Forge previews.
5. Forge material resources are grouped in one typed system parameter so adding
   material families does not exceed Bevy's direct-system parameter arity.

## R3 delivered

1. Forge resolves legacy-compatible Material field maps into typed Toon, Water,
   Energy, Shield, Ice/Snow, and Lava authoring specifications. Each family has
   three palette/tuning presets and four to six named scalar controls.
2. Controller-focusable Material Family, Material Preset, Next Parameter,
   Value −/+, and Reset actions update the selected record and its matching live
   shader preview. Bounds are displayed beside the active value.
3. Publish validation rejects unsupported shader IDs, invalid preset indices,
   nonnumeric fields, and values outside the runtime-safe ranges. Toon shadow/
   light thresholds and band levels remain ordered after legacy or extreme input.
4. The F11 performance overlay now reports active cameras, live projectiles,
   transient combat VFX, and custom/Standard material asset totals alongside
   FPS, frame time, total entities, and fixed ticks.

The controls are discrete controller-friendly steppers rather than mouse-only
sliders. A future slider widget may share the same typed bounds without changing
the saved payload contract.

## R4 delivered

1. Publishing builds a `PublishedMaterialCatalog` keyed by stable Forge content
   IDs. Catalog entries own typed Bevy handles for Toon, Water, Energy, Shield,
   Ice/Snow, or Lava; handles never enter project files or gameplay saves.
2. Scene object drafts have a backward-compatible optional `material_id`.
   Validation rejects missing/non-Material references, rename migrates bindings,
   and delete is blocked while any object depends on the material.
3. Controller-focusable `APPLY TO OBJECT` and `CLEAR OBJECT MATERIAL` actions
   swap the correct typed material component while retaining the object's mesh.
   The outliner displays each bound content ID and the registry reports the live
   runtime-catalog count.
4. Save/publish records unique scene-to-material dependencies. Project reload
   rebuilds published handles, restores bindings by ID, and safely leaves an
   unpublished binding on its Standard preview rather than inventing a handle.

Authoring flow: create/tune a Material, publish once to place it in the runtime
catalog, select a renderable scene object and apply the Material, then publish
again to persist the scene binding and dependency.

## Rendering work remaining

1. Evaluate clustered point/spot lights through Bevy's current cluster lookup;
   compare that path against a cheaper dominant-light CPU uniform on four-way
   split screen.
2. Add per-view GPU timing labels and establish measured one/two/four-camera
   budgets before enabling any full-screen outline, refraction, or
   heat-distortion pass. CPU-visible asset/entity counters are now delivered.
3. Bind biome/road/building recipes to the delivered runtime catalog IDs and
   add controlled regeneration transactions. Scene primitives are delivered.
4. Apply Ice/Lava to authored biome zones only after collision-safe placement
   and one/two/four-camera measurements; Forge previews are the current rollout.

## Shader acceptance gates

- No fixed material bind-group numbers or legacy Bevy matrix interfaces.
- No per-pixel branch for simple band selection when `step`, `mix`, or
  `smoothstep` expresses the same result.
- Any remaining branch must be uniform across the draw or justified by measured
  performance/readability.
- World-lit surfaces follow live scene lighting or clearly declare themselves
  unlit/emissive.
- Transparent materials define their blend mode and bounded alpha defaults.
- Every shader is instantiated by an automated or live smoke scene.
- Four-player performance is measured before broad rollout.
