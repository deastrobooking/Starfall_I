//! The shipped chunk catalogue: reusable cityscape, mountain, castle, and
//! cavern sections for shared-screen platformer routes.
//!
//! Every chunk here is authored against one rule — **it must survive
//! [`ChunkDef::validate`]** — so the catalogue cannot ship a gap the party
//! cannot jump or a fighting room they cannot fight in. A test walks the whole
//! library and fails the build otherwise.
//!
//! Geometry is deliberately blocky and generous: four players share one screen,
//! so depth (z) stays wide enough that nobody queues to cross, and landings are
//! broader than the jump that reaches them.

use bevy::prelude::*;

use super::platformer_chunks::{ChunkDef, ChunkPiece, ChunkRole, ChunkSocket, ChunkTheme};
use crate::engine_tools::platformer_prefabs::PlatformerPrefabKind;

/// Shared play depth. Wide enough for four abreast plus fighting room.
const DEPTH: f32 = 18.0;
/// Standard floor thickness; sockets sit on top of it.
const FLOOR: f32 = 2.0;

fn floor(center_x: f32, width: f32, top_y: f32) -> ChunkPiece {
    ChunkPiece::Solid {
        center: Vec3::new(center_x, top_y - FLOOR * 0.5, 0.0),
        size: Vec3::new(width, FLOOR, DEPTH),
    }
}

fn block(center: Vec3, size: Vec3) -> ChunkPiece {
    ChunkPiece::Solid { center, size }
}

fn trim(center: Vec3, size: Vec3) -> ChunkPiece {
    ChunkPiece::Trim { center, size }
}

/// Backdrop rock, enclosing walls, and ceilings: solid mass the party cannot
/// walk along. Kept distinct from `block` so route validation never mistakes a
/// cliff face for a step.
fn scenery(center: Vec3, size: Vec3) -> ChunkPiece {
    ChunkPiece::Scenery { center, size }
}

fn prefab(kind: PlatformerPrefabKind, center: Vec3) -> ChunkPiece {
    ChunkPiece::Prefab {
        kind,
        center,
        yaw_degrees: 0.0,
    }
}

// ── Cityscape ────────────────────────────────────────────────────────────────

/// Rooftop the party drops onto: flat, railed, unmistakably a starting place.
pub fn city_rooftop_arrival() -> ChunkDef {
    ChunkDef {
        id: "city_rooftop_arrival",
        label: "Rooftop Landing",
        theme: ChunkTheme::Cityscape,
        role: ChunkRole::Arrival,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(26.0, 0.0, 0.0),
        pieces: vec![
            floor(13.0, 26.0, 0.0),
            // Parapet rails read the edges at a glance on a shared screen.
            trim(Vec3::new(13.0, 0.6, DEPTH * 0.5 - 0.4), Vec3::new(26.0, 1.2, 0.8)),
            trim(Vec3::new(13.0, 0.6, -DEPTH * 0.5 + 0.4), Vec3::new(26.0, 1.2, 0.8)),
            trim(Vec3::new(1.0, 1.6, 0.0), Vec3::new(1.2, 3.2, 3.0)),
        ],
    }
}

/// Three rooftops with honest gaps — the city's jump lesson.
pub fn city_rooftop_gaps() -> ChunkDef {
    ChunkDef {
        id: "city_rooftop_gaps",
        label: "Rooftop Gaps",
        theme: ChunkTheme::Cityscape,
        role: ChunkRole::Traverse,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(58.0, 0.0, 0.0),
        pieces: vec![
            floor(7.0, 14.0, 0.0),
            floor(29.0, 16.0, 0.0),
            floor(51.0, 14.0, 0.0),
            trim(Vec3::new(29.0, 1.0, 0.0), Vec3::new(16.0, 0.4, DEPTH)),
        ],
    }
}

/// A service scaffold climbing the side of a tower.
pub fn city_scaffold_ascent() -> ChunkDef {
    ChunkDef {
        id: "city_scaffold_ascent",
        label: "Scaffold Climb",
        theme: ChunkTheme::Cityscape,
        role: ChunkRole::Ascent,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(30.0, 14.0, 0.0),
        pieces: vec![
            floor(6.0, 12.0, 0.0),
            floor(15.0, 10.0, 4.5),
            floor(24.0, 12.0, 9.0),
            floor(30.0, 12.0, 14.0),
            prefab(PlatformerPrefabKind::Ladder, Vec3::new(11.0, 2.0, 0.0)),
        ],
    }
}

