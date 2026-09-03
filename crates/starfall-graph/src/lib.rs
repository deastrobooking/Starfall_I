//! Typed authoring-graph contracts shared by Forge, runtime compilers, games,
//! and native extensions.
//!
//! This module deliberately contains no editor UI and executes no gameplay.
//! Graph documents are authored sources; domain compilers turn validated
//! documents into runtime plans. A shared kernel prevents object, behavior,
//! animation, UI, shader, world, and narrative tools from inventing
//! incompatible identity and connection models.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

pub const GRAPH_SCHEMA_VERSION: u32 = 2;

/// A stable, serialized identity. Display labels are never identities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct StableId(String);

impl<'de> Deserialize<'de> for StableId {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl StableId {
    pub fn new(value: impl Into<String>) -> Result<Self, StableIdError> {
        let value = value.into();
        validate_stable_id(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableIdError {
    pub value: String,
    pub reason: &'static str,
}

impl fmt::Display for StableIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid stable id {:?}: {}",
            self.value, self.reason
        )
    }
}

impl std::error::Error for StableIdError {}

fn validate_stable_id(value: &str) -> Result<(), StableIdError> {
    if value.is_empty() {
        return Err(StableIdError {
            value: value.to_owned(),
            reason: "the id is empty",
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-/".contains(&byte))
    {
        return Err(StableIdError {
            value: value.to_owned(),
            reason: "use lowercase ASCII letters, digits, '.', '_', '-', or '/'",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphDomain {
    Object,
    Behavior,
    Animation,
    Ui,
    Material,
    World,
    Dialogue,
    Mission,
    Campaign,
    Platformer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Signal,
    Bool,
    Integer,
    Float,
    String,
    Vec2,
    Vec3,
    Color,
    Transform,
    ObjectRef,
    AssetRef,
    PlayerRef,
    EntitySelector,
    Tag,
    Record(StableId),
    List(Box<PortType>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GraphValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Color([f32; 4]),
    Transform {
        translation: [f32; 3],
        rotation_xyzw: [f32; 4],
        scale: [f32; 3],
    },
    StableRef(StableId),
    List(Vec<GraphValue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDefinition {
    pub id: StableId,
    pub label: String,
    pub port_type: PortType,
    /// Multiple connections may target this input. Outputs are always allowed
    /// to fan out.
    pub multiple: bool,
}

impl PortDefinition {
    pub fn new(id: StableId, label: impl Into<String>, port_type: PortType) -> Self {
        Self {
            id,
            label: label.into(),
            port_type,
            multiple: false,
        }
    }

    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
}

/// Native metadata registered by an engine, game, or module plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDefinition {
    pub type_id: StableId,
    pub version: u32,
    pub display_name: String,
    pub category: String,
    pub domain: GraphDomain,
    pub inputs: BTreeMap<StableId, PortDefinition>,
    pub outputs: BTreeMap<StableId, PortDefinition>,
}

impl NodeDefinition {
    pub fn new(
        type_id: StableId,
        version: u32,
        display_name: impl Into<String>,
        category: impl Into<String>,
        domain: GraphDomain,
    ) -> Self {
        Self {
            type_id,
            version,
            display_name: display_name.into(),
            category: category.into(),
            domain,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
        }
    }

    pub fn input(mut self, port: PortDefinition) -> Self {
        self.inputs.insert(port.id.clone(), port);
        self
    }

    pub fn output(mut self, port: PortDefinition) -> Self {
        self.outputs.insert(port.id.clone(), port);
        self
    }
}

/// Implemented by native extensions that contribute a node type. Runtime
/// execution/compilation traits remain domain-specific so shader nodes and
/// gameplay nodes are not forced into the same execution model.
pub trait NativeNode: Send + Sync + 'static {
    fn definition() -> NodeDefinition;
}

#[derive(Resource, Debug, Default)]
pub struct NodeRegistry {
    definitions: BTreeMap<StableId, NodeDefinition>,
}

impl NodeRegistry {
    pub fn register<T: NativeNode>(&mut self) -> Result<(), NodeRegistryError> {
        self.register_definition(T::definition())
    }

    pub fn register_definition(
        &mut self,
        definition: NodeDefinition,
    ) -> Result<(), NodeRegistryError> {
        if self.definitions.contains_key(&definition.type_id) {
            return Err(NodeRegistryError::DuplicateType(definition.type_id));
        }
        self.definitions
            .insert(definition.type_id.clone(), definition);
        Ok(())
    }

    pub fn get(&self, type_id: &StableId) -> Option<&NodeDefinition> {
        self.definitions.get(type_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&StableId, &NodeDefinition)> {
        self.definitions.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRegistryError {
    DuplicateType(StableId),
}

impl fmt::Display for NodeRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateType(type_id) => {
                write!(formatter, "node type {type_id} is registered twice")
            }
        }
    }
}

impl std::error::Error for NodeRegistryError {}

pub struct GraphRegistryPlugin;

impl Plugin for GraphRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NodeRegistry>();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: StableId,
    pub type_id: StableId,
    pub type_version: u32,
    #[serde(default)]
    pub properties: BTreeMap<StableId, GraphValue>,
    /// Authoring-only canvas position. Runtime compilers ignore it.
    #[serde(default)]
    pub position: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PortRef {
    pub node_id: StableId,
    pub port_id: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphConnection {
    pub from: PortRef,
    pub to: PortRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphDocument {
    pub schema_version: u32,
    pub id: StableId,
    pub domain: GraphDomain,
    #[serde(default)]
    pub nodes: BTreeMap<StableId, GraphNode>,
    #[serde(default)]
    pub connections: BTreeSet<GraphConnection>,
}

impl GraphDocument {
    pub fn new(id: StableId, domain: GraphDomain) -> Self {
        Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            id,
            domain,
            nodes: BTreeMap::new(),
            connections: BTreeSet::new(),
        }
    }

    pub fn validate(&self, registry: &NodeRegistry) -> Vec<GraphDiagnostic> {
        let mut diagnostics = Vec::new();
        if self.schema_version > GRAPH_SCHEMA_VERSION {
            diagnostics.push(GraphDiagnostic::error(
                "graph.unsupported_schema",
                format!(
                    "graph schema {} is newer than supported schema {}",
                    self.schema_version, GRAPH_SCHEMA_VERSION
                ),
            ));
        }

        for (map_id, node) in &self.nodes {
            if map_id != &node.id {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.node_key_mismatch",
                    format!("node map key {map_id} does not match node id {}", node.id),
                ));
            }
            match registry.get(&node.type_id) {
                None => diagnostics.push(GraphDiagnostic::error(
                    "graph.unknown_node_type",
                    format!("node {} uses unknown type {}", node.id, node.type_id),
                )),
                Some(definition) => {
                    if definition.domain != self.domain {
                        diagnostics.push(GraphDiagnostic::error(
                            "graph.wrong_domain",
                            format!(
                                "node {} belongs to {:?}, not {:?}",
                                node.id, definition.domain, self.domain
                            ),
                        ));
                    }
                    if node.type_version > definition.version {
                        diagnostics.push(GraphDiagnostic::error(
                            "graph.newer_node_version",
                            format!(
                                "node {} version {} is newer than registered version {}",
                                node.id, node.type_version, definition.version
                            ),
                        ));
                    } else if node.type_version < definition.version {
                        diagnostics.push(GraphDiagnostic::warning(
                            "graph.node_migration_required",
                            format!(
                                "node {} version {} requires migration to {}",
                                node.id, node.type_version, definition.version
                            ),
                        ));
                    }
                }
            }
        }

        let mut target_counts = BTreeMap::<PortRef, usize>::new();
        for connection in &self.connections {
            let Some(from_node) = self.nodes.get(&connection.from.node_id) else {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.missing_source_node",
                    format!(
                        "connection source node {} does not exist",
                        connection.from.node_id
                    ),
                ));
                continue;
            };
            let Some(to_node) = self.nodes.get(&connection.to.node_id) else {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.missing_target_node",
                    format!(
                        "connection target node {} does not exist",
                        connection.to.node_id
                    ),
                ));
                continue;
            };
            let Some(from_definition) = registry.get(&from_node.type_id) else {
                continue;
            };
            let Some(to_definition) = registry.get(&to_node.type_id) else {
                continue;
            };
            let Some(output) = from_definition.outputs.get(&connection.from.port_id) else {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.missing_output",
                    format!(
                        "node {} has no output {}",
                        from_node.id, connection.from.port_id
                    ),
                ));
                continue;
            };
            let Some(input) = to_definition.inputs.get(&connection.to.port_id) else {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.missing_input",
                    format!("node {} has no input {}", to_node.id, connection.to.port_id),
                ));
                continue;
            };
            if output.port_type != input.port_type {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.port_type_mismatch",
                    format!(
                        "cannot connect {:?} output {} to {:?} input {}",
                        output.port_type,
                        connection.from.port_id,
                        input.port_type,
                        connection.to.port_id
                    ),
                ));
            }
            let count = target_counts.entry(connection.to.clone()).or_default();
            *count += 1;
            if *count > 1 && !input.multiple {
                diagnostics.push(GraphDiagnostic::error(
                    "graph.input_has_multiple_sources",
                    format!(
                        "input {} on node {} accepts only one connection",
                        connection.to.port_id, connection.to.node_id
                    ),
                ));
            }
        }

        diagnostics
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
}

