# Starfall rendering and creation program

This is the feature and acceptance map for the September 2026 rendering
proposal. It extends the existing engine and Forge. `PROJECT_PLAN.md` owns
cross-project priority; this document owns the dynamic-GI and virtual-geometry
work breakdown. Implemented prototypes are identified separately from targets.

The product target is scalable native rendering for one to four local players,
with reproducible performance evidence, usable authoring tools, and a reliable
standard-renderer fallback. Lumen and Nanite are reference technologies; the
Starfall implementations must be described by the algorithms they actually run.

## Existing features and remaining work

| Area | Existing implementation | Work in this program |
|---|---|---|
| Framework | Public facade; graph, manifest, platformer and compiler crates; minimal dependency guard | Selectable rendering experiments; neutral cooked rendering contracts |
| Native runtime | Bevy 0.19.1, Avian 0.7, fixed motor, buffered input, shared/split cameras | Preserve gameplay/physics ownership while replacing render representations |
| Geometry scale | Hierarchical `SpatialLod`, foliage/building proxies, terrain patches, visibility crossfades | Measured meshlet backend, automatic clustered LOD, occlusion, bounded residency |
| Materials | Standard PBR plus Toon, Grass, Water, Energy, Shield, Ice and Lava families | Explicit GI/material eligibility and indirect-diffuse integration |
| Lighting | Native lights, shadows, contact-shadow preferences and material emission | Dynamic diffuse probe volumes, visibility, history, invalidation and debug views |
| Diagnostics | F11 frame/entity/simulation/LOD counters, Tracy | Reproducible view layouts, device reports, percentiles, per-view/pass observations |
| Forge shell | Project Hub, outliner, registry, viewport, picking, fly/orbit cameras | Extend current native Feathers/controller UI; rendering panels and overlays |
| Transform authoring | Transactional translate/rotate/scale, snapping and stable selection | Preserve command ownership for render-volume and asset settings |
| Documents | Versioned project/source records, undo/redo, atomic saves and recovery | Defaulted rendering settings and migrations; no persistent ECS entities |
| Content loop | Typed records, validation, immutable published generations, runtime catalogs | Background meshlet/voxel cooking, dependency hashes, budgets, cancellation |
| Game export | Standalone native folder, executable/assets/pinned generation | Project-owned roots, dependency cooking, rendering variants and compatibility reports |
| Graphs | Typed kernel, platformer compiler and dialogue records | Material/VFX domain contracts, visual editing, native node extension registration |
| VFX work in progress | `starfall-vfx-graph` documents/compiler and a CPU runtime are being added separately in this workspace | Preserve that work; evaluate visual graph editing, published catalogs and GPU stages against its contracts |
| Preview modes | Existing tools and disposable preview entities | Authoritative Edit/Simulate/PIE lifecycle from Forge M4 |
| Gameplay | Input mapping, AI/combat, data assets, co-op progression and save/load | Focused remaining acceptance work; retain existing systems and schemas |

The suggested egui shell, input-manager replacement, and new save system are
not prerequisites: these responsibilities already have production owners.
Feathers, Starfall's controller action registry, and its document/command
boundary remain the integration points. BSN is an evaluation target for derived
scene instantiation; it does not replace authored project identity or recovery.

## Corrections to the supplied prototype

- Bevy 0.19 camera rendering uses render-world schedules such as `Core3d`,
  `Core3dSystems`, `RenderSystems`, and `RenderGraph`. The old node/`RenderSet`
  skeleton is not a drop-in integration for this pinned release.
- Independent selection of every acceptable cluster draws both ancestors and
  descendants. Selection needs a complete, non-overlapping hierarchy cut,
  parent/group error, conservative bounds and explicit overflow behavior.
- Serialized triangle offsets are checked offsets into owned buffers. Pointer
  addresses, truncated pointers, unchecked ranges and assumed contiguous child
  IDs cannot cross the asset boundary.
