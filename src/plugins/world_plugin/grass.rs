//! Chunked grass field.
//!
//! The naive approach — one entity per blade — costs the CPU an extraction and
//! a transform per blade every frame, which caps out in the low tens of
//! thousands. This module moves the whole cost to the GPU instead:
//!
//! * Blades are **baked into merged chunk meshes**. Every blade in a 16 m chunk
//!   lives in one `Mesh`, so a chunk is one entity and one draw call no matter
//!   how many blades it holds. Per-blade variation that would normally need an
//!   instance buffer (sway phase, tint, the terrain colour underneath) is baked
//!   into the vertex stream and read by `assets/shaders/grass.wgsl`.
//! * Chunks **stream around every active player camera**. Only the chunks that
//!   can be seen exist as entities; their meshes are cached by coordinate so
//!   walking back over ground you have already crossed costs nothing.
//! * Chunks come in **three LOD tiers** with overlapping visibility windows. The
//!   shader dissolves each tier in and out by camera distance, so the density
//!   drop-off happens as a dither cross-fade rather than a visible ring.
//! * Bevy's own frustum culling does the rest: each chunk carries an explicit
//!   `Aabb`, so chunks behind the camera are skipped before they reach the GPU.
//!
//! Placement is a pure function of `(world seed, chunk coordinate)`, so a chunk
//! rebuilt after eviction is identical to the one that was thrown away.

use super::*;
use crate::components::player::PlayerCamera;
use bevy::camera::primitives::Aabb;
use bevy::light::NotShadowCaster;
use std::collections::HashMap;

// ── Field layout ─────────────────────────────────────────────────────────────

/// Chunk edge length in world units. Small enough that frustum culling is
/// meaningful, large enough that the per-chunk terrain queries amortise.
const GRASS_CHUNK_SIZE: f32 = 16.0;

/// Terrain heights only change every `TERRAIN_MESH_CELL_SIZE` (62.5 m), so a
/// chunk sits inside a single terrain triangle pair and its slope is constant.
/// That lets the expensive terrain queries run once per chunk instead of once
/// per blade.
const GRASS_SLOPE_PROBE: f32 = GRASS_CHUNK_SIZE * 0.5;

/// Resolution of the per-chunk ground-colour grid that blades sample. Terrain
/// colour varies far more slowly than blade spacing, so a coarse grid plus
/// bilinear interpolation is indistinguishable from evaluating it per blade —
/// and avoids running the route/erosion noise thousands of times per chunk.
const GRASS_GROUND_SAMPLES: usize = 4;

/// Meshes are built on the main thread, so cap how many a single frame may bake.
const GRASS_BUILD_BUDGET: usize = 2;
/// While the field is still filling in — world load, or a teleport — bake at
/// this rate instead. Loading already stalls, so the cost lands where it hides.
const GRASS_WARMUP_BUDGET: usize = 16;
const GRASS_WARMUP_CHUNKS: usize = 48;

/// Cached meshes are dropped once the player is this far past a tier's spawn
/// radius. The slack means pacing back and forth across a boundary re-uses the
/// cache instead of rebaking.
const GRASS_CACHE_SLACK: f32 = 64.0;

/// A `u16` index buffer halves index memory; this is the vertex ceiling it
/// implies. No tier comes close, but the baker refuses to overrun it.
const GRASS_MAX_CHUNK_VERTICES: usize = 60_000;

// ── Terrain acceptance ───────────────────────────────────────────────────────

/// Grass fades in at the world floor and out into the alpine zone. The upper
/// band is matched to `terrain_vertex_color`: rock takes over around 360-650
/// and snow above 470, so grass thins out across the same range instead of
/// growing through bare stone.
const GRASS_LOW_ALTITUDE: (f32, f32) = (-1.0, 0.25);
const GRASS_HIGH_ALTITUDE: (f32, f32) = (380.0, 640.0);
/// Blades cannot hold onto a cliff, but they do cover rolling hillsides — this
/// is a gradient magnitude, not an angle, so the upper bound is a steep face.
const GRASS_SLOPE_FADE: (f32, f32) = (0.30, 0.70);
/// The city core is paved; grass resumes on the outskirts.
const GRASS_CITY_CLEARANCE: (f32, f32) = (95.0, 138.0);
/// Beyond the coastline the terrain is under the perimeter ocean.
const GRASS_COAST_LIMIT: f32 = 8_600.0;

// ── LOD tiers ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GrassTier {
    Near,
    Mid,
    Far,
}

impl GrassTier {
    const ALL: [GrassTier; 3] = [GrassTier::Near, GrassTier::Mid, GrassTier::Far];

