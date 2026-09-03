//! Reusable level chunks for the shared-screen platformer.
//!
//! A **chunk** is the unit designers actually think in: a stairwell flight, a
//! castle rampart, a rock spire, a rooftop block. It sits above single-object
//! prefab intent and below a level (an ordered route), replacing hand-placed
//! runtime coordinates with data that can be remixed and validated.
//!
//! # Sockets do the layout
//!
//! Every chunk declares an **entry** and an **exit** socket in chunk-local
//! space. Assembling a route translates each chunk so its entry lands exactly
//! on the previous chunk's exit, which means a designer chooses *which chunks
//! and in what order* — never coordinates. A chunk whose exit sits above its
//! entry climbs, so a mountain ascent is just a run of rising chunks, and the
//! route's total height falls out of the arithmetic.
//!
//! # Chunks are self-contained lessons
//!
//! Gaps live *inside* chunks, never between them: sockets always meet flush.
//! That keeps every jump the responsibility of the chunk that authored it, so
//! [`ChunkDef::validate`] can check each gap against a declared movement
//! envelope — a chunk that ships an unjumpable gap fails the build rather than
//! the playtest.

use bevy_math::Vec3;
use std::fmt;

/// Stable gameplay intent shared by Forge recipes, chunk documents, and
/// runtime adapters. Rendering remains the consumer's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformerPrefabKind {
    Ladder,
    Stairs,
    Tower,
    Castle,
    FloatingIsland,
    MovingPlatform,
    SpringPlatform,
    FlipPanel,
    RotatingBridge,
    CollapseBridge,
    SpikeBridge,
    /// Native extension point for game-defined prefab adapters.
    Custom(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformerBehavior {
    StaticTraversal,
    Climbable,
    Oscillate { distance: f32, seconds: f32 },
    Bounce { launch_speed: f32 },
    FlipOnJump,
    Rotate { degrees_per_second: f32 },
    Collapse { warning_seconds: f32 },
    Hazard,
}

impl PlatformerPrefabKind {
    pub const ALL: [Self; 11] = [
        Self::Ladder,
        Self::Stairs,
        Self::Tower,
        Self::Castle,
        Self::FloatingIsland,
        Self::MovingPlatform,
        Self::SpringPlatform,
        Self::FlipPanel,
        Self::RotatingBridge,
        Self::CollapseBridge,
        Self::SpikeBridge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ladder => "Ladder",
            Self::Stairs => "Stairs",
            Self::Tower => "Star Tower",
            Self::Castle => "Star Castle",
            Self::FloatingIsland => "Floating Island",
            Self::MovingPlatform => "Moving Platform",
            Self::SpringPlatform => "Pulse Spring",
            Self::FlipPanel => "Flip Panel",
            Self::RotatingBridge => "Rotating Bridge",
            Self::CollapseBridge => "Collapse Bridge",
            Self::SpikeBridge => "Spike Bridge",
            Self::Custom(id) => id,
        }
    }

    pub fn behavior(self) -> PlatformerBehavior {
        match self {
            Self::Ladder => PlatformerBehavior::Climbable,
            Self::MovingPlatform => PlatformerBehavior::Oscillate {
                distance: 6.0,
                seconds: 3.0,
            },
            Self::SpringPlatform => PlatformerBehavior::Bounce { launch_speed: 1.1 },
            Self::FlipPanel => PlatformerBehavior::FlipOnJump,
            Self::RotatingBridge => PlatformerBehavior::Rotate {
                degrees_per_second: 42.0,
            },
            Self::CollapseBridge => PlatformerBehavior::Collapse {
                warning_seconds: 0.65,
            },
            Self::SpikeBridge => PlatformerBehavior::Hazard,
            Self::Stairs | Self::Tower | Self::Castle | Self::FloatingIsland => {
                PlatformerBehavior::StaticTraversal
            }
            Self::Custom(_) => PlatformerBehavior::StaticTraversal,
        }
    }
}