/// A plaza between towers — the city's fighting room.
pub fn city_plaza_arena() -> ChunkDef {
    ChunkDef {
        id: "city_plaza_arena",
        label: "Neon Plaza",
        theme: ChunkTheme::Cityscape,
        role: ChunkRole::Arena,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(34.0, 0.0, 0.0),
        pieces: vec![
            floor(17.0, 34.0, 0.0),
            // Low cover the party can fight around without breaking sightlines.
            block(Vec3::new(11.0, 1.2, 5.0), Vec3::new(3.0, 2.4, 3.0)),
            block(Vec3::new(23.0, 1.2, -5.0), Vec3::new(3.0, 2.4, 3.0)),
            trim(Vec3::new(17.0, 0.2, 0.0), Vec3::new(34.0, 0.4, 4.0)),
        ],
    }
}

// ── Mountain ─────────────────────────────────────────────────────────────────

/// A wind-scoured shelf where the climb begins.
pub fn mountain_shelf_arrival() -> ChunkDef {
    ChunkDef {
        id: "mountain_shelf_arrival",
        label: "Base Camp Shelf",
        theme: ChunkTheme::Mountain,
        role: ChunkRole::Arrival,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(28.0, 0.0, 0.0),
        pieces: vec![
            floor(14.0, 28.0, 0.0),
            // Rock mass behind the shelf gives the screen a solid backdrop.
            scenery(Vec3::new(14.0, 6.0, -DEPTH * 0.5 - 3.0), Vec3::new(28.0, 14.0, 8.0)),
            block(Vec3::new(3.0, 2.0, 4.0), Vec3::new(4.0, 4.0, 4.0)),
        ],
    }
}

/// Stepping ledges across a ravine face.
pub fn mountain_ledge_traverse() -> ChunkDef {
    ChunkDef {
        id: "mountain_ledge_traverse",
        label: "Ravine Ledges",
        theme: ChunkTheme::Mountain,
        role: ChunkRole::Traverse,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(56.0, 0.0, 0.0),
        pieces: vec![
            floor(6.0, 12.0, 0.0),
            floor(24.0, 12.0, 0.0),
            floor(44.0, 10.0, 0.0),
            floor(53.0, 8.0, 0.0),
            scenery(Vec3::new(28.0, 8.0, -DEPTH * 0.5 - 2.0), Vec3::new(56.0, 18.0, 6.0)),
        ],
    }
}

/// Switchbacks — the honest way up a mountain face.
pub fn mountain_switchback_ascent() -> ChunkDef {
    ChunkDef {
        id: "mountain_switchback_ascent",
        label: "Switchback Climb",
        theme: ChunkTheme::Mountain,
        role: ChunkRole::Ascent,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(40.0, 16.0, 0.0),
        pieces: vec![
            floor(8.0, 16.0, 0.0),
            floor(20.0, 14.0, 4.0),
            floor(30.0, 12.0, 8.0),
            floor(36.0, 12.0, 12.0),
            floor(40.0, 12.0, 16.0),
            scenery(Vec3::new(20.0, 10.0, -DEPTH * 0.5 - 3.0), Vec3::new(40.0, 26.0, 8.0)),
        ],
    }
}

/// An exposed spire crossing on moving stone — the mountain's set piece.
pub fn mountain_spire_crossing() -> ChunkDef {
    ChunkDef {
        id: "mountain_spire_crossing",
        label: "Spire Crossing",
        theme: ChunkTheme::Mountain,
        role: ChunkRole::Traverse,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(64.0, 0.0, 0.0),
        pieces: vec![
            floor(7.0, 14.0, 0.0),
            floor(57.0, 14.0, 0.0),
            // A long span the party crosses on a carried platform.
            prefab(PlatformerPrefabKind::MovingPlatform, Vec3::new(32.0, 0.5, 0.0)),
            scenery(Vec3::new(7.0, -8.0, 0.0), Vec3::new(10.0, 16.0, 10.0)),
            scenery(Vec3::new(57.0, -8.0, 0.0), Vec3::new(10.0, 16.0, 10.0)),
        ],
    }
}