    fn profile(self) -> TierProfile {
        match self {
            // Full detail underfoot: curved four-segment blades at close spacing.
            GrassTier::Near => TierProfile {
                density: 9.0,
                segments: 4,
                width_scale: 1.0,
                height_scale: 1.0,
                spawn_within: 34.0,
                spawn_beyond: 0.0,
                range: Vec4::new(-2.0, -1.0, 20.0, 30.0),
            },
            // Middle distance trades blade count for blade width: fewer, wider
            // blades hold the same apparent coverage for a fraction of the mesh.
            GrassTier::Mid => TierProfile {
                density: 2.2,
                segments: 3,
                width_scale: 1.7,
                height_scale: 1.05,
                spawn_within: 72.0,
                spawn_beyond: 14.0,
                range: Vec4::new(18.0, 28.0, 56.0, 68.0),
            },
            // At the horizon only the silhouette survives, so blades are wide,
            // slightly taller, and cheap.
            GrassTier::Far => TierProfile {
                density: 0.85,
                segments: 2,
                width_scale: 3.0,
                height_scale: 1.15,
                spawn_within: 118.0,
                spawn_beyond: 50.0,
                range: Vec4::new(56.0, 68.0, 96.0, 112.0),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TierProfile {
    /// Blade candidates per square metre before terrain masking.
    density: f32,
    /// Cross-sections above the root; the blade also gets a single tip vertex.
    segments: usize,
    width_scale: f32,
    height_scale: f32,
    /// A chunk is live while its nearest point is within this distance …
    spawn_within: f32,
    /// … and its farthest point is beyond this one. Both margins sit outside the
    /// tier's `range` window, so a chunk is never spawned or despawned while any
    /// of its blades are still visible.
    spawn_beyond: f32,
    /// Shader visibility window: X/Y fade in, Z/W fade out.
    range: Vec4,
}

// ── Material ─────────────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct GrassMaterial {
    #[uniform(0)]
    settings: GrassMaterialUniform,
}

#[derive(ShaderType, Clone, Copy)]
struct GrassMaterialUniform {
    /// RGB tip tint, A blade alpha.
    tip_color: Vec4,
    /// RGB root tint multiplied into the baked ground colour, A root occlusion.
    root_color: Vec4,
    /// X wave speed, Y bend strength, Z spatial frequency, W flutter scale.
    wind: Vec4,
    /// XY wind direction on the XZ plane, Z gust frequency, W gust depth.
    flow: Vec4,
    /// X translucency, Y sheen, Z colour variation, W ambient scale.
    detail: Vec4,
    /// X/Y fade-in distances, Z/W fade-out distances.
    range: Vec4,
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None; // render both faces of each blade
        Ok(())
    }
}

fn tier_material(range: Vec4) -> GrassMaterial {
    GrassMaterial {
        settings: GrassMaterialUniform {
            tip_color: Vec4::new(0.30, 0.52, 0.16, 1.0),
            root_color: Vec4::new(0.66, 0.74, 0.54, 0.55),
            wind: Vec4::new(1.15, 0.26, 0.055, 0.30),
            flow: Vec4::new(0.82, 0.57, 0.011, 0.55),
            detail: Vec4::new(0.55, 0.30, 0.22, 1.0),
            range,
        },
    }
}

// ── Field resource ───────────────────────────────────────────────────────────

/// Marks a live grass chunk. The streamer tracks coordinates and tiers in its
/// own map; this is here so diagnostics and world tooling can find the field.
#[derive(Component, Debug, Clone, Copy)]
struct GrassChunk;

/// Runtime state of the streamed field.
#[derive(Resource, Default)]
pub(super) struct GrassField {
    seed: u64,
    /// One material per tier; they differ only in their visibility window.
    materials: Vec<(GrassTier, Handle<GrassMaterial>)>,
    /// Baked meshes by chunk and tier. `None` records "this chunk holds no
    /// grass", which is just as valuable to cache as a mesh.
    baked: HashMap<(IVec2, GrassTier), Option<Handle<Mesh>>>,
    live: HashMap<(IVec2, GrassTier), Entity>,
    active: bool,
    /// Cleared on arm; set once the field has finished its first fill so the
    /// summary below is logged a single time per world rather than per frame.
    reported: bool,
}

impl GrassField {
    fn material(&self, tier: GrassTier) -> Option<&Handle<GrassMaterial>> {
        self.materials
            .iter()
            .find(|(candidate, _)| *candidate == tier)
            .map(|(_, handle)| handle)
    }
}

/// Called by world generation once the terrain seed is known. Existing chunks
/// were already despawned with the rest of `WorldGeometry`, so this only has to
/// drop the bookkeeping and re-arm the streamer for the new world.
pub(super) fn arm_grass_field(
    field: &mut GrassField,
    materials: &mut Assets<GrassMaterial>,
    seed: u64,
) {
    field.seed = seed;
    field.baked.clear();
    field.live.clear();
    field.reported = false;
    field.materials = GrassTier::ALL
        .iter()
        .map(|&tier| (tier, materials.add(tier_material(tier.profile().range))))
        .collect();
    field.active = true;
}

/// `cleanup_world` despawns every `WorldGeometry` when play ends, and grass
/// chunks carry that marker. Without this the live map would keep entities that
/// no longer exist: the streamer would consider those chunks covered and never
/// rebuild them, leaving bald ground on the next visit.
///
/// The pause guard mirrors `cleanup_world` exactly — a paused session keeps its
/// world entities, so the map must keep pointing at them.
fn retire_grass_field(mut field: ResMut<GrassField>, transition: Res<PlaySessionTransition>) {
    if transition.pausing {
        return;
    }
    field.live.clear();
}

fn disarm_grass_field(mut field: ResMut<GrassField>) {
    // `cleanup_world_for_menu` despawns the chunk entities along with every
    // other `WorldGeometry`; dropping the handles here releases their meshes.
    field.baked.clear();
    field.live.clear();
    field.active = false;
}

// ── Streaming ────────────────────────────────────────────────────────────────

pub(super) struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrassField>()
            .add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .add_systems(OnEnter(AppState::MainMenu), disarm_grass_field)
            .add_systems(OnExit(AppState::Playing), retire_grass_field)
            .add_systems(
                Update,
                stream_grass_chunks.run_if(in_state(AppState::Playing)),
            );
    }
}