/// Visual and structural family. Themes are the "reusable chunks of cityscape,
/// mountains, castles, rock formations" vocabulary: a route normally stays in
/// one theme, or transitions deliberately (city → mountain → castle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkTheme {
    /// Rooftops, ledges, and signage of the star city.
    Cityscape,
    /// Himalayan rock: ledges, spires, and windswept shelves.
    Mountain,
    /// Worked stone: ramparts, stairwells, and halls.
    Castle,
    /// Underground rock formations for dungeon and cave routes.
    Cavern,
    /// Game-defined theme identity. Materials remain a consumer concern.
    Custom(&'static str),
}

impl ChunkTheme {
    /// Every theme, for the chunk palette (see `chunks_for_theme`).
    #[allow(dead_code)]
    pub const ALL: [ChunkTheme; 4] = [
        ChunkTheme::Cityscape,
        ChunkTheme::Mountain,
        ChunkTheme::Castle,
        ChunkTheme::Cavern,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChunkTheme::Cityscape => "Cityscape",
            ChunkTheme::Mountain => "Mountain",
            ChunkTheme::Castle => "Castle",
            ChunkTheme::Cavern => "Cavern",
            ChunkTheme::Custom(id) => id,
        }
    }
}

/// What a chunk is *for*, in progression terms. Roles let a route be read at a
/// glance and let validation hold each kind to its own promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkRole {
    /// Where the party lands: flat, safe, wide enough for four.
    Arrival,
    /// Horizontal movement and jump lessons; must not climb.
    Traverse,
    /// Gains height — stairs, ledges, ladders.
    Ascent,
    /// A combat pocket: flat, enclosed, room to fight.
    Arena,
    /// The boss room at the top of the climb.
    Boss,
}

impl ChunkRole {
    pub fn label(self) -> &'static str {
        match self {
            ChunkRole::Arrival => "Arrival",
            ChunkRole::Traverse => "Traverse",
            ChunkRole::Ascent => "Ascent",
            ChunkRole::Arena => "Arena",
            ChunkRole::Boss => "Boss",
        }
    }

    /// Only ascent chunks may end higher than they began. Keeping traverse and
    /// arena chunks flat is what makes a route's height budget predictable.
    pub fn may_climb(self) -> bool {
        matches!(self, ChunkRole::Ascent)
    }

    /// Fighting rooms need standing room for four players plus enemies.
    pub fn needs_combat_room(self) -> bool {
        matches!(self, ChunkRole::Arena | ChunkRole::Boss)
    }
}

/// One buildable element inside a chunk, in chunk-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChunkPiece {
    /// A solid box of walkable geometry: `center` and full `size`.
    Solid { center: Vec3, size: Vec3 },
    /// An accent box — trim, rails, signage. Non-structural, still solid.
    Trim { center: Vec3, size: Vec3 },
    /// Structural mass that is *not* footing: backdrop rock, enclosing walls,
    /// cave ceilings. Collides like anything else, but the route walker must
    /// ignore it — otherwise a cliff face behind a shelf reads as a thirteen
    /// metre step the party is expected to jump.
    Scenery { center: Vec3, size: Vec3 },
    /// A prefab instance (ladder, spring, moving platform, …) placed by the
    /// existing prefab system rather than re-modelled here.
    Prefab {
        kind: PlatformerPrefabKind,
        center: Vec3,
        yaw_degrees: f32,
    },
}

impl ChunkPiece {
    pub fn center(&self) -> Vec3 {
        match self {
            ChunkPiece::Solid { center, .. }
            | ChunkPiece::Trim { center, .. }
            | ChunkPiece::Scenery { center, .. }
            | ChunkPiece::Prefab { center, .. } => *center,
        }
    }

    /// Whether this piece is part of the walking route — used by gap
    /// analysis. Trim is decorative, scenery is mass rather than footing, and
    /// prefabs carry their own behaviour, so only solids count as guaranteed
    /// landing surfaces.
    pub fn is_landing_surface(&self) -> bool {
        matches!(self, ChunkPiece::Solid { .. })
    }

    /// Top surface height, for reachability checks.
    pub fn top_y(&self) -> f32 {
        match self {
            ChunkPiece::Solid { center, size }
            | ChunkPiece::Trim { center, size }
            | ChunkPiece::Scenery { center, size } => center.y + size.y * 0.5,
            ChunkPiece::Prefab { center, .. } => center.y,
        }
    }