- Mesh simplification must preserve group boundaries, UV/material seams and
  monotonic error. It needs termination when a group cannot simplify. A list
  of independently simplified clusters does not establish these properties.
- The proposed voxel shader uses fixed steps, not DDA, and leaves core
  functions undefined. Its dispatch needs bounds checks and valid one-ray
  behavior; volume exit must not sample clamped edge texels indefinitely.
- Hit albedo is not incident radiance. Ray results alone are not GI: lighting
  injection, irradiance integration, distance moments, visibility weighting,
  temporal updates and surface sampling must all be implemented and tested.
- GPU capability is checked on the selected device. macOS, Vulkan, and a GPU
  family name are not sufficient feature tests. Missing timestamps remain
  unavailable in reports rather than being represented as zero GPU cost.

## Architecture and backend decisions

Keep simulation geometry/colliders independent from render LOD. Cooked assets
contain stable IDs, schema/processor versions, content hashes and bounded
buffer ranges. Forge owns authoring commands and background cooking; runtime
owns resource residency, view selection and shading. Reuse published-generation
selection rather than adding a second source of active-content truth.

Evaluate Bevy's pinned experimental meshlet renderer before maintaining another
full cluster renderer. It already includes preprocessing, hierarchy traversal,
culling and rasterization. Its device-feature, material and MSAA restrictions
are part of the eligibility contract. Feature availability alone is not a
correctness or performance pass. Unsupported assets/devices retain Mesh3d/PBR
and the existing spatial LOD path.

The portable GI track uses a bounded voxel scene and irradiance probes. Bevy
Solari is a separate experimental hardware-ray-query comparison path; it is not
automatically interchangeable with voxel DDGI or required for the Metal path.

World-space probe updates are shared across cameras. Surface sampling, geometry
visibility, screen-space history and occlusion are view-specific. Budget one
shared cache plus the actual number and pixel extent of views, not four copies
of all world resources. Allocate private state for each camera and invalidate
it on camera cuts, resize, scene change and split/shared transitions.

## Delivery sequence and complete feature map

