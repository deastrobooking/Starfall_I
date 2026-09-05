//! A separate consumer package with no direct Bevy, Forge, or demo dependency.

use starfall_i::graph::StableId;
use starfall_i::platformer::{ChunkPiece, ChunkRole, ChunkSocket, ChunkTheme};
use starfall_i::prelude::*;

fn id(value: &str) -> StableId {
    StableId::new(value).expect("example IDs are valid")
}

fn catalog(chunk_id: &str) -> Option<ChunkDef> {
    let (id, role) = match chunk_id {
        "example.landing" => ("example.landing", ChunkRole::Arrival),
        "example.finish" => ("example.finish", ChunkRole::Arena),
        _ => return None,
    };
    Some(ChunkDef {
        id,
        label: "Sky Plaza",
        theme: ChunkTheme::Custom("example.sky"),
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
        id("example.first_route"),
        "First Route",
        id("example.sky"),
    )
    .chunk(id("landing"), id("example.landing"))
    .chunk(id("finish"), id("example.finish"))
    .build();

    let compiled = compile_platformer_graph(&graph, &registry)?;
    let json = compiled.document.to_json_pretty()?;
    let document = PlatformerRouteDocument::parse(&json)?;
    let runtime = document.compile_runtime([0.0, 4.0, 0.0], JumpEnvelope::standard(), catalog)?;
    assert_eq!(runtime.chunks.len(), 2);
    assert!(document
        .compile_runtime([0.0, 4.0, 0.0], JumpEnvelope::standard(), |_| None)
        .is_err());
    println!(
        "{}: authored, serialized, and assembled {} chunks through the public facade",
        document.label,
        runtime.chunks.len()
    );
    Ok(())
}