// ── Castle ───────────────────────────────────────────────────────────────────

/// The gatehouse: where mountain becomes worked stone.
pub fn castle_gate_arrival() -> ChunkDef {
    ChunkDef {
        id: "castle_gate_arrival",
        label: "Castle Gate",
        theme: ChunkTheme::Castle,
        role: ChunkRole::Arrival,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(30.0, 0.0, 0.0),
        pieces: vec![
            floor(15.0, 30.0, 0.0),
            // Gate towers frame the entrance and read as an arrival.
            scenery(Vec3::new(6.0, 7.0, DEPTH * 0.5 - 2.0), Vec3::new(5.0, 16.0, 5.0)),
            scenery(Vec3::new(6.0, 7.0, -DEPTH * 0.5 + 2.0), Vec3::new(5.0, 16.0, 5.0)),
            trim(Vec3::new(6.0, 14.0, 0.0), Vec3::new(5.0, 2.0, DEPTH)),
        ],
    }
}

/// A rampart walk along the curtain wall.
pub fn castle_rampart_traverse() -> ChunkDef {
    ChunkDef {
        id: "castle_rampart_traverse",
        label: "Curtain Rampart",
        theme: ChunkTheme::Castle,
        role: ChunkRole::Traverse,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(48.0, 0.0, 0.0),
        pieces: vec![
            floor(12.0, 24.0, 0.0),
            floor(40.0, 16.0, 0.0),
            // Crenellations along both sides.
            trim(Vec3::new(12.0, 1.4, DEPTH * 0.5 - 0.6), Vec3::new(24.0, 2.8, 1.2)),
            trim(Vec3::new(40.0, 1.4, DEPTH * 0.5 - 0.6), Vec3::new(16.0, 2.8, 1.2)),
            prefab(PlatformerPrefabKind::RotatingBridge, Vec3::new(30.0, 0.4, 0.0)),
        ],
    }
}

/// The stairwell — the shape the whole format was described around: a fighting
/// climb where the party ascends together under pressure.
pub fn castle_stairwell_ascent() -> ChunkDef {
    ChunkDef {
        id: "castle_stairwell_ascent",
        label: "Stairwell Assault",
        theme: ChunkTheme::Castle,
        role: ChunkRole::Ascent,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(44.0, 20.0, 0.0),
        pieces: vec![
            // Wide landings between flights: each is a place to stand and
            // fight, which is what makes a stairwell battle rather than a
            // climb with enemies on it.
            // Even four-metre flights: a stairwell fight must never ask for a
            // jump the party cannot make while defending themselves.
            floor(8.0, 16.0, 0.0),
            floor(18.0, 10.0, 4.0),
            floor(26.0, 14.0, 8.0),
            floor(33.0, 10.0, 12.0),
            floor(38.0, 10.0, 16.0),
            floor(43.0, 14.0, 20.0),
            // Enclosing walls keep the fight readable on a shared screen.
            scenery(Vec3::new(22.0, 12.0, -DEPTH * 0.5 - 1.5), Vec3::new(44.0, 30.0, 3.0)),
            scenery(Vec3::new(22.0, 12.0, DEPTH * 0.5 + 1.5), Vec3::new(44.0, 30.0, 3.0)),
            trim(Vec3::new(26.0, 8.4, 0.0), Vec3::new(14.0, 0.4, DEPTH)),
        ],
    }
}

/// A hall partway up: catch your breath, then fight.
pub fn castle_hall_arena() -> ChunkDef {
    ChunkDef {
        id: "castle_hall_arena",
        label: "Great Hall",
        theme: ChunkTheme::Castle,
        role: ChunkRole::Arena,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(38.0, 0.0, 0.0),
        pieces: vec![
            floor(19.0, 38.0, 0.0),
            scenery(Vec3::new(10.0, 5.0, 6.0), Vec3::new(2.5, 10.0, 2.5)),
            scenery(Vec3::new(10.0, 5.0, -6.0), Vec3::new(2.5, 10.0, 2.5)),
            scenery(Vec3::new(28.0, 5.0, 6.0), Vec3::new(2.5, 10.0, 2.5)),
            scenery(Vec3::new(28.0, 5.0, -6.0), Vec3::new(2.5, 10.0, 2.5)),
            trim(Vec3::new(19.0, 10.0, 0.0), Vec3::new(38.0, 1.0, DEPTH)),
        ],
    }
}