| ID | Feature set and ownership | Acceptance gate | State |
|---|---|---|---|
| RL0 | Standalone rendering lab: geometry and lighting fixtures, frame-indexed camera/occluder motion, 1/2/4 views, fixed resolution, warmup, report schema, device features and timing | Runs without demo/Forge; exact view coverage; finite statistics; new report per run; no false GPU measurements | Implemented and exercised natively; report completion does not certify visual parity |
| RL1 | Baseline campaign scenes: city, four-player combat, platformer, race road, interior and Forge; production quality tiers | Release captures on named hardware; CPU/GPU costs labeled separately; frame pacing and visible work recorded | Planned |
| GI0 | Probe/voxel configuration, checked resource arithmetic, CPU reference traversal and deterministic GPU validation fixture | Overflow/zero/NaN rejection; ray bounds; empty/solid/outside/boundary/axis-aligned cases; CPU/GPU agreement | Passed on M3 Pro/Metal: 520 rays, zero mismatches, maximum absolute error 0.000000477 |
| VG0 | Pinned Bevy meshlet eligibility and asset audit; ordinary-mesh comparison | Actual adapter feature report; materials/MSAA restrictions visible; preserve fallback | Implemented; audit found multi-camera shadow/quality blockers |
| VG1 | Background offline preprocessing; cluster bounds, hierarchy, geometric error, locked seams, versioned baked asset, source/processor hashes | Invalid meshes rejected; bounded deterministic ranges; cancellation and failed bakes preserve last-good asset | Planned |
| VG2 | Opt-in resident meshlet scene, per-view selection, debug LOD/cluster color, shadow/material parity | Compare same source scene against stock renderer for 1/2/4 views; no holes, double cuts, overflow or unsupported-feature exit | Resident fixture implemented; multi-camera shadow parity failed; no production promotion |
| VG3 | Frustum/cone/HZB occlusion validation, reverse-Z handling, previous/current frame recovery | No disappearances on disocclusion, fast camera motion, camera cuts or split changes | Planned |
| VG4 | Streaming pages, pinned coarse fallback, asynchronous upload/eviction, residency and request budgets | Missing finer pages select a complete resident cut; bounded memory and no render-thread disk waits | Planned |
| GI1 | Conservative static/dynamic voxel occupancy, linear albedo/emission/direct-radiance injection, world-to-volume transforms and dirty-region updates | Moving blockers and lights update correct cells; thin-wall and volume-edge reference cases; fixed memory/work caps | Planned |
| GI2 | GPU ray dispatch, directional irradiance integration, octahedral borders and distance moments | Readback/reference agreement; guards for rounded dispatch; radiance and visibility units documented | Planned |
| GI3 | Temporal accumulation, hysteresis, reset/invalidation, probe relocation/classification and bounded scheduling | Stable static scene; bounded change response; no stale rooms after teleport or geometry removal | Planned |
| GI4 | Visibility-weighted surface sampling in eligible PBR and toon materials; artist indirect intensity | Lit/unlit/reference image comparisons; no double ambient term; no wall leaks in acceptance fixtures; unchanged disabled path | Planned |
| GI5 | Probe volumes/cascades, world streaming and multi-camera union prioritization | Bounded shared updates and memory across 1/2/4 cameras; defined behavior outside loaded volumes | Planned |
| GI6 | Optional screen-space detail, specular/reflection comparison, hardware tracing adapter | Separate capability/performance gates; diffuse-only mode remains supported | Planned extension |
| FT0 | Forge probe/voxel/LOD/occlusion/residency overlays and budget panels | Controller navigation, undo/redo and save/reopen parity; overlay caps; last-good preview | Planned |
| FT1 | Typed rendering records, imported mesh eligibility, material properties, thumbnail/drag-drop integration | Stable IDs, dependency validation, migration tests and safe defaults; no GPU handles in project sources | Planned |
| FT2 | Dependency-aware cooking, immutable published rendering catalogs and export variants | Export runs without Forge/source tree; reject stale/unsupported baked formats; graceful per-asset fallback | Planned |
| FT3 | Edit/Simulate/PIE isolation and disposable renderer previews | Stop restores authoring state; no persistent mutations from simulation; resources released | Follow Forge M4 |
| NG0 | Reusable native graph UI over existing typed registry/commands | Selection, connect/disconnect, validation, undo and controller access share one document model | Planned |
| NG1 | Material graph domain and shader compilation | Typed ports, cycle/domain rules, compile diagnostics, cache and last-valid shader | Planned |
| NG2 | VFX graph domain: spawn/update/render stages, particle buffers and capacity budgets | Bounded CPU/GPU work; published deterministic program; per-player effects ownership | CPU compiler/runtime work underway separately; GPU/publishing/UI remain |
| NG3 | Behavior/event graph runtime and native extension hooks | Typed events, bounded execution, save-compatible stable state; no replacement of working AI by default | Planned |
| QA0 | Compatibility matrix, image fixtures, budget regressions, memory/resource lifetime and four-pad play | All feature combinations build; release baselines and human hardware acceptance recorded | Continuous |

A custom virtual-geometry renderer is a deliberate fallback research branch if
Bevy's meshlet implementation cannot satisfy the supported hardware/material
matrix. That branch needs offline grouping/simplification, an executable CPU
cut reference, bounded GPU traversal/indirect buffers, shadow integration and
an exact coverage oracle before it can substitute for VG2. Hardware-only
rasterization, software microtriangle rasterization and disk streaming are
distinct capabilities and must never be implied by the word “meshlets.”

## Quality and promotion gates

- **Correctness:** reference images and numerical tests precede performance
  claims. Zero panics is necessary but does not demonstrate lighting or LOD
  quality. CPU-visible entity counts do not represent GPU triangle/draw counts.
