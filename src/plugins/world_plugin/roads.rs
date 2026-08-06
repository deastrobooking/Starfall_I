//! Speed-road network generation: route profiles, deck/support spawning,
//! settlement rings and spurs, sky access, roadside stunt features, and the
//! initial NPC road-vehicle placement. Extracted verbatim from
//! `world_plugin.rs`; all terrain/settlement queries come from the parent
//! module via `use super::*`.

use super::*;
use crate::components::player::{GrappleSocket, GrappleTargetKind};
use crate::engine::rendering::SpatialBundle;

// Shorter stations let the elevation solver see narrow ridges before a deck
// reaches them.  This is also the maximum collider/deck seam spacing.
pub(super) const SPEED_ROAD_CHUNK_LEN: f32 = 32.0;
// The deck normally rides close to terrain. The profile solver raises it only
// where mountain clearance or the grade limit requires a viaduct.
pub(super) const SPEED_ROAD_CLEARANCE: f32 = 0.75;
pub(super) const SPEED_ROAD_TERRAIN_ENVELOPE: f32 = 0.65;
const SPEED_ROAD_PROFILE_SAFETY_MARGIN: f32 = 0.18;
// Four times the original per-direction lane width: enough room for four local
// riders, NPC traffic, enemy surfers, optional boost lines, and skate features.
pub(super) const SPEED_ROAD_WIDTH: f32 = 256.0;
// Less than half a terrain cell, so every triangle crossed by a road receives
// at least two longitudinal checks.
pub(super) const SPEED_ROAD_TERRAIN_SAMPLE_SPACING: f32 = 1.5;
pub(super) const SPEED_ROAD_TRAFFIC_LANE_OFFSET: f32 = 52.0;
pub(super) const SPEED_ROAD_PATROL_VEHICLE_COUNT: usize = 18;
pub(super) const RACE_REGION_ROAD_WIDTH: f32 = 384.0;
pub(super) const SPEED_ROAD_OUTER_WALL_HEIGHT: f32 = 12.0;
pub(super) const SPEED_ROAD_CENTER_WALL_HEIGHT: f32 = 7.5;

// Heavy Water's Star City silhouette returns at Starfall scale as a compact,
// elevated boost ring around the world origin. Offset the authored circle by
// half a chord so every cardinal access ramp meets the middle of a deck
// section rather than a collider seam.
pub(super) const STAR_CITY_RING_RADIUS: f32 = 280.0;
pub(super) const STAR_CITY_RING_HEIGHT: f32 = 80.0;
pub(super) const STAR_CITY_RING_WIDTH: f32 = 32.0;
pub(super) const STAR_CITY_RING_SEGMENTS: usize = 56;
pub(super) const STAR_CITY_RING_RAMP_COUNT: usize = 4;
pub(super) const STAR_CITY_RING_RAMP_RUN: f32 = 280.0;
pub(super) const STAR_CITY_RING_RAMP_SEGMENTS: usize = 24;
const STAR_CITY_RING_RAMP_WIDTH: f32 = 24.0;
const STAR_CITY_RING_GROUND_EMBED: f32 = 0.08;
const STAR_CITY_RING_LANE_SPAN_WIDTH: f32 = STAR_CITY_RING_WIDTH * 0.72;
const STAR_CITY_RING_LANE_OFFSET: f32 = STAR_CITY_RING_LANE_SPAN_WIDTH * 0.24;
const STAR_CITY_RING_DIVIDER_WIDTH: f32 = 2.4;
// Keep real trigger pads sparse on the short ring chords. Continuous emissive
// lane strips still sell the two-way boost loop, while eight stations avoid
// thousands of chevron/light entities and hundreds of per-pad meshes.
const STAR_CITY_RING_BOOST_INTERVAL: usize = 7;
const STAR_CITY_RING_BOOST_PHASE: usize = 3;

pub(super) fn speed_road_width_for_route(route_index: usize) -> f32 {
    if route_index >= BASE_MOUNTAIN_ROUTE_COUNT {
        RACE_REGION_ROAD_WIDTH
    } else {
        SPEED_ROAD_WIDTH + (route_index % 3) as f32 * 2.4
    }
}

pub(super) fn speed_road_segment_subdivision_count(length: f32) -> usize {
    (length / SPEED_ROAD_CHUNK_LEN).ceil().max(1.0) as usize
}

/// Convert authored route corners into a clamped cubic-Hermite centerline.
/// Tangents are limited by each span chord and lateral deviation stays inside
/// the existing route-clearance corridor, so smoothing cannot create loops or
/// send a deck into prop-filled terrain. The terrain/deck pass subsequently
/// resamples this centerline at the finer freeway chunk length.
/// Smooth an authored waypoint polyline into a rounded centerline.
///
/// Uses a centripetal Catmull-Rom spline (alpha = 0.5, Barry–Goldman form):
/// unlike the uniform parameterization, it cannot cusp, loop, or overshoot
/// within a span even when authored waypoint spacing is very uneven — the
/// standard "advanced" road-spline choice. Sample density adapts to the
/// local turn angle so hairpins stay genuinely round while long straights
/// stay cheap. There is deliberately no chord-deviation clamp: the terrain
/// distance field and route projection follow this same centerline, so the
/// deck is free to cut deep, natural arcs through corners.
/// Stations that must stay exactly on the centerline: every authored route
/// endpoint. Connector roads join trunk routes at these coordinates, and the
/// junction height-sharing pass matches them by rounded X/Z — cutting the
/// corner there would disconnect the network.
fn pinned_route_stations() -> &'static std::collections::HashSet<(i32, i32)> {
    static PINNED: std::sync::OnceLock<std::collections::HashSet<(i32, i32)>> =
        std::sync::OnceLock::new();
    PINNED.get_or_init(|| {
        mountain_routes()
            .iter()
            .flat_map(|route| [route.first(), route.last()])
            .flatten()
            .map(|&(x, z)| (x.round() as i32, z.round() as i32))
            .collect()
    })
}

/// Corner-cutting pass: replace every free interior waypoint with a pair of
/// setback points on its legs, so the spline arcs inside the apex instead of
/// driving through it. Pinned junction stations and straight-through
/// waypoints are kept exact.
fn corner_cut_guides(points: &[Vec2]) -> Vec<Vec2> {
    const MAX_FILLET_SETBACK: f32 = 180.0;
    let pinned = pinned_route_stations();
    let mut guides = Vec::with_capacity(points.len() * 2);
    guides.push(points[0]);
    for index in 1..points.len() - 1 {
        let apex = points[index];
        let before = points[index - 1];
        let after = points[index + 1];
        let d1 = (apex - before).normalize_or_zero();
        let d2 = (after - apex).normalize_or_zero();
        let pinned_station = pinned.contains(&(apex.x.round() as i32, apex.y.round() as i32));
        if pinned_station || d1.dot(d2) > 0.995 {
            guides.push(apex);
            continue;
        }
        let setback =
            (before.distance(apex).min(apex.distance(after)) * 0.35).min(MAX_FILLET_SETBACK);
        guides.push(apex - d1 * setback);
        guides.push(apex + d2 * setback);
    }
    guides.push(points[points.len() - 1]);
    guides
}

pub(super) fn rounded_speed_road_route(route: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if route.len() < 3 {
        return route.to_vec();
    }
    let points = corner_cut_guides(
        &route
            .iter()
            .map(|&(x, z)| Vec2::new(x, z))
            .collect::<Vec<_>>(),
    );
    fn push(rounded: &mut Vec<(f32, f32)>, sample: Vec2) {
        if rounded.last().is_none_or(|last: &(f32, f32)| {
            Vec2::new(last.0, last.1).distance_squared(sample) > 0.01
        }) {
            rounded.push((sample.x, sample.y));
        }
    }
    let mut rounded = Vec::new();
    for span in 0..points.len() - 1 {
        let p1 = points[span];
        let p2 = points[span + 1];
        let chord_len = p1.distance(p2);
        if chord_len <= 0.01 {
            continue;
        }
        let p0 = points[span.saturating_sub(1)];
        let p3 = points[(span + 2).min(points.len() - 1)];
        // Turn severity across the span's endpoints drives sample density:
        // 0.0 for straight-through, 2.0 for a full reversal.
        let in_dir = (p2 - p0).normalize_or_zero();
        let out_dir = (p3 - p1).normalize_or_zero();
        let turn = 1.0 - in_dir.dot(out_dir).clamp(-1.0, 1.0);
        let by_length = (chord_len / 72.0).ceil();
        let by_turn = (turn * 26.0).ceil();
        let slices = (by_length + by_turn).clamp(4.0, 48.0) as usize;
        for slice in 0..slices {
            let t = slice as f32 / slices as f32;
            push(&mut rounded, centripetal_catmull_rom(p0, p1, p2, p3, t));
        }
    }
    if let Some(last) = points.last() {
        push(&mut rounded, *last);
    }
    rounded
}

/// Centripetal Catmull-Rom (alpha = 0.5) evaluated on the `p1..p2` span via
/// the Barry–Goldman recursive form, which stays exact for repeated or
/// near-coincident control points (natural end conditions).
fn centripetal_catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let knot = |a: Vec2, b: Vec2, previous: f32| previous + a.distance(b).max(1.0e-4).sqrt();
    let t0 = 0.0;
    let t1 = knot(p0, p1, t0);
    let t2 = knot(p1, p2, t1);
    let t3 = knot(p2, p3, t2);
    let t = t1 + (t2 - t1) * t.clamp(0.0, 1.0);
    let lerp_at = |a: Vec2, b: Vec2, ta: f32, tb: f32| -> Vec2 {
        if (tb - ta).abs() <= 1.0e-5 {
            a
        } else {
            a.lerp(b, (t - ta) / (tb - ta))
        }
    };
    let a1 = lerp_at(p0, p1, t0, t1);
    let a2 = lerp_at(p1, p2, t1, t2);
    let a3 = lerp_at(p2, p3, t2, t3);
    let b1 = lerp_at(a1, a2, t0, t2);
    let b2 = lerp_at(a2, a3, t1, t3);
    lerp_at(b1, b2, t1, t2)
}

pub(super) fn speed_road_required_terrain_lift(
    start: Vec3,
    end: Vec3,
    width: f32,
    terrain_seed: u64,
    clearance: f32,
) -> f32 {
    let delta_xz = Vec2::new(end.x - start.x, end.z - start.z);
    let horizontal_len = delta_xz.length();
    if horizontal_len <= f32::EPSILON {
        return 0.0;
    }

    let dir = delta_xz / horizontal_len;
    let right = Vec2::new(dir.y, -dir.x);
    let samples = (horizontal_len / SPEED_ROAD_TERRAIN_SAMPLE_SPACING)
        .ceil()
        .clamp(1.0, 512.0) as usize;
    let mut needed_lift: f32 = 0.0;

    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let center = Vec2::new(start.x + delta_xz.x * t, start.z + delta_xz.y * t);
        let road_y = start.y + (end.y - start.y) * t;
        let lateral_samples = (width / 8.0).ceil().clamp(12.0, 48.0) as usize;
        for lateral_step in 0..=lateral_samples {
            let offset = -width * 0.5 + width * lateral_step as f32 / lateral_samples as f32;
            let sample = center + right * offset;
            needed_lift = needed_lift.max(
                terrain_surface_y(sample.x, sample.y, terrain_seed)
                    + clearance
                    + SPEED_ROAD_TERRAIN_ENVELOPE
                    - road_y,
            );
        }
    }

    needed_lift.max(0.0)
}

#[allow(dead_code)]
pub(super) fn mountain_speed_road_segment_count() -> usize {
    mountain_routes()
        .iter()
        .flat_map(|route| route.windows(2))
        .map(|pair| {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            speed_road_segment_subdivision_count(Vec2::new(bx - ax, bz - az).length())
        })
        .sum()
}

pub(super) fn settlement_speed_ring_radius_for_layout(
    kind: MapSettlementKind,
    layout: SettlementTerrainLayout,
) -> f32 {
    match kind {
        MapSettlementKind::City => (layout.platform_radius + 92.0).max(230.0),
        MapSettlementKind::Village => (layout.platform_radius + 54.0).max(116.0),
        MapSettlementKind::Harbor => (layout.platform_radius + 64.0).max(138.0),
        MapSettlementKind::Outpost => (layout.platform_radius + 50.0).max(104.0),
    }
}

pub(super) fn settlement_speed_ring_radius(seed: u64, settlement: MapSettlement) -> f32 {
    let layout =
        settlement_layout_profile(seed, settlement, settlement_plaza_radius(settlement.kind));
    settlement_speed_ring_radius_for_layout(settlement.kind, layout)
}

#[derive(Debug, Clone, Copy)]
struct StarCityRingRampPlan {
    merge: Vec2,
    ground_mouth: Vec2,
}

