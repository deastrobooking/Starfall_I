//! Shared-screen levels expressed as **routes** — ordered chunk sequences.
//!
//! A level here is a short, bounded journey the party makes together: arrive,
//! travel, climb, fight, arrive somewhere that means something. Because a route
//! is just an ordered list of chunk ids, a designer composes levels from the
//! catalogue instead of writing coordinates, and the assembler derives the
//! geometry — including the total climb.
//!
//! Progression is deliberately simple, matching the format: routes are listed
//! in order, each one unlocks the next, and a route's *shape* (its roles in
//! sequence) is what teaches. Validation holds every route to that shape so a
//! level cannot ship that opens mid-air or ends without a destination.

use bevy::prelude::*;

use super::platformer_chunk_library::chunk_by_id;
#[cfg(test)]
use super::platformer_chunks::ChunkRole;
use super::platformer_chunks::{
    ChunkDef, ChunkTheme, JumpEnvelope, PlacedChunk, RouteDefinition, RouteProblem,
};

/// One authored level.
pub type RouteDef = RouteDefinition;

/// Heavy Water catalogue convenience methods over the reusable route schema.
pub trait HeavyWaterRouteExt {
    fn resolve(&self) -> Result<Vec<ChunkDef>, RouteProblem>;
    fn assemble(&self, start: Vec3) -> Result<Vec<PlacedChunk>, RouteProblem>;
    fn total_climb(&self) -> f32;
    fn summary(&self) -> String;
    fn validate(&self, envelope: JumpEnvelope) -> Vec<RouteProblem>;
}

impl HeavyWaterRouteExt for RouteDefinition {
    /// Resolve every chunk id, failing on the first unknown one.
    fn resolve(&self) -> Result<Vec<ChunkDef>, RouteProblem> {
        self.resolve_with(chunk_by_id)
    }

    /// Build the route's world geometry starting from `start`.
    fn assemble(&self, start: Vec3) -> Result<Vec<PlacedChunk>, RouteProblem> {
        self.assemble_with(start, chunk_by_id)
    }

    /// Total height the party climbs across the whole level.
    fn total_climb(&self) -> f32 {
        self.total_climb_with(chunk_by_id).unwrap_or(0.0)
    }

    /// A one-line description for level select and load logs: what the level
    /// is, how far it travels, and how much of it is climb.
    fn summary(&self) -> String {
        let chunks = match self.resolve() {
            Ok(chunks) => chunks,
            Err(problem) => return problem.message(),
        };
        let length: f32 = chunks.iter().map(|chunk| chunk.length()).sum();
        format!(
            "{} — {} · {} chunks · {:.0}m across, {:.0}m up",
            self.label,
            self.theme.label(),
            chunks.len(),
            length,
            self.total_climb()
        )
    }

    /// Check the route's shape and every chunk it uses.
    fn validate(&self, envelope: JumpEnvelope) -> Vec<RouteProblem> {
        self.validate_with(envelope, chunk_by_id)
    }
}

/// The shipped level order. Each entry is a short journey; finishing one
/// unlocks the next. The set deliberately walks the party from the city, up a
/// mountain, into a castle, and finally underground — introducing one theme's
/// vocabulary at a time.
pub const ROUTES: [RouteDef; 4] = [
    RouteDef {
        id: "route_city_rooftops",
        label: "1 · Rooftop Run",
        brief: "Cross the star city together and hold the plaza",
        theme: ChunkTheme::Cityscape,
        chunks: &[
            "city_rooftop_arrival",
            "city_rooftop_gaps",
            "city_scaffold_ascent",
            "city_plaza_arena",
        ],
    },
    RouteDef {
        id: "route_mountain_ascent",
        label: "2 · The Long Climb",
        brief: "Climb the ledges and cross the spire",
        theme: ChunkTheme::Mountain,
        chunks: &[
            "mountain_shelf_arrival",
            "mountain_ledge_traverse",
            "mountain_switchback_ascent",
            "mountain_spire_crossing",
            "city_plaza_arena",
        ],
    },
    RouteDef {
        id: "route_castle_stairwell",
        label: "3 · Stairwell Assault",
        brief: "Fight up the stairwell to the throne at the peak",
        theme: ChunkTheme::Castle,
        chunks: &[
            "castle_gate_arrival",
            "castle_rampart_traverse",
            "castle_stairwell_ascent",
            "castle_hall_arena",
            "castle_stairwell_ascent",
            "castle_throne_boss",
        ],
    },
    RouteDef {
        id: "route_cavern_depths",
        label: "4 · Under the Peak",
        brief: "Pick a path through the formations and climb out",
        theme: ChunkTheme::Cavern,
        chunks: &[
            "cavern_mouth_arrival",
            "cavern_formation_traverse",
            "cavern_chimney_ascent",
            "castle_hall_arena",
        ],
    },
];

/// The route at a progression index, if the party has got that far.
pub fn route_at(index: usize) -> Option<RouteDef> {
    ROUTES.get(index).copied()
}