fn stream_grass_chunks(
    mut commands: Commands,
    mut field: ResMut<GrassField>,
    mut meshes: ResMut<Assets<Mesh>>,
    cameras: Query<(&GlobalTransform, &Camera), With<PlayerCamera>>,
    players: Query<&GlobalTransform, (With<Player>, Without<PlayerCamera>)>,
) {
    if !field.active {
        return;
    }
    // Split-screen cameras can be kilometres apart, so every active view owns
    // a streaming ring. Player transforms are only the startup fallback for a
    // frame where no gameplay camera is active yet.
    let mut focuses: Vec<Vec2> = cameras
        .iter()
        .filter(|(_, camera)| camera.is_active)
        .map(|(transform, _)| transform.translation())
        .map(|position| Vec2::new(position.x, position.z))
        .collect();
    if focuses.is_empty() {
        focuses.extend(players.iter().map(|transform| {
            let position = transform.translation();
            Vec2::new(position.x, position.z)
        }));
    }
    if focuses.is_empty() {
        return;
    }
    let seed = field.seed;

    // ── Decide which chunks should exist ─────────────────────────────────
    let wanted = wanted_grass_chunks(&focuses);
    let mut missing: Vec<(IVec2, GrassTier, f32)> = wanted
        .iter()
        .filter(|(key, _)| !field.live.contains_key(key))
        .map(|(&(coord, tier), &nearest_focus)| (coord, tier, nearest_focus))
        .collect();

    // ── Retire chunks that left every tier's window ──────────────────────
    field.live.retain(|key, entity| {
        if wanted.contains_key(key) {
            return true;
        }
        // `try_despawn`, not `despawn`: world teardown can remove chunks out
        // from under the map, and that is not worth a warning per chunk.
        commands.entity(*entity).try_despawn();
        false
    });

    // ── Bake and spawn, nearest first, within this frame's budget ────────
    // Chunks already in the cache cost nothing to spawn, so only fresh bakes
    // draw against the budget: crossing back over old ground never hitches.
    missing.sort_by(|a, b| a.2.total_cmp(&b.2));
    // Loading into a world needs a few hundred chunks at once. Filling that in
    // two-at-a-time would take seconds of visible pop-in, so the field bakes
    // aggressively until it is populated and then drops to a trickle that keeps
    // ordinary movement hitch-free.
    let budget = if field.live.len() < GRASS_WARMUP_CHUNKS {
        GRASS_WARMUP_BUDGET
    } else {
        GRASS_BUILD_BUDGET
    };
    let mut baked_this_frame = 0usize;
    for (coord, tier, _) in missing {
        let key = (coord, tier);
        if let std::collections::hash_map::Entry::Vacant(entry) = field.baked.entry(key) {
            if baked_this_frame >= budget {
                break;
            }
            baked_this_frame += 1;
            let baked = bake_chunk_mesh(coord, tier, seed).map(|mesh| meshes.add(mesh));
            entry.insert(baked);
        }
        let Some(Some(mesh)) = field.baked.get(&key).cloned() else {
            continue;
        };
        let Some(material) = field.material(tier).cloned() else {
            continue;
        };
        let origin = chunk_origin(coord);
        let (low, high) = chunk_vertical_bounds(coord, seed, tier);
        let entity = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_xyz(origin.x, 0.0, origin.y),
                // Supplied explicitly because the mesh is render-world only:
                // `calculate_bounds` has no CPU copy to measure. Blades are
                // baked at absolute terrain height, so this is the ground under
                // the chunk plus one blade of headroom.
                Aabb::from_min_max(
                    Vec3::new(0.0, low, 0.0),
                    Vec3::new(GRASS_CHUNK_SIZE, high, GRASS_CHUNK_SIZE),
                ),
                // A field of blades makes for enormous shadow-map cost and
                // almost no readable shadow; the terrain underneath casts one.
                NotShadowCaster,
                GrassChunk,
                WorldGeometry,
            ))
            .id();
        field.live.insert(key, entity);
    }

    // One line per world, once the field has settled: enough to confirm the
    // streamer found ground and to spot a badly tuned density at a glance.
    // Fires on the first frame that needed no new bakes, which is when the ring
    // around the spawn point is fully resolved — including the paved city core,
    // where the honest answer is "almost no chunks".
    if !field.reported && baked_this_frame == 0 && !field.baked.is_empty() {
        field.reported = true;
        let blades: usize = GrassTier::ALL
            .iter()
            .map(|&tier| {
                let chunks = field.live.keys().filter(|(_, t)| *t == tier).count();
                chunks * (GRASS_CHUNK_SIZE * GRASS_CHUNK_SIZE * tier.profile().density) as usize
            })
            .sum();
        info!(
            "Grass field: {} chunks live, ~{} blade candidates, {} meshes cached",
            field.live.len(),
            blades,
            field.baked.len()
        );
    }

    // ── Drop meshes for ground the player has left ───────────────────────
    field.baked.retain(|(coord, tier), _| {
        nearest_focus_distance(*coord, &focuses) <= tier.profile().spawn_within + GRASS_CACHE_SLACK
    });
}