/// Closed chord loop for the elevated Star City ring. The duplicated final
/// point makes section count and collider continuity explicit to callers.
fn star_city_ring_points() -> Vec<Vec3> {
    let angle_step = TAU / STAR_CITY_RING_SEGMENTS as f32;
    (0..=STAR_CITY_RING_SEGMENTS)
        .map(|index| {
            let angle = index as f32 * angle_step - angle_step * 0.5;
            Vec3::new(
                angle.cos() * STAR_CITY_RING_RADIUS,
                STAR_CITY_RING_HEIGHT,
                angle.sin() * STAR_CITY_RING_RADIUS,
            )
        })
        .collect()
}

fn star_city_ring_has_boost_station(segment: usize) -> bool {
    segment % STAR_CITY_RING_BOOST_INTERVAL == STAR_CITY_RING_BOOST_PHASE
}

/// Four world-cardinal approaches whose elevated ends land on the outer lane
/// at the midpoint of the corresponding chord, where a protected opening
/// exists in the outside guardrail.
fn star_city_ring_ramp_plans() -> [StarCityRingRampPlan; STAR_CITY_RING_RAMP_COUNT] {
    let chord_midpoint_radius =
        STAR_CITY_RING_RADIUS * (std::f32::consts::PI / STAR_CITY_RING_SEGMENTS as f32).cos();
    let outer_lane_radius = chord_midpoint_radius + STAR_CITY_RING_LANE_OFFSET;
    std::array::from_fn(|index| {
        let angle = index as f32 * TAU / STAR_CITY_RING_RAMP_COUNT as f32;
        let direction = Vec2::new(angle.cos(), angle.sin());
        StarCityRingRampPlan {
            // End on the outer travel lane, not the centerline divider. The
            // chord's outer guardrail still opens at this longitudinal point,
            // creating a protected T-merge into either tangent direction.
            merge: direction * outer_lane_radius,
            ground_mouth: direction * (outer_lane_radius + STAR_CITY_RING_RAMP_RUN),
        }
    })
}

fn star_city_ring_ramp_points(ramp: StarCityRingRampPlan, terrain_seed: u64) -> Vec<Vec3> {
    let mouth_ground = terrain_surface_y(ramp.ground_mouth.x, ramp.ground_mouth.y, terrain_seed);
    // Start embedded slightly into the terrain, matching the established
    // ground-access roads, so the first deck has a driveable half-metre lip
    // instead of a two-metre vertical step.
    let mouth_height = mouth_ground + STAR_CITY_RING_GROUND_EMBED;
    let mut points = Vec::with_capacity(STAR_CITY_RING_RAMP_SEGMENTS + 1);
    for step in 0..=STAR_CITY_RING_RAMP_SEGMENTS {
        let t = step as f32 / STAR_CITY_RING_RAMP_SEGMENTS as f32;
        let xz = ramp.ground_mouth.lerp(ramp.merge, t);
        let eased = t * t * (3.0 - 2.0 * t);
        let engineered_height = mouth_height + (STAR_CITY_RING_HEIGHT - mouth_height) * eased;
        let terrain_clearance =
            terrain_surface_y(xz.x, xz.y, terrain_seed) + STAR_CITY_RING_GROUND_EMBED;
        points.push(Vec3::new(
            xz.x,
            engineered_height.max(terrain_clearance),
            xz.y,
        ));
    }
    if let Some(last) = points.last_mut() {
        *last = Vec3::new(ramp.merge.x, STAR_CITY_RING_HEIGHT, ramp.merge.y);
    }
    points
}

#[cfg(test)]
fn distance_to_star_city_ring_network(x: f32, z: f32) -> f32 {
    let point = Vec2::new(x, z);
    let mut best = (point.length() - STAR_CITY_RING_RADIUS).abs();
    for ramp in star_city_ring_ramp_plans() {
        best = best.min(distance_to_segment_xz(
            x,
            z,
            ramp.merge.x,
            ramp.merge.y,
            ramp.ground_mouth.x,
            ramp.ground_mouth.y,
        ));
    }
    best
}

/// Signed horizontal distance from the actual compact Star City road edge.
/// Negative values are on its 32-unit ring or 24-unit ramp footprint.
pub(super) fn signed_distance_to_star_city_road_footprint(x: f32, z: f32) -> f32 {
    let point = Vec2::new(x, z);
    let mut signed_edge =
        (point.length() - STAR_CITY_RING_RADIUS).abs() - STAR_CITY_RING_WIDTH * 0.5;
    for ramp in star_city_ring_ramp_plans() {
        signed_edge = signed_edge.min(
            distance_to_segment_xz(
                x,
                z,
                ramp.merge.x,
                ramp.merge.y,
                ramp.ground_mouth.x,
                ramp.ground_mouth.y,
            ) - STAR_CITY_RING_RAMP_WIDTH * 0.5,
        );
    }
    signed_edge
}

/// Express compact Star City edge distance in the legacy speed-road field,
/// whose consumers subtract the 256-unit trunk half-width themselves. This
/// prevents a 32-unit ring from clearing a 256-unit donut through downtown.
fn star_city_equivalent_speed_road_distance(x: f32, z: f32) -> f32 {
    signed_distance_to_star_city_road_footprint(x, z) + SPEED_ROAD_WIDTH * 0.5
}

pub(super) fn distance_to_speed_road_network(x: f32, z: f32, seed: u64) -> f32 {
    let mut best = distance_to_mountain_route_network(x, z)
        .min(star_city_equivalent_speed_road_distance(x, z));
    let point = Vec2::new(x, z);

    // Ground-access ramps extend beyond route endpoints. Include their
    // approach corridors in prop/building avoidance so the ramp mouths stay
    // driveable instead of being filled with trees.
    for route in mountain_routes() {
        if route.len() < 2 {
            continue;
        }
        for (endpoint, neighbor) in [
            (route[0], route[1]),
            (route[route.len() - 1], route[route.len() - 2]),
        ] {
            let end = Vec2::new(endpoint.0, endpoint.1);
            let inward = Vec2::new(neighbor.0, neighbor.1) - end;
            if inward.length_squared() <= 0.01 {
                continue;
            }
            let outer = end - inward.normalize() * 420.0;
            best = best.min(distance_to_segment_xz(x, z, outer.x, outer.y, end.x, end.y));
        }
    }
    for &(outer, merge) in sky_road_access_corridors(seed).iter() {
        best = best.min(distance_to_segment_xz(
            x, z, outer.x, outer.y, merge.x, merge.z,
        ));
    }

    for settlement in map_settlements().iter().copied() {
        let center = Vec2::new(settlement.x, settlement.z);
        let ring_radius = settlement_speed_ring_radius(seed, settlement);
        best = best.min((point.distance(center) - ring_radius).abs());

        if let Some(route_projection) = nearest_mountain_route_point(settlement.x, settlement.z) {
            let to_route = route_projection.point - center;
            if to_route.length_squared() > 0.01 {
                let spur_start = center + to_route.normalize() * ring_radius;
                best = best.min(distance_to_segment_xz(
                    x,
                    z,
                    spur_start.x,
                    spur_start.y,
                    route_projection.point.x,
                    route_projection.point.y,
                ));
            }
        }
    }

    best
}

#[allow(dead_code)]
pub(super) fn speed_road_loop_gate_count() -> usize {
    let route_gate_count = mountain_routes()
        .iter()
        .enumerate()
        .flat_map(|(ri, route)| {
            route.windows(2).enumerate().filter(move |(si, pair)| {
                let (ax, az) = pair[0];
                let (bx, bz) = pair[1];
                Vec2::new(bx - ax, bz - az).length() > 1_200.0 && (ri + *si) % 2 == 0
            })
        })
        .count();

    route_gate_count + map_settlements().len()
}

#[allow(dead_code)]
pub(super) fn full_speed_road_loop_count() -> usize {
    mountain_routes()
        .iter()
        .enumerate()
        .flat_map(|(ri, route)| {
            route.windows(2).enumerate().filter(move |(si, pair)| {
                let (ax, az) = pair[0];
                let (bx, bz) = pair[1];
                Vec2::new(bx - ax, bz - az).length() > 1_800.0 && (ri + *si) % 2 == 0
            })
        })
        .count()
}

pub(super) fn spawn_speed_road_network(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let deck_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    spawn_star_city_elevated_ring(commands, meshes, pal, &deck_mesh, terrain_seed);
    let road_profiles = speed_road_network_profiles(terrain_seed);

    for (ri, route) in mountain_routes().iter().enumerate() {
        let width = speed_road_width_for_route(ri);
        let profile = &road_profiles[ri];
        let sky_access_chunks = speed_road_valid_sky_access_chunks(profile, terrain_seed);
        for (chunk, points) in profile.windows(2).enumerate() {
            spawn_speed_road_deck_between(
                commands,
                meshes,
                pal,
                &deck_mesh,
                points[0],
                points[1],
                width,
                true,
                seed + ri as u64 * 7_001 + chunk as u64 * 19,
                terrain_seed,
                !sky_access_chunks.contains(&chunk),
            );
            spawn_speed_road_support_columns(
                commands,
                pal,
                &deck_mesh,
                points[0],
                points[1],
                width,
                terrain_seed,
            );
        }
        spawn_speed_road_ground_access(commands, pal, &deck_mesh, profile, width, terrain_seed);
        spawn_speed_road_sky_access(
            commands,
            pal,
            &deck_mesh,
            profile,
            &sky_access_chunks,
            width,
            terrain_seed,
        );

        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let delta = Vec2::new(bx - ax, bz - az);
            let segment_len = delta.length();
            if segment_len <= 20.0 {
                continue;
            }

            if segment_len > 1_200.0 && (ri + si) % 2 == 0 {
                let t = 0.48 + seeded(seed, ri as u64 * 179 + si as u64 * 23) * 0.10;
                let x = ax + (bx - ax) * t;
                let z = az + (bz - az) * t;
                let center = Vec3::new(
                    x,
                    road_profile_height_at(profile, x, z).unwrap_or_else(|| {
                        terrain_surface_y(x, z, terrain_seed) + SPEED_ROAD_CLEARANCE + 1.0
                    }),
                    z,
                );
                spawn_speed_loop_gate(
                    commands,
                    meshes,
                    pal,
                    center,
                    delta.x.atan2(delta.y),
                    22.0 + (ri % 2) as f32 * 5.0,
                    width,
                    seed + ri as u64 * 811 + si as u64 * 97,
                );
            }

            // Full vertical loop beside long straights: classic drivable
            // 360°, offset laterally so the straight lane stays clear for
            // NPC traffic. Riders steer into it for the stunt line.
            if segment_len > 1_800.0 && (ri + si) % 2 == 0 {
                let t = 0.35;
                let dirv = delta / segment_len;
                let sidev = Vec2::new(dirv.y, -dirv.x);
                let loop_w = width * 0.85;
                let entry = Vec2::new(ax + (bx - ax) * t, az + (bz - az) * t)
                    + sidev * (width * 0.5 + loop_w * 0.62);
                let road_height = road_profile_height_at(profile, entry.x, entry.y)
                    .unwrap_or(terrain_surface_y(entry.x, entry.y, terrain_seed));
                let base = Vec3::new(
                    entry.x,
                    road_height.max(
                        terrain_surface_y(entry.x, entry.y, terrain_seed)
                            + SPEED_ROAD_CLEARANCE
                            + 0.6,
                    ),
                    entry.y,
                );
                let dir3 = Vec3::new(dirv.x, 0.0, dirv.y);
                let radius = 24.0;
                let steps = 22;
                let mut prev = base;
                for step in 1..=steps {
                    let phi = step as f32 / steps as f32 * TAU;
                    let point =
                        base + dir3 * (radius * phi.sin()) + Vec3::Y * (radius * (1.0 - phi.cos()));
                    spawn_banked_deck_segment(
                        commands, pal, &deck_mesh, prev, point, loop_w, 0.0, false,
                    );
                    prev = point;
                }
                commands.spawn((
                    Transform::from_translation(base),
                    GlobalTransform::default(),
                    SpeedLoopGuide {
                        radius,
                        yaw: delta.x.atan2(delta.y),
                        entry_speed: 0.78,
                        lane_half_width: loop_w * 0.5,
                    },
                    WorldGeometry,
                ));

                // The guide takes over once the rider reaches the loop mouth,
                // but it does not provide any acceleration itself. Previously
                // these full loops only used boost-colored materials, so no
                // BoardBoostPad entity existed for board_boost_pad_system to
                // detect. Populate every loop with a real entrance pad that
                // supplies enough sustained speed to complete the stunt.
                spawn_board_boost_pad(
                    commands,
                    meshes,
                    pal,
                    base + Vec3::Y * 0.52,
                    delta.x.atan2(delta.y),
                    0.0,
                    dir3,
                    loop_w * 0.72,
                    radius * 0.78,
                    3.45,
                    4.25,
                    0.0,
                    2.10,
                );
            }
        }

        // Round every interior corner of this route with a banked fillet.
        let fillet_width = speed_road_width_for_route(ri);
        // The spine itself is rounded now; feeding the rounded centerline in
        // lets the fillet pass self-disable on smooth spans and only patch
        // residual sharp samples (e.g. clamped hairpins), aligned to the deck.
        let rounded_route = rounded_speed_road_route(route);
        spawn_route_corner_fillets(
            commands,
            pal,
            &deck_mesh,
            &rounded_route,
            fillet_width,
            terrain_seed,
        );
    }

    spawn_mountain_wrap_ramps(
        commands,
        meshes,
        pal,
        &deck_mesh,
        seed + 30_000,
        terrain_seed,
    );
    spawn_route_sweeper_curves(commands, pal, &deck_mesh, terrain_seed);
    spawn_hoverboard_trick_ramps(
        commands,
        meshes,
        pal,
        seed + 52_000,
        terrain_seed,
        &road_profiles,
    );
    spawn_stunt_park_features(
        commands,
        meshes,
        pal,
        seed + 58_000,
        terrain_seed,
        &road_profiles,
    );
    spawn_race_region_features(
        commands,
        meshes,
        pal,
        seed + 61_000,
        terrain_seed,
        &road_profiles,
    );

    for (si, settlement) in map_settlements().iter().copied().enumerate() {
        let plaza_radius = settlement_plaza_radius(settlement.kind);
        let layout = settlement_layout_profile(terrain_seed, settlement, plaza_radius);
        let ring_radius = settlement_speed_ring_radius_for_layout(settlement.kind, layout);
        spawn_settlement_speed_ring(
            commands,
            meshes,
            pal,
            &deck_mesh,
            settlement,
            layout,
            ring_radius,
            seed + si as u64 * 2_003,
            terrain_seed,
        );
        spawn_settlement_speed_spur(
            commands,
            meshes,
            pal,
            &deck_mesh,
            settlement,
            layout,
            ring_radius,
            seed + si as u64 * 2_311,
            terrain_seed,
        );
    }

    spawn_npc_road_vehicles(commands, meshes, pal, seed + 63_000, &road_profiles);
}