/// Look a route up by id.
pub fn route_by_id(id: &str) -> Option<RouteDef> {
    ROUTES.into_iter().find(|route| route.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_route_is_playable_end_to_end() {
        let envelope = JumpEnvelope::standard();
        for route in ROUTES {
            let problems: Vec<String> = route
                .validate(envelope)
                .iter()
                .map(RouteProblem::message)
                .collect();
            assert!(
                problems.is_empty(),
                "{} ({}): {}",
                route.label,
                route.id,
                problems.join("; ")
            );
        }
    }

    #[test]
    fn routes_open_on_arrival_and_end_somewhere_that_matters() {
        for route in ROUTES {
            let chunks = route.resolve().expect("ids resolve");
            assert_eq!(chunks[0].role, ChunkRole::Arrival, "{}", route.label);
            assert!(
                matches!(
                    chunks[chunks.len() - 1].role,
                    ChunkRole::Arena | ChunkRole::Boss
                ),
                "{} does not arrive anywhere",
                route.label
            );
        }
    }

    #[test]
    fn a_malformed_route_is_rejected_with_a_reason() {
        let envelope = JumpEnvelope::standard();

        // Starting mid-traverse: the party would materialise on a ledge.
        let no_arrival = RouteDef {
            id: "bad_open",
            label: "Bad Open",
            brief: "",
            theme: ChunkTheme::Castle,
            chunks: &["castle_rampart_traverse", "castle_hall_arena"],
        };
        assert!(no_arrival
            .validate(envelope)
            .contains(&RouteProblem::DoesNotOpenOnArrival));

        // Ending on a corridor: the level just stops.
        let no_end = RouteDef {
            id: "bad_end",
            label: "Bad End",
            brief: "",
            theme: ChunkTheme::Castle,
            chunks: &["castle_gate_arrival", "castle_rampart_traverse"],
        };
        assert!(no_end
            .validate(envelope)
            .contains(&RouteProblem::NoDestination));

        // A typo in a chunk id is caught by name rather than silently dropped.
        let typo = RouteDef {
            id: "bad_id",
            label: "Typo",
            brief: "",
            theme: ChunkTheme::Castle,
            chunks: &["castle_gate_arrival", "castle_stairwel_ascent"],
        };
        assert!(typo
            .validate(envelope)
            .iter()
            .any(|problem| matches!(problem, RouteProblem::UnknownChunk(_))));

        // A second arrival mid-route reads as a false start.
        let restart = RouteDef {
            id: "bad_mid",
            label: "Restart",
            brief: "",
            theme: ChunkTheme::Castle,
            chunks: &[
                "castle_gate_arrival",
                "castle_gate_arrival",
                "castle_hall_arena",
            ],
        };
        assert!(restart
            .validate(envelope)
            .iter()
            .any(|problem| matches!(problem, RouteProblem::ArrivalMidRoute { .. })));
    }

    #[test]
    fn the_stairwell_route_actually_climbs_to_its_boss() {
        let route = route_by_id("route_castle_stairwell").expect("route exists");
        // The headline fantasy: real height gained, ending at the throne.
        assert!(
            route.total_climb() >= 35.0,
            "climb was only {:.1}m",
            route.total_climb()
        );
        let chunks = route.resolve().unwrap();
        assert_eq!(chunks[chunks.len() - 1].role, ChunkRole::Boss);

        // Reusing one chunk twice is legitimate and must place it twice, at
        // different heights — this is what makes the catalogue reusable.
        let placed = route.assemble(Vec3::ZERO).unwrap();
        let stairwells: Vec<&PlacedChunk> = placed
            .iter()
            .filter(|chunk| chunk.def.id == "castle_stairwell_ascent")
            .collect();
        assert_eq!(stairwells.len(), 2);
        assert!(stairwells[1].origin.y > stairwells[0].origin.y);
    }

    #[test]
    fn assembled_routes_are_continuous_and_start_where_asked() {
        let start = Vec3::new(12.0, 3.0, 0.0);
        for route in ROUTES {
            let placed = route.assemble(start).expect("assembles");
            assert_eq!(placed[0].world_entry(), start, "{}", route.label);
            for pair in placed.windows(2) {
                assert_eq!(
                    pair[0].world_exit(),
                    pair[1].world_entry(),
                    "{} has a seam",
                    route.label
                );
            }
        }
    }

    #[test]
    fn progression_is_ordered_and_addressable() {
        assert!(ROUTES.len() >= 4);
        for (index, route) in ROUTES.iter().enumerate() {
            assert_eq!(route_at(index).map(|r| r.id), Some(route.id));
            assert_eq!(route_by_id(route.id).map(|r| r.id), Some(route.id));
            assert!(!route.brief.is_empty(), "{} needs a brief", route.label);
        }
        assert!(route_at(ROUTES.len()).is_none());
        assert!(route_by_id("nope").is_none());

        // Ids are unique, or progression could not address a level.
        let mut ids: Vec<&str> = ROUTES.iter().map(|route| route.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }
}