    /// Horizontal span along the route axis as `(min_x, max_x)`.
    pub fn x_span(&self) -> (f32, f32) {
        match self {
            ChunkPiece::Solid { center, size }
            | ChunkPiece::Trim { center, size }
            | ChunkPiece::Scenery { center, size } => {
                (center.x - size.x * 0.5, center.x + size.x * 0.5)
            }
            // A prefab is treated as a point for spacing purposes; its own
            // recipe owns its true footprint.
            ChunkPiece::Prefab { center, .. } => (center.x, center.x),
        }
    }
}

/// A connection point on a chunk's boundary, in chunk-local space.
///
/// Sockets are where the party physically stands as they cross between chunks,
/// so their height is the floor height at that edge — not the chunk origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkSocket {
    pub offset: Vec3,
}

impl ChunkSocket {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            offset: Vec3::new(x, y, z),
        }
    }
}

/// A reusable level chunk.
#[derive(Debug, Clone)]
pub struct ChunkDef {
    pub id: &'static str,
    pub label: &'static str,
    pub theme: ChunkTheme,
    pub role: ChunkRole,
    /// Where the party enters, in chunk-local space.
    pub entry: ChunkSocket,
    /// Where the party leaves. Above `entry` for climbing chunks.
    pub exit: ChunkSocket,
    pub pieces: Vec<ChunkPiece>,
}

/// Minimal movement inputs needed to validate authored platformer geometry.
/// Values use Starfall's per-frame-at-60-Hz controller convention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementProfile {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
    pub fall_gravity_mult: f32,
}

impl Default for MovementProfile {
    fn default() -> Self {
        Self {
            walk_speed: 0.38,
            sprint_speed: 0.70,
            jump_force: 0.64,
            gravity: 0.024,
            fall_gravity_mult: 1.35,
        }
    }
}

/// The reachable jump envelope derived from a declared movement profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpEnvelope {
    /// Peak height above the take-off surface.
    pub rise: f32,
    /// Horizontal distance cleared in a full running jump.
    pub run: f32,
}

impl JumpEnvelope {
    /// Movement velocities are per-frame values integrated as `v * dt * 60`,
    /// so a per-frame figure times 60 is units per second.
    pub fn from_profile(profile: MovementProfile) -> Self {
        let launch = profile.jump_force * 60.0;
        let gravity = profile.gravity * 60.0 * 60.0;
        if gravity <= f32::EPSILON {
            return Self {
                rise: f32::INFINITY,
                run: f32::INFINITY,
            };
        }
        let rise = launch * launch / (2.0 * gravity);
        let time_up = launch / gravity;
        // Falling is faster than rising, so the airborne time is not simply
        // double the ascent.
        let fall_gravity = gravity * profile.fall_gravity_mult.max(0.01);
        let time_down = (2.0 * rise / fall_gravity).sqrt();
        let speed = profile.walk_speed.max(profile.sprint_speed) * 60.0;
        Self {
            rise,
            run: speed * (time_up + time_down),
        }
    }

    /// Shipped tuning, for tests and tools without a spawned player.
    pub fn standard() -> Self {
        Self::from_profile(MovementProfile::default())
    }

    /// Safety margin: authored gaps must sit inside this fraction of the true
    /// envelope so a jump is comfortable rather than frame-perfect.
    pub const COMFORT: f32 = 0.7;

    pub fn comfortable_run(&self) -> f32 {
        self.run * Self::COMFORT
    }

    pub fn comfortable_rise(&self) -> f32 {
        self.rise * Self::COMFORT
    }
}

/// Why a chunk failed validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkProblem {
    NoPieces,
    /// A traverse or arena chunk that changes height.
    UnexpectedClimb {
        rise: f32,
    },
    /// A gap wider than a comfortable running jump.
    GapTooWide {
        after_x: f32,
        gap: f32,
        limit: f32,
    },
    /// A step up taller than a comfortable jump.
    StepTooHigh {
        at_x: f32,
        step: f32,
        limit: f32,
    },
    /// A combat room without space to fight.
    ArenaTooSmall {
        width: f32,
    },
    /// A socket that does not sit on the chunk's own geometry.
    SocketAdrift {
        entry: bool,
    },
}