/// Build Heavy Water's elevated Star City speed ring as one continuous,
/// collidable loop with four protected cardinal merges. The loop retains
/// Starfall's two-way neon boost lanes and uses the shared viaduct supports;
/// each access ramp is a smooth terrain-aware climb onto the outer ring lane.
fn spawn_star_city_elevated_ring(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    terrain_seed: u64,
) {
    let ring_points = star_city_ring_points();
    let ramp_plans = star_city_ring_ramp_plans();
    let segments_per_quadrant = STAR_CITY_RING_SEGMENTS / STAR_CITY_RING_RAMP_COUNT;

    for (segment, points) in ring_points.windows(2).enumerate() {
        let start = points[0];
        let end = points[1];
        let has_cardinal_merge = segment.is_multiple_of(segments_per_quadrant);
        spawn_banked_deck_segment(
            commands,
            pal,
            deck_mesh,
            start,
            end,
            STAR_CITY_RING_WIDTH,
            0.0,
            !has_cardinal_merge,
        );

        if has_cardinal_merge {
            let ramp = ramp_plans[segment / segments_per_quadrant];
            spawn_merge_chunk_guardrails(
                commands,
                pal,
                deck_mesh,
                start,
                end,
                ramp.ground_mouth,
                STAR_CITY_RING_WIDTH,
            );
        }

        let delta = end - start;
        let length = delta.length();
        let yaw = delta.x.atan2(delta.z);
        let rot = Quat::from_rotation_y(yaw);
        let forward = rot * Vec3::Z;
        let right = rot * Vec3::X;
        let lane_span_width = STAR_CITY_RING_LANE_SPAN_WIDTH;
        let lane_width = (lane_span_width * 0.16).max(6.0);
        let lane_offset = STAR_CITY_RING_LANE_OFFSET;
        let road_surface = start.lerp(end, 0.5) + Vec3::Y * 0.58;

        // Shared-mesh emissive strips give the entire ring a continuous neon
        // identity without calling spawn_boost_road_span for every ~31m chord.
        for side in [-1.0_f32, 1.0] {
            let lane_center = road_surface + right * side * lane_offset;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(deck_mesh.clone()),
                    material: MeshMaterial3d(pal.boost_lane.clone()),
                    transform: Transform::from_translation(lane_center)
                        .with_rotation(rot)
                        .with_scale(Vec3::new(lane_width, 0.07, length * 0.98)),
                    ..default()
                },
                WorldGeometry,
            ));

            if star_city_ring_has_boost_station(segment) {
                let direction = if side > 0.0 { forward } else { -forward };
                let lane_yaw = if side > 0.0 {
                    yaw
                } else {
                    yaw + std::f32::consts::PI
                };
                spawn_board_boost_pad(
                    commands,
                    meshes,
                    pal,
                    lane_center + Vec3::Y * 0.08,
                    lane_yaw,
                    0.0,
                    direction,
                    lane_width * 0.84,
                    length.min(15.0),
                    3.15,
                    3.10,
                    0.0,
                    1.70,
                );
            }
        }

        // One shared-mesh physical divider per chord contains opposing boost
        // directions. Collider overlap across seams preserves continuity.
        let divider_height = SPEED_ROAD_CENTER_WALL_HEIGHT;
        let divider_width = STAR_CITY_RING_DIVIDER_WIDTH;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(deck_mesh.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    road_surface + Vec3::Y * (divider_height * 0.5 + 0.12),
                )
                .with_rotation(rot)
                .with_scale(Vec3::new(
                    divider_width,
                    divider_height,
                    length * 1.02,
                )),
                ..default()
            },
            WorldGeometry,
            RoadSafetyBarrier::DirectionDivider,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                divider_width * 0.5,
                divider_height * 0.5,
                length * 0.51,
            ),
            world_space_collider_scale(),
        ));
        spawn_speed_road_support_columns(
            commands,
            pal,
            deck_mesh,
            start,
            end,
            STAR_CITY_RING_WIDTH,
            terrain_seed,
        );
    }

    for (ramp_index, ramp) in ramp_plans.into_iter().enumerate() {
        let points = star_city_ring_ramp_points(ramp, terrain_seed);

        for (segment, pair) in points.windows(2).enumerate() {
            spawn_banked_deck_segment(
                commands,
                pal,
                deck_mesh,
                pair[0],
                pair[1],
                STAR_CITY_RING_RAMP_WIDTH,
                0.0,
                true,
            );
            spawn_speed_road_support_columns(
                commands,
                pal,
                deck_mesh,
                pair[0],
                pair[1],
                STAR_CITY_RING_RAMP_WIDTH,
                terrain_seed,
            );

            // Two real boost sections per climb establish the ring's fast
            // access line without carpeting every short curved ramp chord in
            // overlapping pads.
            if matches!(segment, 3 | 15) {
                let delta = pair[1] - pair[0];
                let horizontal_len = Vec2::new(delta.x, delta.z).length().max(0.001);
                spawn_board_boost_pad(
                    commands,
                    meshes,
                    pal,
                    pair[0].lerp(pair[1], 0.5) + Vec3::Y * 0.52,
                    delta.x.atan2(delta.z),
                    (delta.y / horizontal_len).atan(),
                    delta.normalize_or_zero(),
                    STAR_CITY_RING_RAMP_WIDTH * 0.62,
                    delta.length() * 1.35,
                    3.2,
                    3.4,
                    0.0,
                    1.8,
                );
            }
        }

        commands.spawn((
            Name::new(format!("Star City Ring Access {}", ramp_index + 1)),
            Transform::from_xyz(ramp.merge.x, STAR_CITY_RING_HEIGHT, ramp.merge.y),
            GlobalTransform::default(),
            WorldGeometry,
        ));
    }
}

pub(super) fn speed_road_terrain_point(x: f32, z: f32, terrain_seed: u64, clearance: f32) -> Vec3 {
    Vec3::new(x, terrain_surface_y(x, z, terrain_seed) + clearance, z)
}

/// Maximum sustained road grade (rise per horizontal unit, ≈12.5°). Real
/// mountain roads hold an engineered grade instead of hugging every ridge.
// Mountain trunks are allowed to climb with the terrain. A low global cap
// propagates remote summits backward through the connected graph and turns the
// whole network into a viaduct; the carved/smoothed mountain shelf permits a
// 100% short climb while ordinary sections remain much gentler,
// while endpoint access ramps independently target a gentle 8% approach.
pub(super) const SPEED_ROAD_MAX_GRADE: f32 = 1.0;

/// Build one continuous, terrain-safe elevation profile for a complete route.
///
/// Solving the whole polyline is important: solving each authored span in
/// isolation can leave a vertical step at a waypoint.  After the initial
/// center samples are grade-limited, every deck footprint is sampled across
/// its full width and lifted above any intervening ridge.  The final grade
/// pass only raises neighboring stations, so it cannot invalidate clearance.
pub(super) fn speed_road_route_profile(
    route: &[(f32, f32)],
    width: f32,
    terrain_seed: u64,
    clearance: f32,
) -> Vec<Vec3> {
    let mut profile = Vec::new();
    for pair in route.windows(2) {
        let a = Vec2::new(pair[0].0, pair[0].1);
        let b = Vec2::new(pair[1].0, pair[1].1);
        let distance = a.distance(b);
        if distance <= 4.0 {
            continue;
        }
        let chunks = speed_road_segment_subdivision_count(distance);
        for chunk in 0..=chunks {
            if !profile.is_empty() && chunk == 0 {
                continue;
            }
            let xz = a.lerp(b, chunk as f32 / chunks as f32);
            profile.push(speed_road_terrain_point(
                xz.x,
                xz.y,
                terrain_seed,
                clearance,
            ));
        }
    }

    grade_limit_road_points(&mut profile);

    // A lift is applied to both endpoints. Later lifts can only raise a
    // previously checked segment, so a single ordered pass is sufficient.
    for index in 0..profile.len().saturating_sub(1) {
        let lift = speed_road_required_terrain_lift(
            profile[index],
            profile[index + 1],
            width,
            terrain_seed,
            clearance,
        );
        profile[index].y += lift;
        profile[index + 1].y += lift;
    }
    grade_limit_road_points(&mut profile);
    profile
}

pub(super) fn speed_road_network_profiles(terrain_seed: u64) -> Vec<Vec<Vec3>> {
    let mut profiles: Vec<Vec<Vec3>> = mountain_routes()
        .iter()
        .enumerate()
        .map(|(route_index, route)| {
            let rounded = rounded_speed_road_route(route);
            speed_road_route_profile(
                &rounded,
                speed_road_width_for_route(route_index),
                terrain_seed,
                SPEED_ROAD_CLEARANCE + SPEED_ROAD_PROFILE_SAFETY_MARGIN,
            )
        })
        .collect();

    // Authored connector endpoints use the exact same X/Z as their trunk
    // junction. Share the highest solved elevation at those stations, then
    // propagate the grade outward. Repeating covers connected chains of roads.
    for _ in 0..8 {
        let mut junction_heights: HashMap<(i32, i32), f32> = HashMap::new();
        for profile in &profiles {
            for point in profile {
                let key = (point.x.round() as i32, point.z.round() as i32);
                junction_heights
                    .entry(key)
                    .and_modify(|height| *height = height.max(point.y))
                    .or_insert(point.y);
            }
        }
        for profile in &mut profiles {
            for point in profile.iter_mut() {
                let key = (point.x.round() as i32, point.z.round() as i32);
                point.y = point.y.max(junction_heights[&key]);
            }
            grade_limit_road_points(profile);
        }
    }
    profiles
}

pub(super) fn grade_limit_road_points(points: &mut [Vec3]) {
    if points.len() < 2 {
        return;
    }
    for _ in 0..2 {
        for i in 1..points.len() {
            let distance =
                Vec2::new(points[i].x - points[i - 1].x, points[i].z - points[i - 1].z).length();
            points[i].y = points[i]
                .y
                .max(points[i - 1].y - SPEED_ROAD_MAX_GRADE * distance);
        }
        for i in (0..points.len() - 1).rev() {
            let distance =
                Vec2::new(points[i + 1].x - points[i].x, points[i + 1].z - points[i].z).length();
            points[i].y = points[i]
                .y
                .max(points[i + 1].y - SPEED_ROAD_MAX_GRADE * distance);
        }
    }
}