// ── Chunk geometry helpers ───────────────────────────────────────────────────

/// Union of every tier window around every focus. The value is the chunk's
/// distance to its nearest focus and drives the per-frame build priority.
fn wanted_grass_chunks(focuses: &[Vec2]) -> HashMap<(IVec2, GrassTier), f32> {
    let mut wanted = HashMap::new();
    let widest = GrassTier::ALL
        .iter()
        .map(|tier| tier.profile().spawn_within)
        .fold(0.0_f32, f32::max);
    let reach = (widest / GRASS_CHUNK_SIZE).ceil() as i32 + 1;

    for &focus in focuses {
        let anchor = chunk_of(focus);
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let coord = anchor + IVec2::new(dx, dz);
                let (near, far) = chunk_distance_bounds(coord, focus);
                for tier in GrassTier::ALL {
                    let profile = tier.profile();
                    if near > profile.spawn_within || far < profile.spawn_beyond {
                        continue;
                    }
                    wanted
                        .entry((coord, tier))
                        .and_modify(|nearest: &mut f32| *nearest = nearest.min(near))
                        .or_insert(near);
                }
            }
        }
    }

    // Membership is tier-specific, but build urgency is purely spatial: once
    // any view wants the chunk, prioritize it from the nearest active view.
    for ((coord, _), nearest) in &mut wanted {
        *nearest = nearest_focus_distance(*coord, focuses);
    }

    wanted
}

fn nearest_focus_distance(coord: IVec2, focuses: &[Vec2]) -> f32 {
    focuses
        .iter()
        .map(|&focus| chunk_distance_bounds(coord, focus).0)
        .fold(f32::INFINITY, f32::min)
}

fn chunk_of(point: Vec2) -> IVec2 {
    IVec2::new(
        (point.x / GRASS_CHUNK_SIZE).floor() as i32,
        (point.y / GRASS_CHUNK_SIZE).floor() as i32,
    )
}

fn chunk_origin(coord: IVec2) -> Vec2 {
    Vec2::new(
        coord.x as f32 * GRASS_CHUNK_SIZE,
        coord.y as f32 * GRASS_CHUNK_SIZE,
    )
}

/// Horizontal distance from `focus` to the nearest and farthest points of a
/// chunk's footprint. Tier membership uses both so that a chunk is live for the
/// whole span over which any of its blades could be on screen.
fn chunk_distance_bounds(coord: IVec2, focus: Vec2) -> (f32, f32) {
    let min = chunk_origin(coord);
    let max = min + Vec2::splat(GRASS_CHUNK_SIZE);
    let clamped = focus.clamp(min, max);
    let nearest = focus.distance(clamped);
    let farthest = focus
        .distance(min)
        .max(focus.distance(max))
        .max(focus.distance(Vec2::new(min.x, max.y)))
        .max(focus.distance(Vec2::new(max.x, min.y)));
    (nearest, farthest)
}

// ── Deterministic sampling ───────────────────────────────────────────────────

/// Integer hash in `0.0..1.0`. `seeded` in the parent module is fine for a few
/// hundred props but visibly correlates when it is asked for tens of thousands
/// of values on a lattice, which is exactly what blade placement does.
fn grass_hash(seed: u64, a: i32, b: i32, c: u32, salt: u32) -> f32 {
    let mut h = (seed as u32) ^ 0x9E37_79B9;
    h = h
        .rotate_left(13)
        .wrapping_mul(0x85EB_CA6B)
        .wrapping_add(a as u32);
    h = h
        .rotate_left(7)
        .wrapping_mul(0xC2B2_AE35)
        .wrapping_add(b as u32);
    h = h.rotate_left(11).wrapping_mul(0x27D4_EB2F).wrapping_add(c);
    h = h
        .rotate_left(5)
        .wrapping_mul(0x1656_67B1)
        .wrapping_add(salt);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}