impl ChunkProblem {
    pub fn message(&self) -> String {
        match self {
            ChunkProblem::NoPieces => "chunk has no geometry".to_string(),
            ChunkProblem::UnexpectedClimb { rise } => {
                format!("{rise:.1}m of climb in a chunk whose role must stay level")
            }
            ChunkProblem::GapTooWide {
                after_x,
                gap,
                limit,
            } => format!(
                "gap of {gap:.1}m after x={after_x:.1} exceeds the comfortable jump ({limit:.1}m)"
            ),
            ChunkProblem::StepTooHigh { at_x, step, limit } => format!(
                "step of {step:.1}m at x={at_x:.1} exceeds a comfortable jump ({limit:.1}m)"
            ),
            ChunkProblem::ArenaTooSmall { width } => {
                format!("combat room is only {width:.1}m wide; four players need room")
            }
            ChunkProblem::SocketAdrift { entry } => format!(
                "{} socket is not on the chunk's floor",
                if *entry { "entry" } else { "exit" }
            ),
        }
    }
}

impl fmt::Display for ChunkProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for ChunkProblem {}

/// Minimum width for a room the party is expected to fight in.
const MIN_ARENA_WIDTH: f32 = 18.0;
/// How far a socket may sit from the nearest floor surface before it is adrift.
const SOCKET_TOLERANCE: f32 = 2.5;

impl ChunkDef {
    /// Total height gained from entry to exit.
    pub fn rise(&self) -> f32 {
        self.exit.offset.y - self.entry.offset.y
    }

    /// Route-axis length from entry to exit.
    pub fn length(&self) -> f32 {
        (self.exit.offset.x - self.entry.offset.x).abs()
    }

    /// Landing surfaces sorted along the route axis.
    fn landing_surfaces(&self) -> Vec<&ChunkPiece> {
        let mut surfaces: Vec<&ChunkPiece> = self
            .pieces
            .iter()
            .filter(|piece| piece.is_landing_surface())
            .collect();
        surfaces.sort_by(|a, b| a.x_span().0.total_cmp(&b.x_span().0));
        surfaces
    }

    /// Check the chunk against its role and the player's real jump envelope.
    pub fn validate(&self, envelope: JumpEnvelope) -> Vec<ChunkProblem> {
        let mut problems = Vec::new();
        if self.pieces.is_empty() {
            problems.push(ChunkProblem::NoPieces);
            return problems;
        }

        let rise = self.rise();
        if !self.role.may_climb() && rise.abs() > 0.5 {
            problems.push(ChunkProblem::UnexpectedClimb { rise });
        }

        let surfaces = self.landing_surfaces();
        if self.role.needs_combat_room() {
            let width = surfaces
                .iter()
                .map(|piece| {
                    let (min, max) = piece.x_span();
                    max - min
                })
                .fold(0.0_f32, f32::max);
            if width < MIN_ARENA_WIDTH {
                problems.push(ChunkProblem::ArenaTooSmall { width });
            }
        }

        // Walk the surfaces along the route, measuring each gap and step.
        // Prefabs (ladders, springs, moving platforms) are deliberate bridges
        // over otherwise-impossible spans, so a gap they cover is exempt.
        let run_limit = envelope.comfortable_run();
        let rise_limit = envelope.comfortable_rise();
        for pair in surfaces.windows(2) {
            let (_, prev_max) = pair[0].x_span();
            let (next_min, _) = pair[1].x_span();
            let gap = next_min - prev_max;
            let bridged = self.pieces.iter().any(|piece| {
                matches!(piece, ChunkPiece::Prefab { .. })
                    && piece.center().x >= prev_max - 1.0
                    && piece.center().x <= next_min + 1.0
            });
            if gap > run_limit && !bridged {
                problems.push(ChunkProblem::GapTooWide {
                    after_x: prev_max,
                    gap,
                    limit: run_limit,
                });
            }
            let step = pair[1].top_y() - pair[0].top_y();
            if step > rise_limit && !bridged {
                problems.push(ChunkProblem::StepTooHigh {
                    at_x: next_min,
                    step,
                    limit: rise_limit,
                });
            }
        }

        // Sockets must stand on the chunk's own floor, or assembled routes
        // would hand the party off into thin air.
        for (socket, is_entry) in [(self.entry, true), (self.exit, false)] {
            let supported = surfaces.iter().any(|piece| {
                let (min, max) = piece.x_span();
                socket.offset.x >= min - SOCKET_TOLERANCE
                    && socket.offset.x <= max + SOCKET_TOLERANCE
                    && (socket.offset.y - piece.top_y()).abs() <= SOCKET_TOLERANCE
            });
            if !supported {
                problems.push(ChunkProblem::SocketAdrift { entry: is_entry });
            }
        }
        problems
    }
}