pub(super) fn road_profile_height_at(profile: &[Vec3], x: f32, z: f32) -> Option<f32> {
    let target = Vec2::new(x, z);
    profile
        .windows(2)
        .filter_map(|points| {
            let a = Vec2::new(points[0].x, points[0].z);
            let b = Vec2::new(points[1].x, points[1].z);
            let delta = b - a;
            let length_squared = delta.length_squared();
            if length_squared <= f32::EPSILON {
                return None;
            }
            let t = ((target - a).dot(delta) / length_squared).clamp(0.0, 1.0);
            let distance_squared = target.distance_squared(a + delta * t);
            Some((
                distance_squared,
                points[0].y + (points[1].y - points[0].y) * t,
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, height)| height)
}

/// Return the 3D tangent of the road segment nearest an X/Z position. Unlike
/// the authored route direction, this includes the solved road grade.
pub(super) fn road_profile_tangent_at(profile: &[Vec3], x: f32, z: f32) -> Option<Vec3> {
    let target = Vec2::new(x, z);
    profile
        .windows(2)
        .filter_map(|points| {
            let a = Vec2::new(points[0].x, points[0].z);
            let b = Vec2::new(points[1].x, points[1].z);
            let delta_xz = b - a;
            let length_squared = delta_xz.length_squared();
            if length_squared <= f32::EPSILON {
                return None;
            }
            let t = ((target - a).dot(delta_xz) / length_squared).clamp(0.0, 1.0);
            let distance_squared = target.distance_squared(a + delta_xz * t);
            Some((
                distance_squared,
                (points[1] - points[0]).normalize_or_zero(),
            ))
        })
        .filter(|(_, tangent)| tangent.length_squared() > f32::EPSILON)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, tangent)| tangent)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_speed_road_deck_between(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    mut start: Vec3,
    mut end: Vec3,
    width: f32,
    boost_overlay: bool,
    seed: u64,
    terrain_seed: u64,
    guardrails: bool,
) {
    let mut delta = end - start;
    let horizontal_len = Vec2::new(delta.x, delta.z).length();
    if horizontal_len <= 4.0 {
        return;
    }

    let needed_lift = speed_road_required_terrain_lift(
        start,
        end,
        width,
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 1.0,
    );
    // Never cap this correction. A capped lift was the root cause of deck
    // colliders continuing straight into ridges that rose more than 2.5 units
    // between the old sparse samples.
    start.y += needed_lift;
    end.y += needed_lift;
    delta = end - start;

    let length = delta.length();
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan().clamp(-0.80, 0.80);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let center = start + delta * 0.5;
    let deck_thickness = 0.86;

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(deck_mesh.clone()),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(rot)
                .with_scale(Vec3::new(width, deck_thickness, length)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            width * 0.5,
            deck_thickness * 0.5,
            length * 0.5,
        ),
        ProceduralMaterialBinding::new("road.speed_network", "deck"),
        world_space_collider_scale(),
    ));

    // Guardrails: tall enough to catch vehicles/boards, WITH colliders so
    // nothing slides off the outside of the road network.
    if guardrails {
        spawn_deck_guardrails(commands, pal, deck_mesh, center, rot, width, length);
    }

    if boost_overlay {
        spawn_boost_road_span(
            commands,
            meshes,
            pal,
            center + Vec3::Y * 0.58,
            yaw,
            pitch,
            // Slight overlap across generated chunks keeps the center wall and
            // boost markings continuous at insane racing speeds.
            length * 1.02,
            width * 0.72,
            false,
            seed,
        );
    }
}

/// Shared guardrail spawner: one rail per side, collidable.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_deck_guardrails(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    center: Vec3,
    rot: Quat,
    width: f32,
    length: f32,
) {
    let rail_h = SPEED_ROAD_OUTER_WALL_HEIGHT;
    let rail_w = 2.6;
    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(deck_mesh.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    center + rot * Vec3::new(side * width * 0.505, rail_h * 0.5 + 0.35, 0.0),
                )
                .with_rotation(rot)
                .with_scale(Vec3::new(rail_w, rail_h, length * 0.99)),
                ..default()
            },
            WorldGeometry,
            RoadSafetyBarrier::OuterRail,
            ProceduralMaterialBinding::new("road.speed_network", "barrier"),
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                rail_w * 0.5,
                rail_h * 0.5,
                length * 0.495,
            ),
            world_space_collider_scale(),
        ));
    }
}

/// Paired structural pylons under viaduct portions of the speed-road network.
/// Supports are omitted when the deck is close to grade and grow wider on tall
/// aerial approaches. Their feet query the exact terrain trimesh surface.
pub(super) fn spawn_speed_road_support_columns(
    commands: &mut Commands,
    pal: &Palette,
    unit_cube: &Handle<Mesh>,
    start: Vec3,
    end: Vec3,
    road_width: f32,
    terrain_seed: u64,
) {
    let delta = end - start;
    let horizontal = Vec2::new(delta.x, delta.z);
    let length = horizontal.length();
    if length <= 4.0 {
        return;
    }
    let direction = horizontal / length;
    let right = Vec3::new(direction.y, 0.0, -direction.x);
    let yaw = direction.x.atan2(direction.y);
    let support_count = (length / 110.0).ceil().max(1.0) as usize;

    for support in 0..support_count {
        let t = (support as f32 + 0.5) / support_count as f32;
        let road_center = start.lerp(end, t);
        let center_ground = terrain_surface_y(road_center.x, road_center.z, terrain_seed);
        let center_clearance = road_center.y - center_ground;
        if center_clearance < 8.0 {
            continue;
        }

        let column_width = (2.8 + center_clearance * 0.012).clamp(3.0, 6.5);
        for side in [-1.0_f32, 1.0] {
            let position = road_center + right * side * road_width * 0.30;
            let ground = terrain_surface_y(position.x, position.z, terrain_seed);
            let top = road_center.y - 0.48;
            let height = (top - ground).max(1.0);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.bridge_stone.clone()),
                    transform: Transform::from_xyz(position.x, ground + height * 0.5, position.z)
                        .with_rotation(Quat::from_rotation_y(yaw))
                        .with_scale(Vec3::new(column_width, height, column_width)),
                    ..default()
                },
                WorldGeometry,
                ProceduralMaterialBinding::new("road.speed_network", "support"),
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    column_width * 0.5,
                    height * 0.5,
                    column_width * 0.5,
                ),
                world_space_collider_scale(),
            ));
        }

        // A visible cap ties the paired columns into one bridge bent.
        let grade = (delta.y / length).abs();
        let cap_drop = 1.8 + grade * column_width * 0.75;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(road_center - Vec3::Y * cap_drop)
                    .with_rotation(Quat::from_rotation_y(yaw))
                    .with_scale(Vec3::new(road_width * 0.76, 1.4, column_width * 1.25)),
                ..default()
            },
            WorldGeometry,
            ProceduralMaterialBinding::new("road.speed_network", "support"),
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                road_width * 0.38,
                0.7,
                column_width * 0.625,
            ),
            world_space_collider_scale(),
        ));
    }
}

/// Build a low, terrain-following entrance that merges into each end of an
/// engineered road profile. This keeps an aerial/raised deck reachable from
/// ordinary ground without requiring a jump or spawning a vertical end wall.
pub(super) fn spawn_speed_road_ground_access(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    profile: &[Vec3],
    road_width: f32,
    terrain_seed: u64,
) {
    if profile.len() < 2 {
        return;
    }

    for (endpoint, neighbor) in [
        (profile[0], profile[1]),
        (profile[profile.len() - 1], profile[profile.len() - 2]),
    ] {
        let inward = Vec2::new(neighbor.x - endpoint.x, neighbor.z - endpoint.z);
        if inward.length_squared() <= 0.01 {
            continue;
        }
        let outward = -inward.normalize();
        let endpoint_ground = terrain_surface_y(endpoint.x, endpoint.z, terrain_seed);
        let initial_rise = (endpoint.y - endpoint_ground).max(0.0);
        let mut length = (initial_rise / 0.08).clamp(90.0, 600.0);
        let mut outer = Vec2::new(endpoint.x, endpoint.z) + outward * length;
        let mut outer_ground = terrain_surface_y(outer.x, outer.y, terrain_seed);
        let required_rise = (endpoint.y - outer_ground).max(0.0);
        length = length.max((required_rise / 0.08).clamp(90.0, 600.0));
        outer = Vec2::new(endpoint.x, endpoint.z) + outward * length;
        outer_ground = terrain_surface_y(outer.x, outer.y, terrain_seed);

        let steps = (length / 8.0).ceil().max(8.0) as usize;
        let mut points = Vec::with_capacity(steps + 1);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let xz = outer.lerp(Vec2::new(endpoint.x, endpoint.z), t);
            let ground = terrain_surface_y(xz.x, xz.y, terrain_seed);
            let eased = t * t * (3.0 - 2.0 * t);
            let engineered_height = outer_ground + (endpoint.y - outer_ground) * eased;
            points.push(Vec3::new(xz.x, engineered_height.max(ground + 0.08), xz.y));
        }
        if let Some(last) = points.last_mut() {
            *last = endpoint;
        }

        for points in points.windows(2) {
            spawn_banked_deck_segment(
                commands,
                pal,
                deck_mesh,
                points[0],
                points[1],
                road_width * 0.88,
                0.0,
                true,
            );
        }
    }
}

pub(super) fn speed_road_sky_access_chunks(profile: &[Vec3], terrain_seed: u64) -> Vec<usize> {
    let mut access = Vec::new();
    let mut index = 0usize;
    while index + 1 < profile.len() {
        let midpoint = profile[index].lerp(profile[index + 1], 0.5);
        let aerial = midpoint.y - terrain_surface_y(midpoint.x, midpoint.z, terrain_seed) >= 10.0;
        if !aerial {
            index += 1;
            continue;
        }

        let run_start = index;
        while index + 1 < profile.len() {
            let point = profile[index].lerp(profile[index + 1], 0.5);
            if point.y - terrain_surface_y(point.x, point.z, terrain_seed) < 10.0 {
                break;
            }
            index += 1;
        }
        let run_end = index;
        let run_len = run_end.saturating_sub(run_start);
        if run_len < 2 {
            continue;
        }

        if run_len <= 28 {
            access.push(run_start + run_len / 2);
        } else {
            let mut chunk = run_start + 10;
            while chunk < run_end {
                access.push(chunk);
                chunk += 26;
            }
        }
    }
    access
}

pub(super) fn speed_road_sky_approach(
    merge: Vec3,
    forward: Vec2,
    terrain_seed: u64,
) -> Option<(Vec2, f32)> {
    let right = Vec2::new(forward.y, -forward.x);
    let mut best: Option<(f32, f32, f32, f32)> = None;
    for side in [-1.0_f32, 1.0] {
        for distance in (120..=600).step_by(60) {
            let xz = Vec2::new(merge.x, merge.z) + right * side * distance as f32;
            let ground = terrain_surface_y(xz.x, xz.y, terrain_seed);
            if ground + 0.5 >= merge.y {
                continue;
            }
            let mut obstruction = 0.0_f32;
            for sample in 1..16 {
                let t = sample as f32 / 16.0;
                let sample_xz = xz.lerp(Vec2::new(merge.x, merge.z), t);
                let line_y = ground + (merge.y - ground) * t;
                obstruction = obstruction
                    .max(terrain_surface_y(sample_xz.x, sample_xz.y, terrain_seed) + 0.08 - line_y);
            }
            let score = obstruction.max(0.0) * 10_000.0 + distance as f32 * 0.012;
            if best.is_none_or(|candidate| score < candidate.0) {
                best = Some((score, side, distance as f32, ground));
            }
        }
    }

    let best = best?;
    let rise = (merge.y - best.3).max(0.0);
    let length = best.2.max((rise / 0.10).clamp(120.0, 720.0));
    let outer = Vec2::new(merge.x, merge.z) + right * best.1 * length;
    let outer_ground = terrain_surface_y(outer.x, outer.y, terrain_seed);
    (outer_ground + 0.5 < merge.y).then_some((outer, outer_ground))
}

pub(super) fn speed_road_valid_sky_access_chunks(
    profile: &[Vec3],
    terrain_seed: u64,
) -> Vec<usize> {
    speed_road_sky_access_chunks(profile, terrain_seed)
        .into_iter()
        .filter(|&chunk| {
            let points = &profile[chunk..=chunk + 1];
            let merge = points[0].lerp(points[1], 0.5);
            let forward =
                Vec2::new(points[1].x - points[0].x, points[1].z - points[0].z).normalize_or_zero();
            forward.length_squared() > 0.01
                && speed_road_sky_approach(merge, forward, terrain_seed).is_some()
        })
        .collect()
}

pub(super) fn sky_road_access_corridors(terrain_seed: u64) -> Arc<Vec<(Vec2, Vec3)>> {
    let cache = SKY_ROAD_ACCESS_CORRIDORS
        .get_or_init(|| Mutex::new(BoundedSeedCache::new(MAX_CACHED_WORLD_SEEDS)));
    if let Some(corridors) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(terrain_seed)
    {
        return corridors;
    }

    let profiles = speed_road_network_profiles(terrain_seed);
    let mut corridors = Vec::new();
    for profile in &profiles {
        for chunk in speed_road_valid_sky_access_chunks(profile, terrain_seed) {
            let points = &profile[chunk..=chunk + 1];
            let merge = points[0].lerp(points[1], 0.5);
            let forward =
                Vec2::new(points[1].x - points[0].x, points[1].z - points[0].z).normalize_or_zero();
            if forward.length_squared() <= 0.01 {
                continue;
            }
            if let Some((outer, _)) = speed_road_sky_approach(merge, forward, terrain_seed) {
                corridors.push((outer, merge));
            }
        }
    }
    let corridors = Arc::new(corridors);
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(terrain_seed, corridors)
}

