use bevy_math::Vec3;
use starfall_platformer::{
    ChunkDef, ChunkPiece, ChunkRole, ChunkSocket, ChunkTheme, JumpEnvelope, RouteDefinition,
};

fn pad(id: &'static str, role: ChunkRole, width: f32) -> ChunkDef {
    ChunkDef {
        id,
        label: id,
        theme: ChunkTheme::Cityscape,
        role,
        entry: ChunkSocket::new(0.0, 1.0, 0.0),
        exit: ChunkSocket::new(width, 1.0, 0.0),
        pieces: vec![ChunkPiece::Solid {
            center: Vec3::new(width * 0.5, 0.0, 0.0),
            size: Vec3::new(width, 2.0, 18.0),
        }],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = [
        pad("my_game.arrival", ChunkRole::Arrival, 24.0),
        pad("my_game.arena", ChunkRole::Arena, 32.0),
    ];
    let route = RouteDefinition {
        id: "my_game.first_route",
        label: "First Route",
        brief: "Learn to move, then hold the arena",
        theme: ChunkTheme::Cityscape,
        chunks: &["my_game.arrival", "my_game.arena"],
    };
    let resolve = |id: &str| catalog.iter().find(|chunk| chunk.id == id).cloned();
    let problems = route.validate_with(JumpEnvelope::standard(), resolve);
    if !problems.is_empty() {
        return Err(problems
            .iter()
            .map(|problem| problem.message())
            .collect::<Vec<_>>()
            .join("; ")
            .into());
    }
    let placed = route
        .assemble_with(Vec3::ZERO, resolve)
        .map_err(|problem| problem.message())?;
    println!("{} compiled into {} chunks", route.label, placed.len());
    Ok(())
}