/// A chunk positioned in world space by the assembler.
#[derive(Debug, Clone)]
pub struct PlacedChunk {
    pub def: ChunkDef,
    /// World-space offset applied to every local coordinate in the chunk.
    pub origin: Vec3,
}

impl PlacedChunk {
    pub fn world_entry(&self) -> Vec3 {
        self.origin + self.def.entry.offset
    }

    pub fn world_exit(&self) -> Vec3 {
        self.origin + self.def.exit.offset
    }

    /// Pieces translated into world space, ready to spawn.
    pub fn world_pieces(&self) -> Vec<ChunkPiece> {
        self.pieces_translated()
    }

    fn pieces_translated(&self) -> Vec<ChunkPiece> {
        self.def
            .pieces
            .iter()
            .map(|piece| match *piece {
                ChunkPiece::Solid { center, size } => ChunkPiece::Solid {
                    center: center + self.origin,
                    size,
                },
                ChunkPiece::Trim { center, size } => ChunkPiece::Trim {
                    center: center + self.origin,
                    size,
                },
                ChunkPiece::Scenery { center, size } => ChunkPiece::Scenery {
                    center: center + self.origin,
                    size,
                },
                ChunkPiece::Prefab {
                    kind,
                    center,
                    yaw_degrees,
                } => ChunkPiece::Prefab {
                    kind,
                    center: center + self.origin,
                    yaw_degrees,
                },
            })
            .collect()
    }
}

/// Snap a sequence of chunks into a continuous route.
///
/// Each chunk after the first is translated so its **entry socket lands exactly
/// on the previous chunk's exit socket**. That is the whole layout algorithm:
/// designers order chunks, the route computes coordinates, and a rising chunk
/// carries everything after it upward automatically.
pub fn assemble_route(defs: &[ChunkDef], start: Vec3) -> Vec<PlacedChunk> {
    let mut placed: Vec<PlacedChunk> = Vec::with_capacity(defs.len());
    let mut cursor = start;
    for def in defs {
        let origin = cursor - def.entry.offset;
        let chunk = PlacedChunk {
            def: def.clone(),
            origin,
        };
        cursor = chunk.world_exit();
        placed.push(chunk);
    }
    placed
}

/// Total height climbed across an assembled route.
pub fn route_climb(route: &[PlacedChunk]) -> f32 {
    match (route.first(), route.last()) {
        (Some(first), Some(last)) => last.world_exit().y - first.world_entry().y,
        _ => 0.0,
    }
}

/// Renderer-neutral authored route: an ordered sequence of stable chunk IDs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub brief: &'static str,
    pub theme: ChunkTheme,
    pub chunks: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq)]
pub enum RouteProblem {
    Empty,
    UnknownChunk(String),
    DoesNotOpenOnArrival,
    NoDestination,
    BadChunk { id: String, problem: ChunkProblem },
    ArrivalMidRoute { id: String },
}

impl RouteProblem {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "route has no chunks".into(),
            Self::UnknownChunk(id) => format!("no chunk named '{id}' in the catalogue"),
            Self::DoesNotOpenOnArrival => {
                "a route must open on an arrival chunk so the party lands somewhere safe".into()
            }
            Self::NoDestination => {
                "a route must end on an arena or boss chunk so arriving means something".into()
            }
            Self::BadChunk { id, problem } => format!("chunk '{id}': {}", problem.message()),
            Self::ArrivalMidRoute { id } => {
                format!("'{id}' is an arrival in the middle of a route")
            }
        }
    }
}

impl fmt::Display for RouteProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for RouteProblem {}

