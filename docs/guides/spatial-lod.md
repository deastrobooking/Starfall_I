# Spatial LOD And Distance Culling

`N11a` establishes the reusable large-world rendering service. It wraps
Bevy's camera-aware `VisibilityRange` behind Starfall-owned authoring profiles,
propagates one profile across a render hierarchy, and exposes live coverage in
the F11 performance overlay. `N11b` now provides three production tiers:
detailed procedural trees crossfade into shared trunk/canopy proxies, nearby
building shells crossfade into material-compatible skyline clusters, and the
world terrain swaps between spatial high/low patches.

This slice is render-only. A culled mesh stops being submitted for rendering,
but its entity, collider, gameplay state, and simulation remain loaded. That
separation makes the first rollout safe for traversal and split-screen play.

## Authoring a profile

Add `SpatialLod` to either a mesh entity or the root of a hierarchy:

```rust
commands.spawn((
    SceneRoot(scene),
    SpatialLod::new(1_800.0, 2_100.0),
));
```

All current and newly-added descendant `Mesh3d` entities inherit the nearest
ancestor profile. Removing the profile also removes the ranges managed by that
root. Do not manually add a second `VisibilityRange` to a managed mesh.

Built-in presets:

- `SpatialLod::foliage_high()` renders detailed tree meshes through the
  850–1,050 crossfade.
- `SpatialLod::foliage_proxy()` fades the lightweight replacement in from
  850–1,050, then out from 1,350–1,550.
- `SpatialLod::landmark(height)` gives taller city shells a farther skyline
  range, capped at 6,000 world units.
- `SpatialLod::building_high(height)` and `building_proxy(height)` use matching
  transitions for the 800-unit skyline clusters generated after world spawn.
- `SpatialLod::terrain_high()` renders full 62.5-unit terrain samples nearby;
  `terrain_proxy()` uses 250-unit samples from 2,600–16,000 units.

Use `SpatialLod::tier(min, fade_in_end, fade_out_start, max)` for additional
multi-mesh groups. The fade-out margin of one tier must equal the fade-in margin
of the next tier. A profile placed directly on a descendant overrides an
inherited root profile, which allows imported scene hierarchies to mix tiers.

The visibility test considers every active camera. In local split-screen, a
mesh remains rendered while any player camera can see it within its range.

LOD tiers intentionally use the entity origin rather than each mesh AABB.
Adjacent high/proxy meshes share an authored origin, while their decimated AABB
centers can differ; origin distance therefore preserves stable crossfades. The
current 2,500-unit terrain patches and 800-unit building clusters remain well
inside their transition distances, so a player standing at a patch edge does
not lose the nearby tier.

## Verify changes

1. Start a gameplay session and press F11.
2. Confirm `lod` reports non-zero profile, mesh, and proxy counts after the
   world loads. The following lines report visible/total terrain high patches,
   terrain proxies, and building-cluster proxies. Visibility means visible to
   at least one active render view.
3. Travel away from dense foliage or a city and verify distant meshes fade
   without colliders or walkable surfaces disappearing.
4. Repeat with multiple local players and approach the same asset from different
   cameras.
5. Fast-travel to outer settlements and caves. Confirm terrain seams stay closed,
   roads retain their grades, and F9 colliders do not change as tiers swap.

Automated coverage lives beside the service in `src/spatial_lod.rs` and checks
profile bounds, hierarchy propagation, and clean removal.

## Next slices

The 320×320 terrain grid is rendered as 64 patches. Each patch has a 41×41 near
mesh and an 11×11 far proxy, while the original full-resolution trimesh remains
the sole physics surface. Building clusters likewise keep every original
collider live and only replace rendering. Transparent/glass materials are
excluded from merged skyline proxies to preserve correct sorting and blending.

`N11c` adds biome/chunk streaming with explicit simulation ownership; only that
later slice may unload colliders and gameplay entities, with safety margins
around every active player. Before enabling unload, record four-player frame
and physics timings at the world center and outer fast-travel anchors.