/// Branch ramps connect sustained aerial runs directly to nearby ground. The
/// matching main-road chunk omits its guardrails, creating a physical merge
/// opening instead of ending the ramp against an invisible barrier.
pub(super) fn spawn_speed_road_sky_access(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    profile: &[Vec3],
    access_chunks: &[usize],
    road_width: f32,
    terrain_seed: u64,
) {
    for &chunk in access_chunks {
        let Some(points) = profile.get(chunk..=chunk + 1) else {
            continue;
        };
        let merge = points[0].lerp(points[1], 0.5);
        let forward =
            Vec2::new(points[1].x - points[0].x, points[1].z - points[0].z).normalize_or_zero();
        if forward.length_squared() <= 0.01 {
            continue;
        }
        let Some((outer_xz, outer_ground)) = speed_road_sky_approach(merge, forward, terrain_seed)
        else {
            continue;
        };
        let length = outer_xz.distance(Vec2::new(merge.x, merge.z));
        let steps = (length / 18.0).ceil().max(8.0) as usize;
        let mut ramp = Vec::with_capacity(steps + 1);
        let mut last_height = outer_ground + 0.08;
        let mut blocked = outer_ground + 0.5 >= merge.y;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let xz = outer_xz.lerp(Vec2::new(merge.x, merge.z), t);
            let ground = terrain_surface_y(xz.x, xz.y, terrain_seed);
            let ramp_height = outer_ground + (merge.y - outer_ground) * t;
            let height = ramp_height.max(ground + 0.08).max(last_height);
            blocked |= height > merge.y + 0.01;
            last_height = height;
            ramp.push(Vec3::new(xz.x, height, xz.y));
        }
        if blocked {
            continue;
        }
        spawn_merge_chunk_guardrails(
            commands, pal, deck_mesh, points[0], points[1], outer_xz, road_width,
        );
        if let Some(last) = ramp.last_mut() {
            *last = merge;
        }

        // Raise the approach before any remaining terrain step. This removes
        // the short up/down bumps that were bleeding speed from the player.
        for index in (0..ramp.len().saturating_sub(1)).rev() {
            let distance = Vec2::new(
                ramp[index + 1].x - ramp[index].x,
                ramp[index + 1].z - ramp[index].z,
            )
            .length();
            ramp[index].y = ramp[index].y.max(ramp[index + 1].y - distance * 0.22);
        }

        let ramp_width = road_width * 0.46;
        let segment_count = ramp.len().saturating_sub(1);
        for (segment, points) in ramp.windows(2).enumerate() {
            spawn_banked_deck_segment(
                commands,
                pal,
                deck_mesh,
                points[0],
                points[1],
                ramp_width,
                0.0,
                segment + 2 < segment_count,
            );
            spawn_speed_road_support_columns(
                commands,
                pal,
                deck_mesh,
                points[0],
                points[1],
                ramp_width,
                terrain_seed,
            );
        }
    }
}

/// Restore containment on a sky-road merge chunk while leaving only the ramp
/// mouth open. Previously the entire chunk omitted both rails, creating a long
/// unprotected aerial section.
pub(super) fn spawn_merge_chunk_guardrails(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    start: Vec3,
    end: Vec3,
    approach: Vec2,
    width: f32,
) {
    let delta = end - start;
    let length = delta.length();
    let horizontal = Vec2::new(delta.x, delta.z);
    if length < 1.0 || horizontal.length_squared() < 0.01 {
        return;
    }
    let horizontal_len = horizontal.length();
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan().clamp(-0.80, 0.80);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let center = start + delta * 0.5;
    let right = Vec2::new(horizontal.y, -horizontal.x).normalize_or_zero();
    let approach_side = (approach - Vec2::new(center.x, center.z))
        .dot(right)
        .signum();
    let rail_h = 3.4;
    let rail_w = 0.90;
    let gap = (width * 0.58).min(length * 0.70).max(length * 0.15);

    for side in [-1.0_f32, 1.0] {
        let on_approach_side = side * approach_side > 0.0;
        let spans: Vec<(f32, f32)> = if on_approach_side {
            let span_len = (length - gap) * 0.5;
            let offset = gap * 0.5 + span_len * 0.5;
            vec![(-offset, span_len), (offset, span_len)]
        } else {
            vec![(0.0, length * 0.99)]
        };
        for (local_z, span_len) in spans {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(deck_mesh.clone()),
                    material: MeshMaterial3d(pal.brushed_metal.clone()),
                    transform: Transform::from_translation(
                        center
                            + rot * Vec3::new(side * width * 0.505, rail_h * 0.5 + 0.35, local_z),
                    )
                    .with_rotation(rot)
                    .with_scale(Vec3::new(rail_w, rail_h, span_len)),
                    ..default()
                },
                WorldGeometry,
                RoadSafetyBarrier::MergeRail,
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    rail_w * 0.5,
                    rail_h * 0.5,
                    span_len * 0.5,
                ),
                world_space_collider_scale(),
            ));
        }
    }
}

/// A single deck chunk with full yaw/pitch/**roll** rotation — the building
/// block for banked corner fillets and vertical loops. Pitch is NOT clamped
/// (loops go vertical). `rails` adds collidable guardrails.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_banked_deck_segment(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    start: Vec3,
    end: Vec3,
    width: f32,
    roll: f32,
    rails: bool,
) {
    let delta = end - start;
    let length = delta.length();
    if length < 0.5 {
        return;
    }
    let horizontal_len = Vec2::new(delta.x, delta.z).length().max(0.001);
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan();
    let rot =
        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch) * Quat::from_rotation_z(roll);
    let center = start + delta * 0.5;
    let deck_thickness = 0.86;

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(deck_mesh.clone()),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(rot)
                .with_scale(Vec3::new(width, deck_thickness, length * 1.04)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            width * 0.5,
            deck_thickness * 0.5,
            length * 0.52,
        ),
        world_space_collider_scale(),
    ));
    if rails {
        spawn_deck_guardrails(commands, pal, deck_mesh, center, rot, width, length);
    }
}