impl RouteDefinition {
    pub fn resolve_with(
        &self,
        mut resolve: impl FnMut(&str) -> Option<ChunkDef>,
    ) -> Result<Vec<ChunkDef>, RouteProblem> {
        self.chunks
            .iter()
            .map(|id| resolve(id).ok_or_else(|| RouteProblem::UnknownChunk((*id).to_string())))
            .collect()
    }

    pub fn assemble_with(
        &self,
        start: Vec3,
        resolve: impl FnMut(&str) -> Option<ChunkDef>,
    ) -> Result<Vec<PlacedChunk>, RouteProblem> {
        Ok(assemble_route(&self.resolve_with(resolve)?, start))
    }

    pub fn total_climb_with(
        &self,
        resolve: impl FnMut(&str) -> Option<ChunkDef>,
    ) -> Result<f32, RouteProblem> {
        Ok(route_climb(&self.assemble_with(Vec3::ZERO, resolve)?))
    }

    pub fn validate_with(
        &self,
        envelope: JumpEnvelope,
        resolve: impl FnMut(&str) -> Option<ChunkDef>,
    ) -> Vec<RouteProblem> {
        let chunks = match self.resolve_with(resolve) {
            Ok(chunks) => chunks,
            Err(problem) => return vec![problem],
        };
        validate_resolved_route(&chunks, envelope)
    }
}