/// The summit throne room: the boss fight the climb was for.
pub fn castle_throne_boss() -> ChunkDef {
    ChunkDef {
        id: "castle_throne_boss",
        label: "Throne of the Peak",
        theme: ChunkTheme::Castle,
        role: ChunkRole::Boss,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(46.0, 0.0, 0.0),
        pieces: vec![
            floor(23.0, 46.0, 0.0),
            // A raised dais the boss holds; the party has to take the height.
            block(Vec3::new(38.0, 1.5, 0.0), Vec3::new(12.0, 3.0, 12.0)),
            trim(Vec3::new(38.0, 5.0, -4.0), Vec3::new(4.0, 7.0, 1.5)),
            scenery(Vec3::new(23.0, 9.0, -DEPTH * 0.5 - 1.5), Vec3::new(46.0, 24.0, 3.0)),
            scenery(Vec3::new(23.0, 9.0, DEPTH * 0.5 + 1.5), Vec3::new(46.0, 24.0, 3.0)),
        ],
    }
}

// ── Cavern ───────────────────────────────────────────────────────────────────

/// A cave mouth: the dungeon entrance.
pub fn cavern_mouth_arrival() -> ChunkDef {
    ChunkDef {
        id: "cavern_mouth_arrival",
        label: "Cave Mouth",
        theme: ChunkTheme::Cavern,
        role: ChunkRole::Arrival,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(24.0, 0.0, 0.0),
        pieces: vec![
            floor(12.0, 24.0, 0.0),
            scenery(Vec3::new(12.0, 12.0, 0.0), Vec3::new(24.0, 4.0, DEPTH + 6.0)),
            scenery(Vec3::new(4.0, 3.0, 7.0), Vec3::new(3.0, 6.0, 3.0)),
        ],
    }
}

/// Rock formations to pick a way through — stalagmite footing.
pub fn cavern_formation_traverse() -> ChunkDef {
    ChunkDef {
        id: "cavern_formation_traverse",
        label: "Stone Formations",
        theme: ChunkTheme::Cavern,
        role: ChunkRole::Traverse,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(52.0, 0.0, 0.0),
        pieces: vec![
            floor(7.0, 14.0, 0.0),
            floor(26.0, 12.0, 0.0),
            floor(46.0, 12.0, 0.0),
            // Hanging formations above, for silhouette rather than footing.
            trim(Vec3::new(16.0, 11.0, 3.0), Vec3::new(2.0, 6.0, 2.0)),
            trim(Vec3::new(36.0, 11.0, -3.0), Vec3::new(2.0, 6.0, 2.0)),
            scenery(Vec3::new(26.0, 14.0, 0.0), Vec3::new(52.0, 4.0, DEPTH + 6.0)),
        ],
    }
}

/// A chimney climb up through the rock.
pub fn cavern_chimney_ascent() -> ChunkDef {
    ChunkDef {
        id: "cavern_chimney_ascent",
        label: "Chimney Climb",
        theme: ChunkTheme::Cavern,
        role: ChunkRole::Ascent,
        entry: ChunkSocket::new(0.0, 0.0, 0.0),
        exit: ChunkSocket::new(26.0, 18.0, 0.0),
        pieces: vec![
            floor(7.0, 14.0, 0.0),
            floor(14.0, 8.0, 5.0),
            floor(20.0, 8.0, 10.0),
            floor(25.0, 10.0, 14.0),
            floor(26.0, 10.0, 18.0),
            prefab(PlatformerPrefabKind::SpringPlatform, Vec3::new(10.0, 0.6, 0.0)),
        ],
    }
}

