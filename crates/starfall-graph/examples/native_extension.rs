use starfall_graph::{
    GraphDocument, GraphDomain, NativeNode, NodeDefinition, NodeRegistry, PortDefinition, PortType,
    StableId,
};

struct PrintMessage;

impl NativeNode for PrintMessage {
    fn definition() -> NodeDefinition {
        NodeDefinition::new(
            id("example.behavior.print_message"),
            1,
            "Print Message",
            "Example/Behavior",
            GraphDomain::Behavior,
        )
        .input(PortDefinition::new(
            id("message"),
            "Message",
            PortType::String,
        ))
        .output(PortDefinition::new(
            id("finished"),
            "Finished",
            PortType::Signal,
        ))
    }
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("example IDs are valid")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = NodeRegistry::default();
    registry.register::<PrintMessage>()?;

    let graph = GraphDocument::new(id("example.behavior.hello"), GraphDomain::Behavior);
    assert!(graph.validate(&registry).is_empty());
    println!("registered {} native node type", registry.iter().count());
    Ok(())
}