/// Banked corner fillets: rounds every interior route waypoint with a
/// quadratic-arc of short decks, rolled toward the inside of the turn, so
/// route corners drive like road curves instead of sharp wall joints.
pub(super) fn spawn_route_corner_fillets(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    route: &[(f32, f32)],
    width: f32,
    terrain_seed: u64,
) {
    for w in route.windows(3) {
        let a = Vec2::new(w[0].0, w[0].1);
        let b = Vec2::new(w[1].0, w[1].1);
        let c = Vec2::new(w[2].0, w[2].1);
        let d1 = (b - a).normalize_or_zero();
        let d2 = (c - b).normalize_or_zero();
        if d1.dot(d2) > 0.985 {
            continue; // effectively straight — no fillet needed
        }
        let turn = d1.perp_dot(d2);
        let setback = ((b - a).length().min((c - b).length()) * 0.25).min(220.0);
        let p0 = b - d1 * setback;
        let p2 = b + d2 * setback;
        let samples = 7usize;
        let curve: Vec<(f32, f32)> = (0..=samples)
            .map(|i| {
                let t = i as f32 / samples as f32;
                // Quadratic Bézier with the waypoint as control point.
                let q = p0.lerp(b, t).lerp(b.lerp(p2, t), t);
                (q.x, q.y)
            })
            .collect();
        // Curves wander outside the straight centerline corridor, so solve
        // their entire footprint too instead of terrain-hugging only their
        // endpoints. This prevents a rounded corner from cutting into a peak.
        let pts =
            speed_road_route_profile(&curve, width, terrain_seed, SPEED_ROAD_CLEARANCE + 1.12);
        let roll = turn.signum() * 0.15; // bank into the turn
        for i in 0..pts.len().saturating_sub(1) {
            spawn_banked_deck_segment(
                commands,
                pal,
                deck_mesh,
                pts[i],
                pts[i + 1],
                width,
                roll,
                true,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_settlement_speed_ring(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let segment_count = match settlement.kind {
        MapSettlementKind::City => 18,
        MapSettlementKind::Harbor => 14,
        _ => 12,
    };
    let width = match settlement.kind {
        MapSettlementKind::City => 46.0,
        MapSettlementKind::Harbor => 42.0,
        MapSettlementKind::Village => 38.0,
        MapSettlementKind::Outpost => 36.0,
    };

    let opponent_path = (0..=24)
        .map(|step| {
            let angle = step as f32 * TAU / 24.0;
            settlement_speed_ring_point(settlement, layout, ring_radius, angle, terrain_seed)
                + Vec3::Y * 1.7
        })
        .collect::<Vec<_>>();

    for segment in 0..segment_count {
        let a0 = segment as f32 * TAU / segment_count as f32;
        let a1 = (segment + 1) as f32 * TAU / segment_count as f32;
        let start = settlement_speed_ring_point(settlement, layout, ring_radius, a0, terrain_seed);
        let end = settlement_speed_ring_point(settlement, layout, ring_radius, a1, terrain_seed);
        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            start,
            end,
            width,
            true,
            seed + segment as u64 * 31,
            terrain_seed,
            true,
        );
    }

    for gate_index in 0..4u8 {
        let angle = gate_index as f32 * TAU / 4.0;
        let position =
            settlement_speed_ring_point(settlement, layout, ring_radius, angle, terrain_seed)
                + Vec3::Y * (width * 0.34);
        let tangent = Vec2::new(-angle.sin(), angle.cos()).normalize_or_zero();
        let yaw = tangent.x.atan2(tangent.y);
        commands.spawn((
            Name::new(format!("{} Race Gate {}", settlement.name, gate_index + 1)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus {
                    major_radius: width * 0.34,
                    minor_radius: 0.62,
                })),
                material: MeshMaterial3d(if gate_index == 0 {
                    pal.crystal_dragon.clone()
                } else {
                    pal.crystal_aurora.clone()
                }),
                transform: Transform::from_translation(position).with_rotation(
                    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
                ..default()
            },
            StuntRaceGate {
                course_id: settlement.anchor_id,
                course_label: settlement.name,
                gate_index,
                gate_count: 4,
                radius: width * 0.58,
            },
            WorldGeometry,
        ));
    }

    let opponent_mesh = meshes.add(Cuboid::new(6.4, 1.7, 10.2));
    for racer_id in 0..2u8 {
        let lane_offset = if racer_id == 0 { -7.0 } else { 7.0 };
        let vehicle = NpcRoadVehicle {
            path: opponent_path.clone(),
            segment: usize::from(racer_id) * 8,
            progress: 0.0,
            speed: 46.0 + racer_id as f32 * 5.0,
            lane_offset,
            hit_radius: 5.4,
            wreck_timer: 4.0,
        };
        let (position, yaw) = road_vehicle_pose(&vehicle).unwrap_or((opponent_path[0], 0.0));
        commands.spawn((
            Name::new(format!("{} Rival Racer {}", settlement.name, racer_id + 1)),
            PbrBundle {
                mesh: Mesh3d(opponent_mesh.clone()),
                material: MeshMaterial3d(if racer_id == 0 {
                    pal.city_metal_panel.clone()
                } else {
                    pal.crystal_emerald.clone()
                }),
                transform: Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                ..default()
            },
            StuntRaceOpponent {
                racer_id,
                course_id: settlement.anchor_id,
            },
            vehicle,
            WorldGeometry,
            Health::new(120.0),
            Damageable::default(),
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            crate::engine::physics::prelude::Collider::cuboid(3.2, 0.85, 5.1),
            crate::engine::physics::prelude::CollisionProfile::VehicleHurtbox,
        ));
    }

    let gate_angle = std::f32::consts::FRAC_PI_2 - settlement.facing_yaw;
    let gate_pos =
        settlement_speed_ring_point(settlement, layout, ring_radius, gate_angle, terrain_seed);
    let tangent = Vec2::new(-gate_angle.sin(), gate_angle.cos()).normalize_or_zero();
    let yaw = tangent.x.atan2(tangent.y);
    spawn_speed_loop_gate(
        commands,
        meshes,
        pal,
        gate_pos + Vec3::Y * 0.72,
        yaw,
        if matches!(settlement.kind, MapSettlementKind::City) {
            27.0
        } else {
            20.0
        },
        width,
        seed + 997,
    );
}

pub(super) fn settlement_speed_ring_point(
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    angle: f32,
    terrain_seed: u64,
) -> Vec3 {
    let x = settlement.x + angle.cos() * ring_radius;
    let z = settlement.z + angle.sin() * ring_radius;
    let y =
        layout.floor_y_at(x, z, terrain_seed) + if layout.is_sky_district() { 3.2 } else { 2.4 };
    Vec3::new(x, y, z)
}

#[allow(dead_code)]
pub(super) fn stunt_race_gate_count() -> usize {
    map_settlements().len() * 4
}

#[allow(dead_code)]
pub(super) fn stunt_race_opponent_count() -> usize {
    map_settlements().len() * 2
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_settlement_speed_spur(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let Some(route_projection) = nearest_mountain_route_point(settlement.x, settlement.z) else {
        return;
    };
    let center = Vec2::new(settlement.x, settlement.z);
    let to_route = route_projection.point - center;
    if to_route.length_squared() <= 0.01 {
        return;
    }

    let dir = to_route.normalize();
    let start_angle = dir.y.atan2(dir.x);
    let start =
        settlement_speed_ring_point(settlement, layout, ring_radius, start_angle, terrain_seed);
    let end = speed_road_terrain_point(
        route_projection.point.x,
        route_projection.point.y,
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 0.6,
    );
    let total_len = Vec2::new(end.x - start.x, end.z - start.z).length();
    let chunk_count = speed_road_segment_subdivision_count(total_len).max(1);

    for chunk in 0..chunk_count {
        let t0 = chunk as f32 / chunk_count as f32;
        let t1 = (chunk + 1) as f32 / chunk_count as f32;
        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            start.lerp(end, t0),
            start.lerp(end, t1),
            SPEED_ROAD_WIDTH * 0.86,
            true,
            seed + chunk as u64 * 41,
            terrain_seed,
            true,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_route_sweeper_curves(
    commands: &mut Commands,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    terrain_seed: u64,
) {
    for route in mountain_routes() {
        for vi in 1..route.len().saturating_sub(1) {
            let (px, pz) = route[vi - 1];
            let (cx, cz) = route[vi];
            let (nx, nz) = route[vi + 1];
            let into = Vec2::new(cx - px, cz - pz).normalize_or_zero();
            let out = Vec2::new(nx - cx, nz - cz).normalize_or_zero();
            if into.length_squared() <= 0.01 || out.length_squared() <= 0.01 {
                continue;
            }
            let turn_strength = (1.0 - into.dot(out)).clamp(0.0, 2.0);
            if turn_strength < 0.10 {
                continue;
            }

            let p0 = Vec2::new(cx, cz) - into * 230.0;
            let p2 = Vec2::new(cx, cz) + out * 230.0;
            let outside = Vec2::new(-into.y - out.y, into.x + out.x).normalize_or_zero();
            let control = Vec2::new(cx, cz)
                + outside * (95.0 + turn_strength * 95.0)
                + (out - into).normalize_or_zero() * 36.0;
            let steps = 7usize;
            let curve: Vec<(f32, f32)> = (0..=steps)
                .map(|step| {
                    let point = quadratic_bezier_xz(p0, control, p2, step as f32 / steps as f32);
                    (point.x, point.y)
                })
                .collect();
            let width = SPEED_ROAD_WIDTH * (1.04 + turn_strength * 0.08);
            let profile =
                speed_road_route_profile(&curve, width, terrain_seed, SPEED_ROAD_CLEARANCE + 1.2);
            let bank = -into.perp_dot(out).signum() * (0.13 + turn_strength * 0.17).min(0.34);

            for points in profile.windows(2) {
                spawn_banked_deck_segment(
                    commands, pal, deck_mesh, points[0], points[1], width, bank, true,
                );
            }
        }
    }
}

pub(super) fn quadratic_bezier_xz(a: Vec2, control: Vec2, b: Vec2, t: f32) -> Vec2 {
    let inv = 1.0 - t;
    a * inv * inv + control * 2.0 * inv * t + b * t * t
}

#[allow(dead_code)]
pub(super) fn speed_road_sweeper_curve_count() -> usize {
    mountain_routes()
        .iter()
        .map(|route| route.len().saturating_sub(2))
        .sum()
}

pub(super) fn spawn_hoverboard_trick_ramps(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
    road_profiles: &[Vec<Vec3>],
) {
    let mut spawned = 0usize;
    for (ri, route) in mountain_routes().iter().enumerate() {
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let delta = Vec2::new(bx - ax, bz - az);
            let len = delta.length();
            if len < 900.0 {
                continue;
            }
            let yaw = delta.x.atan2(delta.y);
            let dir = Vec3::new(delta.x, 0.0, delta.y).normalize_or_zero();
            let right = Vec3::new(dir.z, 0.0, -dir.x).normalize_or_zero();
            for (slot, t) in [0.26_f32, 0.50, 0.74].into_iter().enumerate() {
                if spawned >= hoverboard_trick_ramp_count() {
                    return;
                }
                if (ri + si + slot) % 2 == 1 {
                    continue;
                }
                let x = ax + (bx - ax) * t;
                let z = az + (bz - az) * t;
                let lane = if slot == 0 { -1.0 } else { 1.0 };
                let ramp_x = x + right.x * lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET;
                let ramp_z = z + right.z * lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET;
                let base_center = Vec3::new(
                    ramp_x,
                    road_profile_height_at(&road_profiles[ri], ramp_x, ramp_z).unwrap_or_else(
                        || {
                            terrain_surface_y(ramp_x, ramp_z, terrain_seed)
                                + SPEED_ROAD_CLEARANCE
                                + 1.05
                        },
                    ) + 0.55,
                    ramp_z,
                );
                let road_direction = road_profile_tangent_at(&road_profiles[ri], ramp_x, ramp_z)
                    .filter(|tangent| tangent.dot(dir) >= 0.0)
                    .unwrap_or(dir);
                spawn_board_boost_ramp(
                    commands,
                    meshes,
                    pal,
                    base_center,
                    road_direction,
                    18.0,
                    54.0,
                    18.0,
                    7.2,
                    2.35,
                );
                spawn_speed_loop_gate(
                    commands,
                    meshes,
                    pal,
                    base_center + dir * 42.0 + Vec3::Y * 2.4,
                    yaw,
                    18.0 + seeded(seed, spawned as u64 * 17) * 7.0,
                    24.0,
                    seed + spawned as u64 * 97,
                );
                spawned += 1;
            }
        }
    }
}

pub(super) fn hoverboard_trick_ramp_count() -> usize {
    28
}

pub(super) const STUNT_GRIND_RAIL_LIMIT: usize = 18;

#[allow(dead_code)]
pub(super) fn stunt_grind_rail_count() -> usize {
    mountain_routes()
        .iter()
        .flat_map(|route| route.windows(2))
        .filter(|pair| Vec2::new(pair[1].0 - pair[0].0, pair[1].1 - pair[0].1).length() >= 1_050.0)
        .count()
        .min(STUNT_GRIND_RAIL_LIMIT)
}

pub(super) fn spawn_sonic_spring_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    direction: Vec3,
    launch_speed: f32,
    launch_lift: f32,
    force_hoverboard: bool,
) {
    let direction = direction.with_y(0.0).normalize_or_zero();
    let yaw = direction.x.atan2(direction.z);
    commands.spawn((
        Name::new("Sonic Spring Jump"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.0, 0.75))),
            material: MeshMaterial3d(pal.crystal_dragon.clone()),
            transform: Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y(yaw)),
            ..default()
        },
        SpringJumpPad {
            launch_velocity: direction * launch_speed + Vec3::Y * launch_lift,
            radius: 3.4,
            cooldown: 0.16,
            cooldown_timers: [0.0; 4],
            force_hoverboard,
        },
        GrappleSocket::new(GrappleTargetKind::ZipPoint)
            .with_radius(3.4)
            .with_priority(1.18),
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(0.375, 3.0),
    ));
    for layer in 0..3 {
        commands.spawn((
            Name::new("Spring Energy Coil"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus {
                    major_radius: 2.25 - layer as f32 * 0.22,
                    minor_radius: 0.18,
                })),
                material: MeshMaterial3d(pal.crystal_aurora.clone()),
                transform: Transform::from_translation(
                    position + Vec3::Y * (0.55 + layer as f32 * 0.38),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

pub(super) fn spawn_stunt_grind_rail(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    start: Vec3,
    end: Vec3,
    seed: u64,
) {
    let delta = end - start;
    let length = delta.length();
    if length < 8.0 {
        return;
    }
    let rotation = Quat::from_rotation_arc(Vec3::Y, delta / length);
    commands.spawn((
        Name::new("High Tech Grind Rail"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.38, length))),
            material: MeshMaterial3d(if seed.is_multiple_of(2) {
                pal.crystal_aurora.clone()
            } else {
                pal.crystal_emerald.clone()
            }),
            transform: Transform::from_translation(start.lerp(end, 0.5)).with_rotation(rotation),
            ..default()
        },
        StuntGrindRail {
            start,
            end,
            speed: 58.0 + (seed % 5) as f32 * 2.5,
            snap_radius: 2.8,
            exit_lift: 9.5,
        },
        GrappleSocket::new(GrappleTargetKind::SwingPoint)
            .with_radius(4.5)
            .with_priority(1.28),
        WorldGeometry,
    ));

    let support_count = (length / 24.0).ceil() as usize;
    for support in 0..=support_count {
        let t = support as f32 / support_count.max(1) as f32;
        let point = start.lerp(end, t);
        let support_height = 3.0 + (t * std::f32::consts::PI).sin() * 1.8;
        commands.spawn((
            Name::new("Grind Rail Support"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(0.28, support_height))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    point - Vec3::Y * (support_height * 0.5 + 0.25),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

pub(super) fn spawn_stunt_park_features(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
    road_profiles: &[Vec<Vec3>],
) {
    let mut spawned = 0usize;
    for (route_index, route) in mountain_routes().iter().enumerate() {
        for (segment_index, pair) in route.windows(2).enumerate() {
            if spawned >= STUNT_GRIND_RAIL_LIMIT {
                return;
            }
            let a = Vec2::new(pair[0].0, pair[0].1);
            let b = Vec2::new(pair[1].0, pair[1].1);
            let delta = b - a;
            let segment_length = delta.length();
            if segment_length < 1_050.0 {
                continue;
            }
            let direction_xz = delta / segment_length;
            let direction = Vec3::new(direction_xz.x, 0.0, direction_xz.y);
            let right = Vec3::new(direction.z, 0.0, -direction.x);
            let feature_seed = seed + route_index as u64 * 1_009 + segment_index as u64 * 71;
            let center_xz = a.lerp(b, 0.58 + (seeded(feature_seed, 3) - 0.5) * 0.12);
            let lane_side = if (route_index + segment_index).is_multiple_of(2) {
                -1.0
            } else {
                1.0
            };
            let rail_length = 92.0 + seeded(feature_seed, 7) * 54.0;
            let rail_center = Vec3::new(center_xz.x, 0.0, center_xz.y)
                + right * lane_side * SPEED_ROAD_TRAFFIC_LANE_OFFSET;
            let rail_start_xz = rail_center - direction * (rail_length * 0.5);
            let rail_end_xz = rail_center + direction * (rail_length * 0.5);
            let road_height = |point: Vec3| {
                road_profile_height_at(&road_profiles[route_index], point.x, point.z)
                    .unwrap_or_else(|| {
                        terrain_surface_y(point.x, point.z, terrain_seed) + SPEED_ROAD_CLEARANCE
                    })
            };
            let rail_start = Vec3::new(
                rail_start_xz.x,
                road_height(rail_start_xz) + 3.0,
                rail_start_xz.z,
            );
            let rail_end = Vec3::new(rail_end_xz.x, road_height(rail_end_xz) + 4.6, rail_end_xz.z);
            spawn_stunt_grind_rail(commands, meshes, pal, rail_start, rail_end, feature_seed);

            let spring_xz = rail_start_xz - direction * 17.0;
            let spring = Vec3::new(spring_xz.x, road_height(spring_xz) + 0.75, spring_xz.z);
            spawn_sonic_spring_pad(commands, meshes, pal, spring, direction, 39.0, 17.5, true);

            // Alternate sites add a free-standing vertical trick spring on the
            // opposite lane, creating Tony-Hawk-style transfer opportunities.
            if spawned.is_multiple_of(2) {
                let transfer_xz =
                    rail_center - right * lane_side * SPEED_ROAD_TRAFFIC_LANE_OFFSET * 2.0;
                let transfer = Vec3::new(
                    transfer_xz.x,
                    road_height(transfer_xz) + 0.75,
                    transfer_xz.z,
                );
                spawn_sonic_spring_pad(
                    commands, meshes, pal, transfer, direction, 20.0, 25.0, false,
                );
            }
            spawned += 1;
        }
    }
}

fn spawn_raceway_bowl(
    commands: &mut Commands,
    pal: &Palette,
    unit_cube: &Handle<Mesh>,
    center: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
) {
    let direction = direction.with_y(0.0).normalize_or(Vec3::Z);
    let right = Vec3::new(direction.z, 0.0, -direction.x);
    let yaw = direction.x.atan2(direction.z);
    const STRIPS: usize = 11;
    let strip_width = width / STRIPS as f32;
    for strip in 0..STRIPS {
        let u = (strip as f32 + 0.5) / STRIPS as f32 * 2.0 - 1.0;
        let offset = u * width * 0.5;
        let rise = u.abs().powf(2.15) * 13.0;
        let roll = -u * 0.58;
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
        commands.spawn((
            Name::new("Grand Raceway Bowl Curve"),
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(if strip.is_multiple_of(2) {
                    pal.highway.clone()
                } else {
                    pal.city_metal_panel.clone()
                }),
                transform: Transform::from_translation(
                    center + right * offset + Vec3::Y * (0.58 + rise),
                )
                .with_rotation(rotation)
                .with_scale(Vec3::new(strip_width * 1.16, 0.86, length)),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                strip_width * 0.58,
                0.43,
                length * 0.5,
            ),
            world_space_collider_scale(),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_raceway_vertical_loop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    base: Vec3,
    direction: Vec3,
    seed: u64,
) {
    let direction = direction.with_y(0.0).normalize_or(Vec3::Z);
    let radius = 32.0;
    let loop_width = 46.0;
    let steps = 28;
    let mut previous = base;
    for step in 1..=steps {
        let phi = step as f32 / steps as f32 * TAU;
        let point =
            base + direction * (radius * phi.sin()) + Vec3::Y * (radius * (1.0 - phi.cos()));
        spawn_banked_deck_segment(
            commands, pal, deck_mesh, previous, point, loop_width, 0.0, false,
        );
        previous = point;
    }
    commands.spawn((
        Name::new("Grand Raceway Full Loop Guide"),
        Transform::from_translation(base),
        GlobalTransform::default(),
        SpeedLoopGuide {
            radius,
            yaw: direction.x.atan2(direction.z),
            entry_speed: 0.78,
            lane_half_width: loop_width * 0.5,
        },
        WorldGeometry,
    ));
    spawn_board_boost_pad(
        commands,
        meshes,
        pal,
        base + Vec3::Y * 0.52,
        direction.x.atan2(direction.z),
        0.0,
        direction,
        loop_width * 0.72,
        radius * 0.82,
        3.65 + seeded(seed, 9) * 0.25,
        4.5,
        0.0,
        2.2,
    );
}

pub(super) fn spawn_race_region_features(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
    road_profiles: &[Vec<Vec3>],
) {
    let Some(profile) = road_profiles.get(RACE_CIRCUIT_ROUTE_INDEX) else {
        return;
    };
    let skate_deck_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    for point in RACE_REGION_TRAVEL_POINTS {
        let y = road_profile_height_at(profile, point.x, point.z)
            .unwrap_or_else(|| terrain_surface_y(point.x, point.z, terrain_seed) + 1.0);
        let position = Vec3::new(point.x, y + 1.3, point.z);
        spawn_world_anchor(commands, point.anchor_id, position);
        commands.spawn((
            Name::new(point.label),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(7.5, 0.42))),
                material: MeshMaterial3d(pal.boost_lane.clone()),
                transform: Transform::from_translation(position - Vec3::Y * 1.05),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cylinder(0.21, 7.5),
        ));
    }

    const GATE_ROUTE_POINTS: [usize; 6] = [0, 2, 4, 5, 7, 9];
    for (gate_index, route_point) in GATE_ROUTE_POINTS.into_iter().enumerate() {
        let &(x, z) = ROUTE_RACE_CIRCUIT
            .get(route_point)
            .unwrap_or(&ROUTE_RACE_CIRCUIT[0]);
        let y = road_profile_height_at(profile, x, z)
            .unwrap_or_else(|| terrain_surface_y(x, z, terrain_seed) + SPEED_ROAD_CLEARANCE);
        let tangent = road_profile_tangent_at(profile, x, z).unwrap_or(Vec3::Z);
        let yaw = tangent.x.atan2(tangent.z);
        commands.spawn((
            Name::new(format!("Grand Raceway Gate {}", gate_index + 1)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus {
                    major_radius: RACE_REGION_ROAD_WIDTH * 0.44,
                    minor_radius: 1.8,
                })),
                material: MeshMaterial3d(if gate_index == 0 {
                    pal.crystal_dragon.clone()
                } else {
                    pal.crystal_aurora.clone()
                }),
                transform: Transform::from_xyz(x, y + RACE_REGION_ROAD_WIDTH * 0.44, z)
                    .with_rotation(
                        Quat::from_rotation_y(yaw)
                            * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    ),
                ..default()
            },
            StuntRaceGate {
                course_id: "starfall_grand_raceway",
                course_label: "Starfall Grand Raceway",
                gate_index: gate_index as u8,
                gate_count: GATE_ROUTE_POINTS.len() as u8,
                radius: RACE_REGION_ROAD_WIDTH * 0.49,
            },
            WorldGeometry,
        ));
    }

    // The circuit's shorter technical spans do not qualify for the trunk-road
    // stunt-ramp pass, so give the terrain course its own four launch lines.
    for (ramp_index, segment_index) in [1usize, 4, 7, 9].into_iter().enumerate() {
        let pair = [
            ROUTE_RACE_CIRCUIT[segment_index],
            ROUTE_RACE_CIRCUIT[segment_index + 1],
        ];
        let authored_start = Vec2::new(pair[0].0, pair[0].1);
        let authored_end = Vec2::new(pair[1].0, pair[1].1);
        let xz = authored_start.lerp(authored_end, 0.52);
        let authored_direction = (authored_end - authored_start).normalize_or_zero();
        let mut direction = road_profile_tangent_at(profile, xz.x, xz.y).unwrap_or(Vec3::new(
            authored_direction.x,
            0.0,
            authored_direction.y,
        ));
        if direction
            .with_y(0.0)
            .dot(Vec3::new(authored_direction.x, 0.0, authored_direction.y))
            < 0.0
        {
            direction = -direction;
        }
        let right = Vec2::new(direction.z, -direction.x).normalize_or_zero();
        let lane_offset = if ramp_index.is_multiple_of(2) {
            -96.0
        } else {
            96.0
        };
        let ramp_xz = xz + right * lane_offset;
        let y = road_profile_height_at(profile, xz.x, xz.y)
            .unwrap_or_else(|| terrain_surface_y(xz.x, xz.y, terrain_seed) + 1.0);
        spawn_board_boost_ramp(
            commands,
            meshes,
            pal,
            Vec3::new(ramp_xz.x, y + 0.55, ramp_xz.y),
            direction,
            44.0,
            62.0 + ramp_index as f32 * 5.0,
            8.0 + ramp_index as f32 * 1.4 + seeded(seed, ramp_index as u64 * 31) * 1.2,
            8.0,
            3.1 + ramp_index as f32 * 0.35,
        );
    }

    // Dedicated skate-park lines occupy the huge non-boost shoulders. These
    // are intentionally optional: each side retains a clean racing lane.
    for (feature_index, (segment_index, t, side)) in [(2usize, 0.46_f32, -1.0_f32), (7, 0.54, 1.0)]
        .into_iter()
        .enumerate()
    {
        let a = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index]);
        let b = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index + 1]);
        let center_xz = a.lerp(b, t);
        let direction = road_profile_tangent_at(profile, center_xz.x, center_xz.y)
            .unwrap_or(Vec3::new((b - a).x, 0.0, (b - a).y).normalize_or_zero());
        let right = Vec3::new(direction.z, 0.0, -direction.x).normalize_or_zero();
        let y = road_profile_height_at(profile, center_xz.x, center_xz.y)
            .unwrap_or_else(|| terrain_surface_y(center_xz.x, center_xz.y, terrain_seed) + 1.0);
        spawn_raceway_bowl(
            commands,
            pal,
            &skate_deck_mesh,
            Vec3::new(center_xz.x, y, center_xz.y) + right * side * 104.0,
            direction,
            142.0,
            132.0 + feature_index as f32 * 18.0,
        );
    }

    for (rail_index, segment_index) in [0usize, 2, 4, 6, 8, 10].into_iter().enumerate() {
        let a = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index]);
        let b = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index + 1]);
        let center_xz = a.lerp(b, 0.66);
        let direction = road_profile_tangent_at(profile, center_xz.x, center_xz.y)
            .unwrap_or(Vec3::new((b - a).x, 0.0, (b - a).y).normalize_or_zero());
        let right = Vec3::new(direction.z, 0.0, -direction.x).normalize_or_zero();
        let side = if rail_index.is_multiple_of(2) {
            -1.0
        } else {
            1.0
        };
        let y = road_profile_height_at(profile, center_xz.x, center_xz.y)
            .unwrap_or_else(|| terrain_surface_y(center_xz.x, center_xz.y, terrain_seed) + 1.0);
        let center = Vec3::new(center_xz.x, y + 4.2, center_xz.y)
            + right * side * (68.0 + rail_index as f32 * 7.0);
        let half_length = 68.0 + rail_index as f32 * 5.0;
        spawn_stunt_grind_rail(
            commands,
            meshes,
            pal,
            center - direction * half_length,
            center + direction * half_length + Vec3::Y * 2.5,
            seed + 4_000 + rail_index as u64 * 97,
        );
    }

    for (loop_index, (segment_index, side)) in [(3usize, -1.0_f32), (8usize, 1.0_f32)]
        .into_iter()
        .enumerate()
    {
        let a = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index]);
        let b = Vec2::from(ROUTE_RACE_CIRCUIT[segment_index + 1]);
        let center_xz = a.lerp(b, 0.34);
        let direction = road_profile_tangent_at(profile, center_xz.x, center_xz.y)
            .unwrap_or(Vec3::new((b - a).x, 0.0, (b - a).y).normalize_or_zero());
        let right = Vec3::new(direction.z, 0.0, -direction.x).normalize_or_zero();
        let y = road_profile_height_at(profile, center_xz.x, center_xz.y)
            .unwrap_or_else(|| terrain_surface_y(center_xz.x, center_xz.y, terrain_seed) + 1.0);
        spawn_raceway_vertical_loop(
            commands,
            meshes,
            pal,
            &skate_deck_mesh,
            Vec3::new(center_xz.x, y + 0.55, center_xz.y) + right * side * 112.0,
            direction,
            seed + 6_000 + loop_index as u64 * 211,
        );
    }

    let opponent_path = profile
        .iter()
        .copied()
        .map(|point| point + Vec3::Y * 1.7)
        .collect::<Vec<_>>();
    if opponent_path.len() < 2 {
        return;
    }
    let opponent_mesh = meshes.add(Cuboid::new(5.2, 0.42, 9.4));
    let opponent_body_mesh = meshes.add(Capsule3d::new(1.15, 2.8));
    let opponent_head_mesh = meshes.add(Sphere::new(1.2));
    let lane_offsets = [-152.0_f32, -104.0, -56.0, 56.0, 104.0, 152.0];
    for (racer_id, lane_offset) in lane_offsets.into_iter().enumerate() {
        let vehicle = NpcRoadVehicle {
            path: opponent_path.clone(),
            segment: racer_id * (opponent_path.len() / lane_offsets.len()).max(1),
            progress: 0.0,
            speed: 62.0 + racer_id as f32 * 4.5,
            lane_offset,
            hit_radius: 4.2,
            wreck_timer: 4.0,
        };
        let (position, yaw) = road_vehicle_pose(&vehicle).unwrap_or((opponent_path[0], 0.0));
        commands
            .spawn((
                Name::new(format!("Enemy Hoverboard Surfer {}", racer_id + 1)),
                PbrBundle {
                    mesh: Mesh3d(opponent_mesh.clone()),
                    material: MeshMaterial3d(if racer_id.is_multiple_of(2) {
                        pal.city_metal_panel.clone()
                    } else {
                        pal.crystal_emerald.clone()
                    }),
                    transform: Transform::from_translation(position)
                        .with_rotation(Quat::from_rotation_y(yaw)),
                    ..default()
                },
                vehicle,
                StuntRaceOpponent {
                    racer_id: racer_id as u8,
                    course_id: "starfall_grand_raceway",
                },
                EnemyHoverboardSurfer,
                WorldGeometry,
            ))
            .with_children(|surfer| {
                surfer.spawn((PbrBundle {
                    mesh: Mesh3d(opponent_body_mesh.clone()),
                    material: MeshMaterial3d(pal.crystal_dragon.clone()),
                    transform: Transform::from_xyz(0.0, 2.3, 0.1)
                        .with_rotation(Quat::from_rotation_z(-0.18)),
                    ..default()
                },));
                surfer.spawn((PbrBundle {
                    mesh: Mesh3d(opponent_head_mesh.clone()),
                    material: MeshMaterial3d(pal.crystal_aurora.clone()),
                    transform: Transform::from_xyz(0.0, 4.4, -0.1),
                    ..default()
                },));
            });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_mountain_wrap_ramps(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    seed: u64,
    terrain_seed: u64,
) {
    let mut spawned = 0usize;
    for (ri, route) in mountain_routes().iter().enumerate() {
        for vertex in route
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .take(route.len().saturating_sub(2))
        {
            if spawned >= 8 {
                return;
            }
            let (vi, (x, z)) = vertex;
            if (ri + vi) % 2 != 0 || terrain_surface_y(x, z, terrain_seed) < 90.0 {
                continue;
            }

            let radius = 88.0 + seeded(seed, ri as u64 * 211 + vi as u64 * 17) * 46.0;
            let height = 28.0 + seeded(seed, ri as u64 * 227 + vi as u64 * 19) * 28.0;
            let start_angle = seeded(seed, ri as u64 * 239 + vi as u64 * 23) * TAU;
            spawn_helical_speed_ramp(
                commands,
                meshes,
                pal,
                deck_mesh,
                Vec2::new(x, z),
                radius,
                SPEED_ROAD_WIDTH * 0.82,
                1.08,
                height * 1.28,
                start_angle,
                seed + ri as u64 * 997 + vi as u64 * 71,
                terrain_seed,
            );
            spawned += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_helical_speed_ramp(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    center: Vec2,
    radius: f32,
    width: f32,
    turns: f32,
    height: f32,
    start_angle: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let steps = (turns * 14.0).ceil().clamp(8.0, 18.0) as usize;
    let angle_span = turns * TAU;
    for step in 0..steps {
        let t0 = step as f32 / steps as f32;
        let t1 = (step + 1) as f32 / steps as f32;
        let a0 = start_angle + angle_span * t0;
        let a1 = start_angle + angle_span * t1;
        let point = |angle: f32, t: f32| {
            let x = center.x + angle.cos() * radius;
            let z = center.y + angle.sin() * radius;
            let y = terrain_surface_y(x, z, terrain_seed) + SPEED_ROAD_CLEARANCE + 1.4 + height * t;
            Vec3::new(x, y, z)
        };

        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            point(a0, t0),
            point(a1, t1),
            width,
            true,
            seed + step as u64 * 29,
            terrain_seed,
            true,
        );
    }
}

pub(super) fn spawn_speed_loop_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
    radius: f32,
    width: f32,
    seed: u64,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let yaw_rot = Quat::from_rotation_y(yaw);
    let forward = yaw_rot * Vec3::Z;
    let segments = 18usize;
    let arc_len = TAU * radius / segments as f32 * 1.07;
    let segment_width = (width * 0.36).clamp(7.0, 10.0);

    commands.spawn((
        Name::new("Speed Loop Grapple Apex"),
        SpatialBundle {
            transform: Transform::from_translation(center + Vec3::Y * (radius * 2.0 + 1.5)),
            ..default()
        },
        GrappleSocket::new(GrappleTargetKind::SwingPoint)
            .with_radius(6.0)
            .with_priority(1.42),
    ));

    for segment in 0..segments {
        let a0 = segment as f32 * TAU / segments as f32;
        let a1 = (segment + 1) as f32 * TAU / segments as f32;
        let angle = (a0 + a1) * 0.5 - std::f32::consts::FRAC_PI_2;
        let local = Vec3::new(0.0, radius + angle.sin() * radius, angle.cos() * radius);
        let pos = center + yaw_rot * local;
        let rot = yaw_rot * Quat::from_rotation_x(angle + std::f32::consts::FRAC_PI_2);
        let transform = Transform::from_translation(pos)
            .with_rotation(rot)
            .with_scale(Vec3::new(segment_width, 0.62, arc_len));

        if angle.sin() < -0.20 {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.boost_ramp.clone()),
                    transform,
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    segment_width * 0.5,
                    0.31,
                    arc_len * 0.5,
                ),
                world_space_collider_scale(),
            ));
        } else {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(if segment % 2 == 0 {
                        pal.boost_lane.clone()
                    } else {
                        pal.guide_glow.clone()
                    }),
                    transform,
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    spawn_board_boost_pad(
        commands,
        meshes,
        pal,
        center + Vec3::Y * 0.34,
        yaw,
        0.0,
        forward,
        width * 0.54,
        radius * 1.45,
        3.35,
        5.0,
        0.75,
        1.65,
    );

    for end in [-1.0_f32, 1.0] {
        let ramp_dir = if end > 0.0 { forward } else { -forward };
        spawn_board_boost_ramp(
            commands,
            meshes,
            pal,
            center + forward * end * radius * 1.16 + Vec3::Y * 0.80,
            ramp_dir,
            width * 0.48,
            radius * 1.05,
            7.0 + seeded(seed, end.to_bits() as u64) * 2.0,
            5.2,
            1.15,
        );
    }
}