/// Every chunk in the catalogue.
pub fn chunk_library() -> Vec<ChunkDef> {
    vec![
        city_rooftop_arrival(),
        city_rooftop_gaps(),
        city_scaffold_ascent(),
        city_plaza_arena(),
        mountain_shelf_arrival(),
        mountain_ledge_traverse(),
        mountain_switchback_ascent(),
        mountain_spire_crossing(),
        castle_gate_arrival(),
        castle_rampart_traverse(),
        castle_stairwell_ascent(),
        castle_hall_arena(),
        castle_throne_boss(),
        cavern_mouth_arrival(),
        cavern_formation_traverse(),
        cavern_chimney_ascent(),
    ]
}

/// Look a chunk up by its stable id.
pub fn chunk_by_id(id: &str) -> Option<ChunkDef> {
    chunk_library().into_iter().find(|chunk| chunk.id == id)
}

/// Every chunk of one theme, for palettes and theme-locked routes.
///
/// The consumer is the chunk palette in the level workspace, which is the next
/// step in this track; until it lands this is exercised only by the catalogue
/// tests. Remove the allow when the palette calls it.
#[allow(dead_code)]
pub fn chunks_for_theme(theme: ChunkTheme) -> Vec<ChunkDef> {
    chunk_library()
        .into_iter()
        .filter(|chunk| chunk.theme == theme)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::platformer_chunks::{ChunkProblem, JumpEnvelope};

    #[test]
    fn every_shipped_chunk_is_playable() {
        // The catalogue's one rule. A chunk with an unjumpable gap, a cramped
        // arena, or a socket hanging in space fails the build, not a playtest.
        let envelope = JumpEnvelope::standard();
        for chunk in chunk_library() {
            let problems: Vec<String> = chunk
                .validate(envelope)
                .iter()
                .map(ChunkProblem::message)
                .collect();
            assert!(
                problems.is_empty(),
                "{} ({}): {}",
                chunk.label,
                chunk.id,
                problems.join("; ")
            );
        }
    }

    #[test]
    fn chunk_ids_are_unique_and_resolvable() {
        let library = chunk_library();
        let mut ids: Vec<&str> = library.iter().map(|chunk| chunk.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate chunk id");

        for chunk in &library {
            assert_eq!(chunk_by_id(chunk.id).map(|c| c.id), Some(chunk.id));
            assert!(!chunk.label.is_empty());
        }
        assert!(chunk_by_id("no_such_chunk").is_none());
    }

    #[test]
    fn every_theme_can_build_a_complete_route() {
        // A theme is only useful if it can open, travel, and climb on its own.
        for theme in ChunkTheme::ALL {
            let chunks = chunks_for_theme(theme);
            assert!(!chunks.is_empty(), "{} has no chunks", theme.label());
            for role in [ChunkRole::Arrival, ChunkRole::Traverse, ChunkRole::Ascent] {
                assert!(
                    chunks.iter().any(|chunk| chunk.role == role),
                    "{} cannot supply a {} chunk",
                    theme.label(),
                    role.label()
                );
            }
        }
    }

    #[test]
    fn ascent_chunks_actually_climb_and_others_stay_level() {
        for chunk in chunk_library() {
            if chunk.role == ChunkRole::Ascent {
                assert!(
                    chunk.rise() > 3.0,
                    "{} is an ascent that barely climbs ({:.1}m)",
                    chunk.label,
                    chunk.rise()
                );
            } else {
                assert!(
                    chunk.rise().abs() <= 0.5,
                    "{} changes height outside an ascent role",
                    chunk.label
                );
            }
        }
    }

    #[test]
    fn the_castle_can_stage_the_stairwell_climb_to_a_boss() {
        // The format's headline fantasy: fight up a stairwell to a boss room.
        let castle = chunks_for_theme(ChunkTheme::Castle);
        let stairwell = castle
            .iter()
            .find(|chunk| chunk.id == "castle_stairwell_ascent")
            .expect("castle has a stairwell");
        assert!(stairwell.rise() >= 15.0, "a stairwell should gain real height");
        // Its landings are wide enough to stop and fight on.
        let wide_landings = stairwell
            .pieces
            .iter()
            .filter(|piece| {
                let (min, max) = piece.x_span();
                piece.is_landing_surface() && (max - min) >= 12.0
            })
            .count();
        assert!(wide_landings >= 2, "a fighting climb needs standing room");

        assert!(castle.iter().any(|chunk| chunk.role == ChunkRole::Boss));
    }
}