/// Validates an already resolved route. Graph/document compilers use this
/// entry point after resolving owned serialized IDs through a game catalog.
pub fn validate_resolved_route(chunks: &[ChunkDef], envelope: JumpEnvelope) -> Vec<RouteProblem> {
    if chunks.is_empty() {
        return vec![RouteProblem::Empty];
    }

    let mut problems = Vec::new();
    if chunks[0].role != ChunkRole::Arrival {
        problems.push(RouteProblem::DoesNotOpenOnArrival);
    }
    if !matches!(
        chunks[chunks.len() - 1].role,
        ChunkRole::Arena | ChunkRole::Boss
    ) {
        problems.push(RouteProblem::NoDestination);
    }
    for chunk in chunks.iter().skip(1) {
        if chunk.role == ChunkRole::Arrival {
            problems.push(RouteProblem::ArrivalMidRoute {
                id: chunk.id.to_string(),
            });
        }
    }
    for chunk in chunks {
        for problem in chunk.validate(envelope) {
            problems.push(RouteProblem::BadChunk {
                id: chunk.id.to_string(),
                problem,
            });
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_pad(id: &'static str, role: ChunkRole, width: f32) -> ChunkDef {
        ChunkDef {
            id,
            label: "Test Pad",
            theme: ChunkTheme::Castle,
            role,
            entry: ChunkSocket::new(0.0, 1.0, 0.0),
            exit: ChunkSocket::new(width, 1.0, 0.0),
            pieces: vec![ChunkPiece::Solid {
                center: Vec3::new(width * 0.5, 0.0, 0.0),
                size: Vec3::new(width, 2.0, 16.0),
            }],
        }
    }

    #[test]
    fn the_jump_envelope_comes_from_real_movement_tuning() {
        let envelope = JumpEnvelope::standard();
        // Sanity: a player can clear a few metres up and a useful run across.
        assert!(envelope.rise > 4.0, "rise {}", envelope.rise);
        assert!(
            envelope.run > envelope.rise,
            "a jump goes further than high"
        );

        // Retuning movement retunes validation — the whole point of deriving
        // rather than hardcoding.
        let mut floaty = MovementProfile::default();
        floaty.jump_force *= 2.0;
        assert!(JumpEnvelope::from_profile(floaty).rise > envelope.rise);

        // Comfort margin is a real reduction, so authored gaps keep slack.
        assert!(envelope.comfortable_run() < envelope.run);
    }

    #[test]
    fn assembling_snaps_each_entry_onto_the_previous_exit() {
        let defs = vec![
            flat_pad("a", ChunkRole::Arrival, 20.0),
            flat_pad("b", ChunkRole::Traverse, 30.0),
            flat_pad("c", ChunkRole::Traverse, 10.0),
        ];
        let route = assemble_route(&defs, Vec3::new(5.0, 1.0, 0.0));

        assert_eq!(route.len(), 3);
        assert_eq!(route[0].world_entry(), Vec3::new(5.0, 1.0, 0.0));
        // Every seam is flush: no gaps appear *between* chunks.
        for pair in route.windows(2) {
            assert_eq!(
                pair[0].world_exit(),
                pair[1].world_entry(),
                "chunks must hand off exactly"
            );
        }
        // Length accumulates along the route.
        assert_eq!(route[2].world_exit().x, 5.0 + 20.0 + 30.0 + 10.0);
    }

    #[test]
    fn route_contract_resolves_and_validates_an_external_catalog() {
        let arrival = flat_pad("arrival", ChunkRole::Arrival, 20.0);
        let arena = flat_pad("arena", ChunkRole::Arena, 30.0);
        let catalog = [arrival, arena];
        let route = RouteDefinition {
            id: "example.route",
            label: "Example Route",
            brief: "Cross the test route",
            theme: ChunkTheme::Castle,
            chunks: &["arrival", "arena"],
        };
        let resolve = |id: &str| catalog.iter().find(|chunk| chunk.id == id).cloned();

        assert!(route
            .validate_with(JumpEnvelope::standard(), resolve)
            .is_empty());
        let placed = route
            .assemble_with(Vec3::new(5.0, 1.0, 0.0), resolve)
            .unwrap();
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].world_exit(), placed[1].world_entry());
    }

    #[test]
    fn custom_prefabs_and_themes_are_not_closed_to_stock_content() {
        let prefab = PlatformerPrefabKind::Custom("my_game.wind_lift");
        let theme = ChunkTheme::Custom("my_game.sky_garden");
        assert_eq!(prefab.label(), "my_game.wind_lift");
        assert_eq!(prefab.behavior(), PlatformerBehavior::StaticTraversal);
        assert_eq!(theme.label(), "my_game.sky_garden");
    }

    #[test]
    fn a_run_of_ascent_chunks_carries_the_route_upward() {
        let stair = |id: &'static str| ChunkDef {
            id,
            label: "Flight",
            theme: ChunkTheme::Castle,
            role: ChunkRole::Ascent,
            entry: ChunkSocket::new(0.0, 1.0, 0.0),
            exit: ChunkSocket::new(16.0, 7.0, 0.0),
            pieces: vec![ChunkPiece::Solid {
                center: Vec3::new(8.0, 0.0, 0.0),
                size: Vec3::new(16.0, 2.0, 16.0),
            }],
        };
        let defs = vec![stair("s1"), stair("s2"), stair("s3")];
        let route = assemble_route(&defs, Vec3::ZERO);

        // Three six-metre flights stack into an eighteen-metre climb, and
        // later chunks sit bodily higher in the world.
        assert!((route_climb(&route) - 18.0).abs() < 1e-4);
        assert!(route[2].origin.y > route[0].origin.y);
        assert!(route[2].world_exit().y > route[0].world_exit().y);
    }

    #[test]
    fn world_pieces_are_translated_by_the_placement() {
        let defs = vec![flat_pad("a", ChunkRole::Arrival, 20.0)];
        let route = assemble_route(&defs, Vec3::new(100.0, 5.0, 0.0));
        let piece = route[0].world_pieces()[0];
        // Local (10, 0) with entry at local (0,1) placed at world (100,5).
        assert_eq!(piece.center(), Vec3::new(110.0, 4.0, 0.0));
    }

    #[test]
    fn a_gap_wider_than_a_comfortable_jump_is_rejected() {
        let envelope = JumpEnvelope::standard();
        let limit = envelope.comfortable_run();
        let far = limit + 12.0;
        let chunk = ChunkDef {
            id: "chasm",
            label: "Chasm",
            theme: ChunkTheme::Mountain,
            role: ChunkRole::Traverse,
            entry: ChunkSocket::new(0.0, 1.0, 0.0),
            exit: ChunkSocket::new(20.0 + far, 1.0, 0.0),
            pieces: vec![
                ChunkPiece::Solid {
                    center: Vec3::new(5.0, 0.0, 0.0),
                    size: Vec3::new(10.0, 2.0, 16.0),
                },
                ChunkPiece::Solid {
                    center: Vec3::new(10.0 + far + 5.0, 0.0, 0.0),
                    size: Vec3::new(10.0, 2.0, 16.0),
                },
            ],
        };
        let problems = chunk.validate(envelope);
        assert!(
            problems
                .iter()
                .any(|problem| matches!(problem, ChunkProblem::GapTooWide { .. })),
            "{problems:?}"
        );
    }

    #[test]
    fn a_prefab_bridges_a_gap_that_would_otherwise_fail() {
        let envelope = JumpEnvelope::standard();
        let far = envelope.comfortable_run() + 12.0;
        let mut chunk = ChunkDef {
            id: "bridged",
            label: "Bridged",
            theme: ChunkTheme::Cavern,
            role: ChunkRole::Traverse,
            entry: ChunkSocket::new(0.0, 1.0, 0.0),
            exit: ChunkSocket::new(20.0 + far, 1.0, 0.0),
            pieces: vec![
                ChunkPiece::Solid {
                    center: Vec3::new(5.0, 0.0, 0.0),
                    size: Vec3::new(10.0, 2.0, 16.0),
                },
                ChunkPiece::Solid {
                    center: Vec3::new(10.0 + far + 5.0, 0.0, 0.0),
                    size: Vec3::new(10.0, 2.0, 16.0),
                },
            ],
        };
        assert!(
            !chunk.validate(envelope).is_empty(),
            "unbridged should fail"
        );

        // A moving platform spanning the gap makes it a designed crossing.
        chunk.pieces.push(ChunkPiece::Prefab {
            kind: PlatformerPrefabKind::MovingPlatform,
            center: Vec3::new(10.0 + far * 0.5, 1.0, 0.0),
            yaw_degrees: 0.0,
        });
        let problems = chunk.validate(envelope);
        assert!(
            !problems
                .iter()
                .any(|problem| matches!(problem, ChunkProblem::GapTooWide { .. })),
            "a bridged gap is a designed crossing: {problems:?}"
        );
    }

    #[test]
    fn only_ascent_chunks_may_change_height() {
        let envelope = JumpEnvelope::standard();
        let climbing_traverse = ChunkDef {
            role: ChunkRole::Traverse,
            exit: ChunkSocket::new(20.0, 9.0, 0.0),
            ..flat_pad("sneaky", ChunkRole::Traverse, 20.0)
        };
        assert!(climbing_traverse
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::UnexpectedClimb { .. })));

        // The same geometry as an Ascent chunk is legitimate.
        let ascent = ChunkDef {
            role: ChunkRole::Ascent,
            ..climbing_traverse.clone()
        };
        assert!(!ascent
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::UnexpectedClimb { .. })));
    }

    #[test]
    fn combat_rooms_must_have_room_to_fight() {
        let envelope = JumpEnvelope::standard();
        let cramped = flat_pad("closet", ChunkRole::Arena, 8.0);
        assert!(cramped
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::ArenaTooSmall { .. })));

        let roomy = flat_pad("hall", ChunkRole::Arena, 30.0);
        assert!(!roomy
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::ArenaTooSmall { .. })));

        // The same small footprint is fine for a traverse chunk.
        assert!(!flat_pad("ledge", ChunkRole::Traverse, 8.0)
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::ArenaTooSmall { .. })));
    }

    #[test]
    fn a_socket_floating_off_the_geometry_is_caught() {
        let envelope = JumpEnvelope::standard();
        let adrift = ChunkDef {
            exit: ChunkSocket::new(20.0, 40.0, 0.0),
            role: ChunkRole::Ascent,
            ..flat_pad("adrift", ChunkRole::Ascent, 20.0)
        };
        // Exit is 40m up with no geometry beneath it — assembling this would
        // hand the next chunk off into thin air.
        assert!(adrift
            .validate(envelope)
            .iter()
            .any(|p| matches!(p, ChunkProblem::SocketAdrift { entry: false })));
    }

    #[test]
    fn an_empty_chunk_is_never_valid() {
        let empty = ChunkDef {
            pieces: Vec::new(),
            ..flat_pad("void", ChunkRole::Traverse, 10.0)
        };
        assert_eq!(
            empty.validate(JumpEnvelope::standard()),
            vec![ChunkProblem::NoPieces]
        );
    }
}