pub(super) fn spawn_npc_road_vehicles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    road_profiles: &[Vec<Vec3>],
) {
    let vehicle_mesh = meshes.add(Cuboid::new(8.2, 2.5, 13.2));
    let mut spawned = 0usize;

    for (ri, route) in mountain_routes().iter().enumerate() {
        if route.len() < 2 {
            continue;
        }
        let path = road_vehicle_path_from_profile(&road_profiles[ri]);
        if path.len() < 2 {
            continue;
        }

        for lane in [-1.0_f32, 1.0] {
            if spawned >= SPEED_ROAD_PATROL_VEHICLE_COUNT {
                return;
            }
            let speed = 34.0 + seeded(seed, spawned as u64 * 13 + 1) * 18.0;
            let initial_distance = seeded(seed, spawned as u64 * 13 + 2) * 1_800.0;
            let lane_offset = lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET;
            let mut vehicle = NpcRoadVehicle {
                path: path.clone(),
                segment: spawned % path.len(),
                progress: 0.0,
                speed,
                lane_offset,
                hit_radius: 7.5,
                wreck_timer: 5.0,
            };
            advance_road_vehicle(&mut vehicle, initial_distance / speed.max(0.01));
            let (position, yaw) =
                road_vehicle_pose(&vehicle).unwrap_or((path[0], route_heading_yaw(route)));
            let route_dir = Quat::from_rotation_y(yaw) * Vec3::Z;
            let right = Vec3::new(route_dir.z, 0.0, -route_dir.x).normalize_or_zero();
            let color_mat = match (ri + spawned) % 4 {
                0 => pal.brushed_metal.clone(),
                1 => pal.industrial_metal.clone(),
                2 => pal.city_metal_panel.clone(),
                _ => pal.crystal_aurora.clone(),
            };

            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(vehicle_mesh.clone()),
                    material: MeshMaterial3d(color_mat),
                    transform: Transform::from_translation(position + right * lane_offset)
                        .with_rotation(Quat::from_rotation_y(yaw)),
                    ..default()
                },
                WorldGeometry,
                vehicle,
                Health::new(85.0),
                Damageable::default(),
                crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
                crate::engine::physics::prelude::Collider::cuboid(4.1, 1.25, 6.6),
                crate::engine::physics::prelude::CollisionProfile::VehicleHurtbox,
            ));
            spawned += 1;
        }
    }
}