- **Performance:** the product goal remains 60 Hz / 16.67 ms. Initial experiment
  budgets are proposals until release measurements exist. Record mean, p50,
  p95, p99 and worst frame, warmup, view pixels, topology and hardware. Do not
  sum nested asynchronous pass spans as a total GPU frame.
- **Memory/work:** calculate every allocation before creation, including
  double-buffered history, atlases/borders, depth moments, staging and per-view
  state. Enforce update/ray/cluster/request budgets and retain a complete fallback.
- **Compatibility:** test Metal/Vulkan/DX12 only where the selected backend is
  supported; standard rendering remains the universal application path. Keep
  minimal and Game/Designer builds green and the engine version pinned.
- **Authoring:** edits are transactional and validated; cooking is cancelable;
  a failure preserves authored sources and the last valid published generation.
- **Rollout:** opt-in lab → Forge preview → focused published Game scene →
  campaign adoption after measured quality/performance and controller acceptance.

## Run the first milestone

```sh
cargo test --locked --no-default-features --features render-lab
cargo run --release --locked --no-default-features --features render-lab \
  --example render_lab -- --scene geometry --views 1 \
  --output target/render-lab/geometry-1.json
# Repeat with --views 2 and --views 4 and distinct output names.
# Repeat the view matrix with --scene lighting.
# Add --validate-probes to run the one-shot voxel GPU/CPU comparison before warmup.
```

The lab defaults to stock PBR. Its opt-in meshlet mode uses Bevy's experimental
resident renderer; dynamic GI and streamed geometry remain future work.
Geometry uses a repeatable high-resolution sphere grid; lighting
uses colored walls, a point light and a moving blocker for future GI comparison.
Reports distinguish enabled device capabilities from installed render features.
The pinned Bevy implementation disables encoder timestamp writes on macOS;
the lab omits those GPU diagnostics even when their names appear in the store.
AutoNoVsync is a requested presentation mode; desktop compositing and refresh
scheduling can still affect wall-clock intervals. These are not pure GPU times.
Use release builds for performance evidence; debug results are correctness
smokes. The first fixtures are synthetic, not campaign performance guarantees.

The optional GI0 check traces 520 rays through a fixed 16³ radiance/occupancy
volume in nine 64-thread workgroups. It includes outside and axis-aligned rays,
checks every output against the CPU DDA reference, and requires agreement within
0.002 world units per distance component. The RGB comparison uses the same
absolute tolerance. Validation happens before warmup; one dispatch and its
readback are excluded from the timing interval. This fixture is independent of
the visible scene and does not yet apply indirect illumination to its materials.

## Resident meshlet comparison

```sh
cargo build --release --locked --no-default-features \
  --features render-lab-meshlets --example render_lab
target/release/examples/render_lab --renderer pbr --views 4 \
  --output target/render-lab/pbr-4.json --capture target/render-lab/pbr-4.png
target/release/examples/render_lab --renderer meshlets --views 4 \
  --output target/render-lab/meshlets-4.json --capture target/render-lab/meshlets-4.png
# Repeat at 1 and 2 views, using new filenames for each report and capture.
```

Use the same feature-enabled executable for both sides of the comparison.
The meshlet feature adds the processor dependencies required by pinned Bevy;
it is absent from the default game and minimal framework. The adapter checks
the actual enabled device features and Metal/Vulkan backend before registering
`MeshletPlugin`. A missing build feature, unsupported device, lighting-scene
request, or failed fixture cook retains PBR and records a reason. Inspect
`geometry_backend.active` and `scene_inventory.meshlet_instances` before
treating a run as meshlet evidence.