/// Two-octave value noise used to clump blades into tufts. Uniform scatter
/// reads as artificial turf; real grass grows in patches with bare ground
/// between them.
fn clump_mask(x: f32, z: f32, seed: u64) -> f32 {
    let octave = |scale: f32, salt: u32| {
        let px = x * scale;
        let pz = z * scale;
        let ix = px.floor();
        let iz = pz.floor();
        let fx = px - ix;
        let fz = pz - iz;
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sz = fz * fz * (3.0 - 2.0 * fz);
        let lattice =
            |ox: f32, oz: f32| grass_hash(seed, (ix + ox) as i32, (iz + oz) as i32, 0, salt);
        let bottom = lattice(0.0, 0.0) + (lattice(1.0, 0.0) - lattice(0.0, 0.0)) * sx;
        let top = lattice(0.0, 1.0) + (lattice(1.0, 1.0) - lattice(0.0, 1.0)) * sx;
        bottom + (top - bottom) * sz
    };
    // Broad patches with a finer break-up laid over them. The smoothstep is
    // what makes this read as grass: a raw noise multiplier thins the whole
    // field evenly, whereas this keeps most ground fully covered and opens
    // occasional bare patches.
    let raw = octave(0.045, 17) * 0.68 + octave(0.19, 91) * 0.32;
    smoothstep(0.18, 0.58, raw)
}

/// Vertical extent of a chunk's blades, for its frustum-culling bounds.
///
/// Terrain is planar across a chunk, so the four corners bracket the ground and
/// the tallest possible blade tops it off. This matters: a chunk whose `Aabb`
/// spanned the whole grass altitude band would survive culling almost anywhere
/// the camera looked.
fn chunk_vertical_bounds(coord: IVec2, seed: u64, tier: GrassTier) -> (f32, f32) {
    let origin = chunk_origin(coord);
    let mut low = f32::MAX;
    let mut high = f32::MIN;
    for (dx, dz) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let y = terrain_surface_y(
            origin.x + dx * GRASS_CHUNK_SIZE,
            origin.y + dz * GRASS_CHUNK_SIZE,
            seed,
        );
        low = low.min(y);
        high = high.max(y);
    }
    // Tallest blade plus the headroom the shader's wind can add.
    (low - 0.5, high + 0.82 * tier.profile().height_scale + 0.5)
}

// ── Terrain masking ──────────────────────────────────────────────────────────

/// Everything about a chunk that is constant across its footprint.
struct ChunkGround {
    /// Multiplier applied to every blade in the chunk; zero means skip it.
    base_density: f32,
    /// Terrain colour sampled on a coarse grid and interpolated per blade.
    colors: Vec<[f32; 4]>,
}

fn survey_chunk(coord: IVec2, seed: u64) -> Option<ChunkGround> {
    let origin = chunk_origin(coord);
    let center = origin + Vec2::splat(GRASS_CHUNK_SIZE * 0.5);

    // Out past the coastline the terrain is sea floor.
    if center.x.abs().max(center.y.abs()) > GRASS_COAST_LIMIT {
        return None;
    }

    // Mountain lake basins are flooded; their surface sits above the terrain.
    for &(_, lake_x, lake_z, radius_x, radius_z) in MOUNTAIN_LAKES {
        let normalized = Vec2::new(
            (center.x - lake_x) / (radius_x + GRASS_CHUNK_SIZE),
            (center.y - lake_z) / (radius_z + GRASS_CHUNK_SIZE),
        )
        .length();
        if normalized < 1.0 {
            return None;
        }
    }

    // Slope is effectively constant inside a chunk because the terrain mesh has
    // one vertex every 62.5 m, so one probe serves the whole footprint.
    let normal = terrain_normal(center.x, center.y, seed);
    let slope = (1.0 - normal.y).clamp(0.0, 1.0);
    let slope_density = 1.0 - smoothstep(GRASS_SLOPE_FADE.0, GRASS_SLOPE_FADE.1, slope);
    if slope_density <= 0.0 {
        return None;
    }

    // Mountain routes carve trails; thin the grass out along them rather than
    // cutting a hard edge, because the influence field is hundreds of metres wide.
    let route = mountain_route_influence(center.x, center.y);
    let route_density = 1.0 - 0.85 * smoothstep(0.45, 0.95, route);

    let base_density = slope_density * route_density;
    if base_density <= 0.02 {
        return None;
    }

    let step = GRASS_CHUNK_SIZE / GRASS_GROUND_SAMPLES as f32;
    let mut colors = Vec::with_capacity((GRASS_GROUND_SAMPLES + 1).pow(2));
    for iz in 0..=GRASS_GROUND_SAMPLES {
        for ix in 0..=GRASS_GROUND_SAMPLES {
            let x = origin.x + ix as f32 * step;
            let z = origin.y + iz as f32 * step;
            let y = terrain_surface_y(x, z, seed);
            colors.push(terrain_vertex_color(x, z, y, normal, seed));
        }
    }

    Some(ChunkGround {
        base_density,
        colors,
    })
}