#[allow(dead_code)]
pub(super) fn road_vehicle_route_path(route: &[(f32, f32)], terrain_seed: u64) -> Vec<Vec3> {
    let forward = speed_road_route_profile(
        route,
        SPEED_ROAD_WIDTH,
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 2.0,
    );
    road_vehicle_path_from_profile(&forward)
}

pub(super) fn road_vehicle_path_from_profile(profile: &[Vec3]) -> Vec<Vec3> {
    let ride_height = Vec3::Y * 1.8;
    let mut path: Vec<Vec3> = profile.iter().map(|point| *point + ride_height).collect();
    for point in profile.iter().rev().map(|point| *point + ride_height) {
        push_speed_road_path_point(&mut path, point);
    }
    path
}

pub(super) fn push_speed_road_path_point(path: &mut Vec<Vec3>, point: Vec3) {
    if let Some(last) = path.last_mut() {
        let same_xz = Vec2::new(last.x - point.x, last.z - point.z).length_squared() < 0.01;
        if same_xz {
            last.y = last.y.max(point.y);
            return;
        }
    }
    path.push(point);
}

pub(super) fn route_heading_yaw(route: &[(f32, f32)]) -> f32 {
    if route.len() < 2 {
        return 0.0;
    }
    let (ax, az) = route[0];
    let (bx, bz) = route[1];
    (bx - ax).atan2(bz - az)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected} +/- {tolerance}, got {actual}"
        );
    }

    #[test]
    fn star_city_ring_keeps_heavy_water_parameters_and_section_count() {
        assert_eq!(STAR_CITY_RING_SEGMENTS, 56);
        assert_near(STAR_CITY_RING_RADIUS, 280.0, f32::EPSILON);
        assert_near(STAR_CITY_RING_HEIGHT, 80.0, f32::EPSILON);

        let points = star_city_ring_points();
        assert_eq!(points.len(), STAR_CITY_RING_SEGMENTS + 1);
        assert!(points[0].distance(points[STAR_CITY_RING_SEGMENTS]) < 0.001);
        assert!(points.iter().all(|point| {
            (Vec2::new(point.x, point.z).length() - STAR_CITY_RING_RADIUS).abs() < 0.001
                && (point.y - STAR_CITY_RING_HEIGHT).abs() < f32::EPSILON
        }));
    }

    #[test]
    fn star_city_ring_chords_are_uniform_and_continuous() {
        let points = star_city_ring_points();
        let expected_chord = 2.0
            * STAR_CITY_RING_RADIUS
            * (std::f32::consts::PI / STAR_CITY_RING_SEGMENTS as f32).sin();
        for pair in points.windows(2) {
            assert_near(pair[0].distance(pair[1]), expected_chord, 0.001);
        }

        // The closed point list gives each section exactly one shared endpoint
        // with its successor, including the last-to-first seam.
        for segment in 0..STAR_CITY_RING_SEGMENTS {
            let next_segment = (segment + 1) % STAR_CITY_RING_SEGMENTS;
            let end = points[segment + 1];
            let next_start = if next_segment == 0 {
                points[0]
            } else {
                points[next_segment]
            };
            assert!(end.distance(next_start) < 0.001);
        }
    }

    #[test]
    fn star_city_ring_uses_eight_sparse_boost_stations_away_from_merges() {
        let stations: Vec<_> = (0..STAR_CITY_RING_SEGMENTS)
            .filter(|segment| star_city_ring_has_boost_station(*segment))
            .collect();
        assert_eq!(stations.len(), 8);

        let segments_per_quadrant = STAR_CITY_RING_SEGMENTS / STAR_CITY_RING_RAMP_COUNT;
        assert!(stations
            .iter()
            .all(|segment| !segment.is_multiple_of(segments_per_quadrant)));
    }

    #[test]
    fn star_city_ring_has_four_cardinal_mid_chord_access_ramps() {
        let points = star_city_ring_points();
        let ramps = star_city_ring_ramp_plans();
        assert_eq!(ramps.len(), STAR_CITY_RING_RAMP_COUNT);
        assert_eq!(STAR_CITY_RING_RAMP_SEGMENTS, 24);

        let segments_per_quadrant = STAR_CITY_RING_SEGMENTS / STAR_CITY_RING_RAMP_COUNT;
        for (index, ramp) in ramps.iter().enumerate() {
            let ring_segment = index * segments_per_quadrant;
            let chord_midpoint = Vec2::new(
                (points[ring_segment].x + points[ring_segment + 1].x) * 0.5,
                (points[ring_segment].z + points[ring_segment + 1].z) * 0.5,
            );
            let angle = index as f32 * TAU / STAR_CITY_RING_RAMP_COUNT as f32;
            let cardinal = Vec2::new(angle.cos(), angle.sin());
            let expected_outer_lane = chord_midpoint + cardinal * STAR_CITY_RING_LANE_OFFSET;
            assert!(ramp.merge.distance(expected_outer_lane) < 0.001);
            assert!(
                ramp.merge.distance(chord_midpoint) > STAR_CITY_RING_DIVIDER_WIDTH * 0.5 + 2.0,
                "ramp endpoint must clear the ring's center divider"
            );
            assert!(ramp.ground_mouth.normalize().dot(cardinal) > 0.9999);
            assert_near(
                ramp.ground_mouth.length(),
                ramp.merge.length() + STAR_CITY_RING_RAMP_RUN,
                0.001,
            );
        }
    }

    #[test]
    fn star_city_ring_ramp_mouths_are_driveable_from_terrain() {
        let terrain_seed = 0x5a17_c170_u64;
        for ramp in star_city_ring_ramp_plans() {
            let points = star_city_ring_ramp_points(ramp, terrain_seed);
            let mouth = points.first().expect("ramp has a ground mouth");
            let ground = terrain_surface_y(mouth.x, mouth.z, terrain_seed);

            assert_near(mouth.y, ground + STAR_CITY_RING_GROUND_EMBED, f32::EPSILON);
            assert_eq!(points.len(), STAR_CITY_RING_RAMP_SEGMENTS + 1);
        }
    }

    #[test]
    fn star_city_ring_and_ramps_participate_in_world_keepout_distance() {
        assert!(distance_to_star_city_ring_network(STAR_CITY_RING_RADIUS, 0.0) < 0.001);
        for ramp in star_city_ring_ramp_plans() {
            let midpoint = ramp.merge.lerp(ramp.ground_mouth, 0.5);
            assert!(distance_to_star_city_ring_network(midpoint.x, midpoint.y) < 0.001);
        }
        assert!(distance_to_star_city_ring_network(0.0, 0.0) > 270.0);
    }

    #[test]
    fn star_city_keepout_preserves_compact_ring_and_ramp_widths() {
        assert_near(
            star_city_equivalent_speed_road_distance(STAR_CITY_RING_RADIUS, 0.0),
            SPEED_ROAD_WIDTH * 0.5 - STAR_CITY_RING_WIDTH * 0.5,
            0.001,
        );

        let ramp = star_city_ring_ramp_plans()[0];
        let midpoint = ramp.merge.lerp(ramp.ground_mouth, 0.5);
        assert_near(
            star_city_equivalent_speed_road_distance(midpoint.x, midpoint.y),
            SPEED_ROAD_WIDTH * 0.5 - STAR_CITY_RING_RAMP_WIDTH * 0.5,
            0.001,
        );
    }
}
