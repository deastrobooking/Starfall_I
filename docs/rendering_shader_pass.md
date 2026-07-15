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

## R2 remaining

1. Evaluate clustered point/spot lights through Bevy's current cluster lookup;
   compare that path against a cheaper dominant-light CPU uniform on four-way
   split screen.
2. Add per-view GPU timing labels and material/entity counters. Establish
   measured one/two/four-camera budgets before enabling any full-screen outline,
   refraction, or heat-distortion pass.
3. Expose typed Material payload fields in Forge with live sliders, palette
   presets, validation bounds, preview meshes, and publish-to-runtime catalogs.
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