impl ChunkGround {
    /// Bilinear lookup into the coarse ground-colour grid, in chunk-local space.
    fn ground_color(&self, local_x: f32, local_z: f32) -> [f32; 3] {
        let stride = GRASS_GROUND_SAMPLES + 1;
        let gx = (local_x / GRASS_CHUNK_SIZE * GRASS_GROUND_SAMPLES as f32)
            .clamp(0.0, GRASS_GROUND_SAMPLES as f32);
        let gz = (local_z / GRASS_CHUNK_SIZE * GRASS_GROUND_SAMPLES as f32)
            .clamp(0.0, GRASS_GROUND_SAMPLES as f32);
        let x0 = (gx.floor() as usize).min(GRASS_GROUND_SAMPLES - 1);
        let z0 = (gz.floor() as usize).min(GRASS_GROUND_SAMPLES - 1);
        let tx = gx - x0 as f32;
        let tz = gz - z0 as f32;

        let mut out = [0.0_f32; 3];
        for (channel, output) in out.iter_mut().enumerate() {
            let c00 = self.colors[z0 * stride + x0][channel];
            let c10 = self.colors[z0 * stride + x0 + 1][channel];
            let c01 = self.colors[(z0 + 1) * stride + x0][channel];
            let c11 = self.colors[(z0 + 1) * stride + x0 + 1][channel];
            let bottom = c00 + (c10 - c00) * tx;
            let top = c01 + (c11 - c01) * tx;
            *output = bottom + (top - bottom) * tz;
        }
        out
    }
}

/// Surface normal of the rendered terrain, by central difference against the
/// same height function gameplay and the collider use.
fn terrain_normal(x: f32, z: f32, seed: u64) -> Vec3 {
    let dx = terrain_surface_y(x + GRASS_SLOPE_PROBE, z, seed)
        - terrain_surface_y(x - GRASS_SLOPE_PROBE, z, seed);
    let dz = terrain_surface_y(x, z + GRASS_SLOPE_PROBE, seed)
        - terrain_surface_y(x, z - GRASS_SLOPE_PROBE, seed);
    Vec3::new(-dx, 2.0 * GRASS_SLOPE_PROBE, -dz).normalize_or(Vec3::Y)
}

/// Height-dependent survival odds for a single blade.
fn altitude_density(y: f32) -> f32 {
    smoothstep(GRASS_LOW_ALTITUDE.0, GRASS_LOW_ALTITUDE.1, y)
        * (1.0 - smoothstep(GRASS_HIGH_ALTITUDE.0, GRASS_HIGH_ALTITUDE.1, y))
}

// ── Mesh baking ──────────────────────────────────────────────────────────────

/// Builds one chunk's merged blade mesh, or `None` when no grass grows there.
///
/// Vertices are chunk-local in X/Z and absolute in Y, matching the chunk
/// entity's transform. Attributes follow the contract documented at the top of
/// `assets/shaders/grass.wgsl`.
fn bake_chunk_mesh(coord: IVec2, tier: GrassTier, seed: u64) -> Option<Mesh> {
    let ground = survey_chunk(coord, seed)?;
    let profile = tier.profile();
    let origin = chunk_origin(coord);

    // A jittered grid instead of pure random scatter: it guarantees even
    // coverage without the clumping artefacts of rejection sampling, and the
    // clump mask then puts the *intentional* clumping back.
    let cells = ((GRASS_CHUNK_SIZE * profile.density.sqrt()).round() as i32).max(1);
    let cell_size = GRASS_CHUNK_SIZE / cells as f32;

    let vertices_per_blade = profile.segments * 2 + 1;
    let capacity = (cells * cells) as usize * vertices_per_blade;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(capacity);
    let mut indices: Vec<u16> = Vec::new();

    for cz in 0..cells {
        for cx in 0..cells {
            let index = (cz * cells + cx) as u32;
            let hash = |salt: u32| grass_hash(seed, coord.x, coord.y, index, salt);

            let local_x = (cx as f32 + hash(1)) * cell_size;
            let local_z = (cz as f32 + hash(2)) * cell_size;
            let world_x = origin.x + local_x;
            let world_z = origin.y + local_z;

            let y = terrain_surface_y(world_x, world_z, seed);
            let city = smoothstep(
                GRASS_CITY_CLEARANCE.0,
                GRASS_CITY_CLEARANCE.1,
                Vec2::new(world_x, world_z).length(),
            );
            let density = ground.base_density
                * altitude_density(y)
                * city
                * clump_mask(world_x, world_z, seed);
            if hash(3) > density {
                continue;
            }
            if positions.len() + vertices_per_blade > GRASS_MAX_CHUNK_VERTICES {
                break;
            }

            let blade_hash = hash(4);
            let height = (0.38 + hash(5) * 0.44) * profile.height_scale;
            let width = (0.028 + hash(6) * 0.024) * profile.width_scale;
            // Blades lean downwind by default with a wide spread, so the field
            // has a resting direction the wind then animates around.
            let yaw = hash(7) * TAU;
            let curl = 0.35 + hash(8) * 0.55;

            push_blade(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &mut indices,
                Vec3::new(local_x, y, local_z),
                yaw,
                height,
                width,
                curl,
                blade_hash,
                ground.ground_color(local_x, local_z),
                profile.segments,
            );
        }
    }

    if indices.is_empty() {
        return None;
    }

    // The mesh is only ever read by the GPU, and the chunk carries an explicit
    // `Aabb`, so there is no reason to keep a CPU copy resident.
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U16(indices));
    Some(mesh)
}