Both paths share the exact indexed sphere mesh, opaque StandardMaterial,
transforms, lights, camera path, resolution and MSAA-off setting. The UV
sphere's duplicate seam/pole positions and normals are canonicalized before
either path receives it; an exact-position edge-incidence test requires a closed,
non-degenerate surface. UV coordinates and source triangle indices are retained.
The PBR floor remains in both paths. Every 3D camera renders into a texture matching
its tile size; a final 2D camera composes those images into the fixed window.
This avoids a native Bevy 0.19.1 meshlet attachment-size validation failure with
sub-viewports of a shared window. PBR uses the same composition path, including
its cost. Reports identify it as `per_view_texture_then_ui_composite`; earlier
direct-window baselines are not an equivalent comparison. One sphere is cooked during Startup and shared
by every instance; cook time and asset format version are recorded separately.
This bounded procedural bake is a lab fixture, not the background asset pipeline
specified by VG1. The plugin preallocates 2²¹ cluster slots (8 MiB for that
buffer alone); reports do not claim this is total GPU memory or used clusters.
`visible_mesh_entities` counts only ordinary Mesh3d CPU candidates and excludes
GPU-culled meshlet instances. Cluster counts and selected triangle counts remain
unavailable. Capture holds the last measured camera pose and writes a new PNG
after timing ends; a capture failure exits unsuccessfully while retaining the
completed timing report. Capture verifies physical dimensions and non-uniform
content in every tile. A blank capture is saved for diagnosis but fails the run;
this sanity check cannot establish shadow, material or LOD parity.

`--shadows off` separates surface/LOD artifacts from shadow errors.
`--meshlet-precision 1..16` controls position quantization at 1/2^n centimeters
(default 4, matching pinned Bevy). Record both settings in comparisons. Neither
knob changes source geometry or silently substitutes a different renderer.

After building, automate the 1/2/4-view geometry/PBR, geometry/meshlet and
lighting/PBR matrix with one executable and no overlapping runs:

```sh
python3 scripts/benchmark_render_lab.py --output target/render-lab/new-matrix
```

The runner requires a new directory, captures every case, validates GPU probe
traversal before warmup, and rejects a fallback/debug build or a change of
source/device/composition between runs. Each case has a configurable 600-second
timeout. `summary.json` records frame mean/p95;
individual reports retain all percentiles and settings. A completed matrix
establishes measurement comparability, not visual acceptance or production
performance. Use `--shadows off` to run the same matrix without shadow maps.

The September 5 native pass fixed mismatched attachments through per-view
targets and removed visible sphere seam cracks by canonicalizing duplicate
positions. It also exposed incorrect two-/four-camera meshlet shadows. A full
four-camera shadowed run produced a blank capture and severe frame stalls;
its successful process exit was not a rendering pass and motivated the blank
capture guard. Keep this backend opt-in. The immediate geometry work is to
isolate/fix per-view shadow ownership in the pinned backend before adding
production assets or claiming performance gains. GI1 voxel updates/radiance
injection remains the next independent lighting slice.

The initial eligibility scope excludes masked/transparent/transmissive assets,
custom vertex attributes and custom vertex shaders. Existing Starfall material
families need separate meshlet-fragment compatibility work. Debug cluster/LOD
color, thin geometry, large scenes, disocclusion, camera cuts, split transitions,
memory pressure and published-asset fallback remain promotion gates.

## Source basis

- [DDGI: Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields](https://jcgt.org/published/0008/02/01/)
  establishes the irradiance-field and visibility reference.
- [Epic's Nanite deep dive, SIGGRAPH 2021](https://advances.realtimerendering.com/s2021/index.html)
  covers the complete geometry pipeline beyond meshlet generation.
- [Bevy 0.19.1 meshlet implementation](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_pbr/src/meshlet/mod.rs)
  owns the experimental backend requirements and restrictions; local pinned
  source was checked before choosing the integration path.
- [Bevy 0.19.1 Solari contract](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_solari/src/lib.rs)
  defines the separate ray-query backend's required features.
- [Meshoptimizer upstream](https://github.com/zeux/meshoptimizer)
  documents meshlet/simplification APIs and their limits; processor versions
  must follow the pinned Bevy dependency graph when using its cooker.