impl GraphDiagnostic {
    fn warning(code: &'static str, message: String) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code,
            message,
        }
    }

    fn error(code: &'static str, message: String) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> StableId {
        StableId::new(value).unwrap()
    }

    fn source_definition() -> NodeDefinition {
        NodeDefinition::new(
            id("test.signal_source"),
            1,
            "Signal Source",
            "Tests",
            GraphDomain::Behavior,
        )
        .output(PortDefinition::new(id("fired"), "Fired", PortType::Signal))
    }

    fn target_definition() -> NodeDefinition {
        NodeDefinition::new(
            id("test.signal_target"),
            2,
            "Signal Target",
            "Tests",
            GraphDomain::Behavior,
        )
        .input(PortDefinition::new(id("run"), "Run", PortType::Signal))
    }

    #[test]
    fn stable_ids_reject_display_text_and_accept_namespaced_ids() {
        assert!(StableId::new("heavy_water.city/shop-door").is_ok());
        assert!(StableId::new("Shop Door").is_err());
        assert!(StableId::new("").is_err());
        assert!(serde_json::from_str::<StableId>(r#""invalid id""#).is_err());
    }

    #[test]
    fn valid_typed_graph_has_no_diagnostics() {
        let mut registry = NodeRegistry::default();
        registry.register_definition(source_definition()).unwrap();
        registry.register_definition(target_definition()).unwrap();

        let source_id = id("source");
        let target_id = id("target");
        let mut graph = GraphDocument::new(id("test.behavior"), GraphDomain::Behavior);
        graph.nodes.insert(
            source_id.clone(),
            GraphNode {
                id: source_id.clone(),
                type_id: id("test.signal_source"),
                type_version: 1,
                properties: BTreeMap::new(),
                position: [0.0, 0.0],
            },
        );
        graph.nodes.insert(
            target_id.clone(),
            GraphNode {
                id: target_id.clone(),
                type_id: id("test.signal_target"),
                type_version: 2,
                properties: BTreeMap::new(),
                position: [200.0, 0.0],
            },
        );
        graph.connections.insert(GraphConnection {
            from: PortRef {
                node_id: source_id,
                port_id: id("fired"),
            },
            to: PortRef {
                node_id: target_id,
                port_id: id("run"),
            },
        });

        assert!(graph.validate(&registry).is_empty());
    }

    #[test]
    fn validation_reports_unknown_nodes_wrong_ports_and_migrations() {
        let mut registry = NodeRegistry::default();
        registry.register_definition(source_definition()).unwrap();
        registry.register_definition(target_definition()).unwrap();

        let source_id = id("source");
        let target_id = id("target");
        let unknown_id = id("unknown");
        let mut graph = GraphDocument::new(id("test.behavior"), GraphDomain::Behavior);
        for (node_id, type_id, version) in [
            (source_id.clone(), id("test.signal_source"), 1),
            (target_id.clone(), id("test.signal_target"), 1),
            (unknown_id.clone(), id("third_party.missing"), 1),
        ] {
            graph.nodes.insert(
                node_id.clone(),
                GraphNode {
                    id: node_id,
                    type_id,
                    type_version: version,
                    properties: BTreeMap::new(),
                    position: [0.0, 0.0],
                },
            );
        }
        graph.connections.insert(GraphConnection {
            from: PortRef {
                node_id: source_id,
                port_id: id("not_an_output"),
            },
            to: PortRef {
                node_id: target_id,
                port_id: id("run"),
            },
        });

        let codes = graph
            .validate(&registry)
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("graph.unknown_node_type"));
        assert!(codes.contains("graph.node_migration_required"));
        assert!(codes.contains("graph.missing_output"));
    }

    #[test]
    fn node_registry_rejects_duplicate_native_type_ids() {
        let mut registry = NodeRegistry::default();
        registry.register_definition(source_definition()).unwrap();
        assert!(matches!(
            registry.register_definition(source_definition()),
            Err(NodeRegistryError::DuplicateType(_))
        ));
    }

    #[test]
    fn graph_documents_round_trip_through_json() {
        let graph = GraphDocument::new(id("test.serialized_graph"), GraphDomain::Object);
        let encoded = serde_json::to_string_pretty(&graph).unwrap();
        assert_eq!(
            serde_json::from_str::<GraphDocument>(&encoded).unwrap(),
            graph
        );
    }
}
