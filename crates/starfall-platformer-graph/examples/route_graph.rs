use starfall_graph::{NodeRegistry, StableId};
use starfall_platformer::{ChunkDef, ChunkPiece, ChunkRole, ChunkSocket, ChunkTheme, JumpEnvelope};
use starfall_platformer_graph::{
    compile_platformer_graph, register_platformer_nodes, PlatformerRouteGraphBuilder,
};

fn id(value: &str) -> StableId {
    StableId::new(value).expect("example IDs are valid")
}

fn catalog(chunk_id: &str) -> Option<ChunkDef> {
    let role = match chunk_id {
        "my_game.safe_landing" => ChunkRole::Arrival,
        "my_game.finish_arena" => ChunkRole::Arena,
        _ => return None,
    };
    Some(ChunkDef {
        id: match role {
            ChunkRole::Arrival => "my_game.safe_landing",
            _ => "my_game.finish_arena",
        },
        label: "Stone Plaza",
        theme: ChunkTheme::Custom("my_game.sky_city"),
        role,
        entry: ChunkSocket::new(-15.0, 0.5, 0.0),
        exit: ChunkSocket::new(15.0, 0.5, 0.0),
        pieces: vec![ChunkPiece::Solid {
            center: [0.0, 0.0, 0.0].into(),
            size: [30.0, 1.0, 8.0].into(),
        }],
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = NodeRegistry::default();
    register_platformer_nodes(&mut registry)?;

    let graph = PlatformerRouteGraphBuilder::new(
        id("my_game.first_route"),
        "First Route",
        id("my_game.sky_city"),
    )
    .brief("Cross the sky city and hold its plaza.")
    .chunk(id("landing"), id("my_game.safe_landing"))
    .chunk(id("finish"), id("my_game.finish_arena"))
    .build();

    let authored = compile_platformer_graph(&graph, &registry)?;
    println!("{}", authored.document.to_json_pretty()?);

    let runtime =
        authored
            .document
            .compile_runtime([0.0, 4.0, 0.0], JumpEnvelope::standard(), catalog)?;
    println!(
        "runtime route contains {} placed chunks",
        runtime.chunks.len()
    );
    Ok(())
}