/// Appends one tapered, forward-curling blade.
///
/// The blade is a strip of `segments` quads narrowing to a single tip vertex.
/// It is built already curved: the shader only has to add wind on top, which
/// keeps the vertex program short enough to run over half a million times.
#[allow(clippy::too_many_arguments)]
fn push_blade(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u16>,
    base: Vec3,
    yaw: f32,
    height: f32,
    width: f32,
    curl: f32,
    blade_hash: f32,
    ground_color: [f32; 3],
    segments: usize,
) {
    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let side = Vec3::new(forward.z, 0.0, -forward.x);
    let first = positions.len() as u16;
    let color = [
        ground_color[0],
        ground_color[1],
        ground_color[2],
        blade_hash,
    ];

    // Cross-sections from the root up. `segments` two-vertex rows, then a tip.
    for row in 0..segments {
        let t = row as f32 / segments as f32;
        let droop = curl * t * t;
        // Curling forward costs height: without this the blade would stretch.
        let rise = height * t * (1.0 - 0.30 * droop);
        let reach = height * droop * 0.55;
        let center = base + forward * reach + Vec3::Y * rise;
        // Grass tapers to a point, but not linearly — the taper accelerates.
        let half = width * 0.5 * (1.0 - t).powf(0.55);

        // Tangent of the curve at this height, used for the face normal.
        let tangent = (Vec3::Y + forward * (2.0 * curl * t * 0.55)).normalize_or(Vec3::Y);
        let face = side.cross(tangent).normalize_or(forward);
        // Cupping the two edges outward makes a flat strip shade like the
        // curved ribbon a real blade is.
        let cup = 0.35;
        let left = (face + side * cup).normalize_or(face);
        let right = (face - side * cup).normalize_or(face);

        positions.push((center - side * half).to_array());
        normals.push(left.to_array());
        uvs.push([0.0, t]);
        colors.push(color);

        positions.push((center + side * half).to_array());
        normals.push(right.to_array());
        uvs.push([1.0, t]);
        colors.push(color);
    }

    // Tip vertex.
    let tip_droop = curl;
    let tip = base
        + forward * (height * tip_droop * 0.55)
        + Vec3::Y * (height * (1.0 - 0.30 * tip_droop));
    let tip_tangent = (Vec3::Y + forward * (2.0 * curl * 0.55)).normalize_or(Vec3::Y);
    let tip_normal = side.cross(tip_tangent).normalize_or(forward);
    positions.push(tip.to_array());
    normals.push(tip_normal.to_array());
    uvs.push([0.5, 1.0]);
    colors.push(color);

    // Strip between consecutive rows, then the closing triangle at the tip.
    for row in 0..segments - 1 {
        let lower = first + (row as u16) * 2;
        let upper = lower + 2;
        indices.extend_from_slice(&[lower, lower + 1, upper + 1]);
        indices.extend_from_slice(&[lower, upper + 1, upper]);
    }
    let last = first + ((segments - 1) as u16) * 2;
    let tip_index = first + (segments as u16) * 2;
    indices.extend_from_slice(&[last, last + 1, tip_index]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_windows_overlap_and_stay_inside_their_spawn_margins() {
        for tier in GrassTier::ALL {
            let profile = tier.profile();
            // Fade-in must complete before fade-out begins, or the tier is
            // never fully opaque anywhere.
            assert!(profile.range.x <= profile.range.y, "{tier:?} fade-in");
            assert!(profile.range.y <= profile.range.z, "{tier:?} plateau");
            assert!(profile.range.z < profile.range.w, "{tier:?} fade-out");
            // A chunk must be spawned before any of its blades become visible
            // and retired only after they are all gone, otherwise the streamer
            // pops geometry in and out on screen.
            assert!(
                profile.spawn_within > profile.range.w,
                "{tier:?} despawns while still visible"
            );
            assert!(
                profile.spawn_beyond <= profile.range.x.max(0.0),
                "{tier:?} spawns after it should already be visible"
            );
        }

        // Adjacent tiers have to overlap so one dissolves in as the other
        // leaves; a gap would show a bare ring of terrain.
        let near = GrassTier::Near.profile().range;
        let mid = GrassTier::Mid.profile().range;
        let far = GrassTier::Far.profile().range;
        assert!(mid.x < near.w, "near/mid gap");
        assert!(far.x < mid.w, "mid/far gap");
    }

    #[test]
    fn blade_topology_is_closed_and_indices_stay_in_range() {
        for segments in [2usize, 3, 4] {
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut colors = Vec::new();
            let mut indices = Vec::new();
            push_blade(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &mut indices,
                Vec3::ZERO,
                0.7,
                0.6,
                0.04,
                0.5,
                0.25,
                [0.2, 0.3, 0.1],
                segments,
            );

            assert_eq!(positions.len(), segments * 2 + 1);
            assert_eq!(normals.len(), positions.len());
            assert_eq!(uvs.len(), positions.len());
            assert_eq!(colors.len(), positions.len());
            assert_eq!(indices.len(), ((segments - 1) * 2 + 1) * 3);
            assert!(indices.iter().all(|&i| (i as usize) < positions.len()));

            // The root must sit exactly on the ground and the tip at v = 1, or
            // the shader's height-anchored sway would lift blades off terrain.
            assert_eq!(uvs[0], [0.0, 0.0]);
            assert_eq!(uvs[1], [1.0, 0.0]);
            assert!((positions[0][1]).abs() < 1e-6);
            assert!((positions[1][1]).abs() < 1e-6);
            assert_eq!(uvs[positions.len() - 1][1], 1.0);
        }
    }

    #[test]
    fn placement_is_deterministic_for_a_seed_and_chunk() {
        let a = grass_hash(7, -3, 11, 42, 5);
        let b = grass_hash(7, -3, 11, 42, 5);
        assert_eq!(a, b);
        assert!((0.0..1.0).contains(&a));
        // Neighbouring lattice cells must not correlate, or clumps line up in
        // visible rows across the field.
        assert!((grass_hash(7, -3, 11, 42, 5) - grass_hash(7, -2, 11, 42, 5)).abs() > 0.02);
    }

    #[test]
    fn chunk_distance_bounds_bracket_the_footprint() {
        let coord = IVec2::new(2, -1);
        let origin = chunk_origin(coord);
        let inside = origin + Vec2::splat(GRASS_CHUNK_SIZE * 0.5);
        let (near, far) = chunk_distance_bounds(coord, inside);
        assert_eq!(near, 0.0);
        assert!(far > 0.0);

        let outside = origin - Vec2::splat(40.0);
        let (near, far) = chunk_distance_bounds(coord, outside);
        assert!(near > 0.0 && near < far);
    }

    #[test]
    fn split_screen_streaming_unions_focus_rings_and_uses_nearest_priority() {
        let first_focus = Vec2::new(8.0, 8.0);
        let second_focus = Vec2::new(GRASS_CHUNK_SIZE * 40.0 + 8.0, 8.0);
        let first_coord = chunk_of(first_focus);
        let second_coord = chunk_of(second_focus);

        let first_only = wanted_grass_chunks(&[first_focus]);
        let wanted = wanted_grass_chunks(&[first_focus, second_focus]);

        assert!(wanted.len() > first_only.len());
        assert_eq!(wanted.get(&(first_coord, GrassTier::Near)), Some(&0.0));
        assert_eq!(wanted.get(&(second_coord, GrassTier::Near)), Some(&0.0));

        let midpoint = chunk_of((first_focus + second_focus) * 0.5);
        assert!(
            GrassTier::ALL
                .iter()
                .all(|&tier| !wanted.contains_key(&(midpoint, tier))),
            "the union must not turn separate focus rings into one giant field"
        );

        let near_second = second_coord + IVec2::new(2, 0);
        let expected_priority = [first_focus, second_focus]
            .into_iter()
            .map(|focus| chunk_distance_bounds(near_second, focus).0)
            .fold(f32::INFINITY, f32::min);
        assert_eq!(
            wanted.get(&(near_second, GrassTier::Near)),
            Some(&expected_priority),
            "missing chunks must be prioritized from their nearest active view"
        );
    }

    #[test]
    fn steep_and_submerged_ground_grows_nothing() {
        // Far past the coastline is sea floor.
        let offshore = chunk_of(Vec2::new(GRASS_COAST_LIMIT + 500.0, 0.0));
        assert!(survey_chunk(offshore, 42).is_none());

        // Lake basins are flooded.
        let (_, lake_x, lake_z, ..) = MOUNTAIN_LAKES[0];
        assert!(survey_chunk(chunk_of(Vec2::new(lake_x, lake_z)), 42).is_none());
    }

    #[test]
    fn altitude_band_excludes_the_waterline_and_the_snowline() {
        // The city-adjacent terrain is clamped flat at the world floor; grass
        // has to survive there or the whole spawn ring comes back bare.
        assert!(altitude_density(0.0) > 0.5);
        assert!(altitude_density(40.0) > 0.99);
        assert!(altitude_density(250.0) > 0.99);
        // Rock and snow take over up high.
        assert_eq!(altitude_density(GRASS_HIGH_ALTITUDE.1 + 10.0), 0.0);
        assert!(altitude_density(520.0) < 0.5);
    }
}
