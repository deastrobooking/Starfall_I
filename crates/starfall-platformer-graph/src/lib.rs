//! Typed authoring graphs and compiled documents for platformer routes.
//!
//! This is an adapter crate by design: [`starfall_graph`] stays domain-neutral,
//! while [`starfall_platformer`] stays independent of any authoring UI or file
//! format. Games only pay for this layer when they want graph-authored routes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use starfall_graph::{
    DiagnosticSeverity, GraphConnection, GraphDiagnostic, GraphDocument, GraphDomain, GraphNode,
    GraphValue, NativeNode, NodeDefinition, NodeRegistry, NodeRegistryError, PortDefinition,
    PortRef, PortType, StableId,
};
use starfall_platformer::{
    assemble_route, validate_resolved_route, ChunkDef, JumpEnvelope, PlacedChunk, RouteProblem,
};

/// Current schema for the compiled, portable route document.
pub const PLATFORMER_ROUTE_SCHEMA_VERSION: u32 = 1;
pub const ROUTE_START_NODE: &str = "starfall.platformer.route_start";
pub const CHUNK_NODE: &str = "starfall.platformer.chunk";
pub const ROUTE_END_NODE: &str = "starfall.platformer.route_end";

const PREVIOUS_PORT: &str = "previous";
const NEXT_PORT: &str = "next";
const LABEL_PROPERTY: &str = "label";
const BRIEF_PROPERTY: &str = "brief";
const THEME_PROPERTY: &str = "theme";
const CHUNK_ID_PROPERTY: &str = "chunk_id";

/// Fluent construction for code generators, tests, and games that want the
/// same graph shape Forge emits without assembling raw node property maps.
#[derive(Debug, Clone)]
pub struct PlatformerRouteGraphBuilder {
    graph: GraphDocument,
    previous: StableId,
    next_x: f32,
}

impl PlatformerRouteGraphBuilder {
    pub fn new(route_id: StableId, label: impl Into<String>, theme: StableId) -> Self {
        let mut graph = GraphDocument::new(route_id, GraphDomain::Platformer);
        let start_id = stable_id("start");
        let mut properties = BTreeMap::new();
        properties.insert(stable_id(LABEL_PROPERTY), GraphValue::String(label.into()));
        properties.insert(stable_id(THEME_PROPERTY), GraphValue::StableRef(theme));
        graph.nodes.insert(
            start_id.clone(),
            GraphNode {
                id: start_id.clone(),
                type_id: stable_id(ROUTE_START_NODE),
                type_version: 1,
                properties,
                position: [0.0, 0.0],
            },
        );
        Self {
            graph,
            previous: start_id,
            next_x: 240.0,
        }
    }

    pub fn brief(mut self, brief: impl Into<String>) -> Self {
        self.graph
            .nodes
            .get_mut(&stable_id("start"))
            .expect("builder always owns a start node")
            .properties
            .insert(stable_id(BRIEF_PROPERTY), GraphValue::String(brief.into()));
        self
    }

    pub fn chunk(mut self, node_id: StableId, chunk_id: StableId) -> Self {
        let node = GraphNode {
            id: node_id.clone(),
            type_id: stable_id(CHUNK_NODE),
            type_version: 1,
            properties: BTreeMap::from([(
                stable_id(CHUNK_ID_PROPERTY),
                GraphValue::StableRef(chunk_id),
            )]),
            position: [self.next_x, 0.0],
        };
        self.graph.nodes.insert(node_id.clone(), node);
        connect_nodes(&mut self.graph, self.previous, node_id.clone());
        self.previous = node_id;
        self.next_x += 240.0;
        self
    }

    pub fn build(mut self) -> GraphDocument {
        let end_id = stable_id("end");
        self.graph.nodes.insert(
            end_id.clone(),
            GraphNode {
                id: end_id.clone(),
                type_id: stable_id(ROUTE_END_NODE),
                type_version: 1,
                properties: BTreeMap::new(),
                position: [self.next_x, 0.0],
            },
        );
        connect_nodes(&mut self.graph, self.previous, end_id);
        self.graph
    }
}

/// Owned, versioned route data produced by a graph compiler and safe to store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformerRouteDocument {
    pub schema_version: u32,
    pub id: StableId,
    pub label: String,
    #[serde(default)]
    pub brief: String,
    pub theme: StableId,
    pub chunks: Vec<StableId>,
}

impl PlatformerRouteDocument {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn parse(source: &str) -> Result<Self, PlatformerRouteLoadError> {
        let document: Self = serde_json::from_str(source)?;
        document.validate_schema()?;
        Ok(document)
    }

    pub fn validate_schema(&self) -> Result<(), PlatformerRouteLoadError> {
        if self.schema_version != PLATFORMER_ROUTE_SCHEMA_VERSION {
            return Err(PlatformerRouteLoadError::UnsupportedSchema {
                found: self.schema_version,
                supported: PLATFORMER_ROUTE_SCHEMA_VERSION,
            });
        }
        Ok(())
    }

    /// Resolves game-owned chunk IDs and runs the reusable geometry checks.
    pub fn compile_runtime(
        &self,
        start: [f32; 3],
        envelope: JumpEnvelope,
        mut resolve: impl FnMut(&str) -> Option<ChunkDef>,
    ) -> Result<CompiledPlatformerRoute, PlatformerRuntimeCompileError> {
        let mut definitions = Vec::with_capacity(self.chunks.len());
        for chunk_id in &self.chunks {
            definitions
                .push(resolve(chunk_id.as_str()).ok_or_else(|| {
                    PlatformerRuntimeCompileError::UnknownChunk(chunk_id.clone())
                })?);
        }

        let problems = validate_resolved_route(&definitions, envelope);
        if !problems.is_empty() {
            return Err(PlatformerRuntimeCompileError::InvalidRoute(problems));
        }

        Ok(CompiledPlatformerRoute {
            source: self.clone(),
            chunks: assemble_route(&definitions, start.into()),
        })
    }
}

/// Runtime-ready output. Geometry remains renderer- and ECS-neutral.
#[derive(Debug, Clone)]
pub struct CompiledPlatformerRoute {
    pub source: PlatformerRouteDocument,
    pub chunks: Vec<PlacedChunk>,
}

#[derive(Debug)]
pub enum PlatformerRouteLoadError {
    Json(serde_json::Error),
    UnsupportedSchema { found: u32, supported: u32 },
}

impl fmt::Display for PlatformerRouteLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid platformer route JSON: {error}"),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "platformer route schema {found} is not supported; this build supports schema {supported}"
            ),
        }
    }
}

impl std::error::Error for PlatformerRouteLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::UnsupportedSchema { .. } => None,
        }
    }
}

impl From<serde_json::Error> for PlatformerRouteLoadError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformerRuntimeCompileError {
    UnknownChunk(StableId),
    InvalidRoute(Vec<RouteProblem>),
}

impl fmt::Display for PlatformerRuntimeCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChunk(id) => write!(formatter, "chunk catalog has no entry for {id}"),
            Self::InvalidRoute(problems) => write!(
                formatter,
                "route failed runtime validation: {}",
                problems
                    .iter()
                    .map(RouteProblem::message)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
}

impl std::error::Error for PlatformerRuntimeCompileError {}

/// Successful graph compilation plus non-fatal migration warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformerRouteGraph {
    pub document: PlatformerRouteDocument,
    pub warnings: Vec<GraphDiagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlatformerGraphCompileError {
    pub diagnostics: Vec<GraphDiagnostic>,
}

impl fmt::Display for PlatformerGraphCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "platformer graph did not compile: {}",
            self.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

impl std::error::Error for PlatformerGraphCompileError {}

pub struct RouteStartNode;

impl NativeNode for RouteStartNode {
    fn definition() -> NodeDefinition {
        NodeDefinition::new(
            stable_id(ROUTE_START_NODE),
            1,
            "Route Start",
            "Platformer/Route",
            GraphDomain::Platformer,
        )
        .output(signal_port(NEXT_PORT, "Next"))
    }
}

pub struct ChunkNode;

impl NativeNode for ChunkNode {
    fn definition() -> NodeDefinition {
        NodeDefinition::new(
            stable_id(CHUNK_NODE),
            1,
            "Chunk",
            "Platformer/Route",
            GraphDomain::Platformer,
        )
        .input(signal_port(PREVIOUS_PORT, "Previous"))
        .output(signal_port(NEXT_PORT, "Next"))
    }
}

pub struct RouteEndNode;

impl NativeNode for RouteEndNode {
    fn definition() -> NodeDefinition {
        NodeDefinition::new(
            stable_id(ROUTE_END_NODE),
            1,
            "Route End",
            "Platformer/Route",
            GraphDomain::Platformer,
        )
        .input(signal_port(PREVIOUS_PORT, "Previous"))
    }
}

/// Installs every built-in platformer route node into a shared registry.
pub fn register_platformer_nodes(registry: &mut NodeRegistry) -> Result<(), NodeRegistryError> {
    registry.register::<RouteStartNode>()?;
    registry.register::<ChunkNode>()?;
    registry.register::<RouteEndNode>()?;
    Ok(())
}

/// Compiles one linear typed graph into portable route data.
pub fn compile_platformer_graph(
    graph: &GraphDocument,
    registry: &NodeRegistry,
) -> Result<PlatformerRouteGraph, PlatformerGraphCompileError> {
    let mut diagnostics = graph.validate(registry);
    if graph.domain != GraphDomain::Platformer {
        diagnostics.push(error(
            "platformer.wrong_graph_domain",
            format!("expected Platformer graph, found {:?}", graph.domain),
        ));
    }

    let starts = nodes_of_type(graph, ROUTE_START_NODE);
    let ends = nodes_of_type(graph, ROUTE_END_NODE);
    if starts.len() != 1 {
        diagnostics.push(error(
            "platformer.route_start_count",
            format!(
                "route requires exactly one start node, found {}",
                starts.len()
            ),
        ));
    }
    if ends.len() != 1 {
        diagnostics.push(error(
            "platformer.route_end_count",
            format!("route requires exactly one end node, found {}", ends.len()),
        ));
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(PlatformerGraphCompileError { diagnostics });
    }

    let start = starts[0];
    let end = ends[0];
    let label = string_property(start, LABEL_PROPERTY, &mut diagnostics);
    let brief = optional_string_property(start, BRIEF_PROPERTY, &mut diagnostics);
    let theme = stable_ref_property(start, THEME_PROPERTY, &mut diagnostics);

    let adjacency = route_adjacency(graph, &mut diagnostics);
    let mut visited = BTreeSet::new();
    let mut chunk_ids = Vec::new();
    let mut current = start.id.clone();
    loop {
        if !visited.insert(current.clone()) {
            diagnostics.push(error(
                "platformer.route_cycle",
                format!("route revisits node {current}"),
            ));
            break;
        }
        let Some(next_nodes) = adjacency.get(&current) else {
            diagnostics.push(error(
                "platformer.route_dead_end",
                format!("route stops at node {current} before its end"),
            ));
            break;
        };
        if next_nodes.len() != 1 {
            diagnostics.push(error(
                "platformer.route_branch",
                format!(
                    "node {current} has {} outgoing route links",
                    next_nodes.len()
                ),
            ));
            break;
        }
        let next_id = &next_nodes[0];
        let next = &graph.nodes[next_id];
        if next.id == end.id {
            visited.insert(next.id.clone());
            break;
        }
        if next.type_id.as_str() != CHUNK_NODE {
            diagnostics.push(error(
                "platformer.unexpected_route_node",
                format!("node {} cannot appear inside a route", next.id),
            ));
            break;
        }
        if let Some(chunk_id) = stable_ref_property(next, CHUNK_ID_PROPERTY, &mut diagnostics) {
            chunk_ids.push(chunk_id);
        }
        current = next.id.clone();
    }

    let route_nodes = graph
        .nodes
        .values()
        .filter(|node| {
            matches!(
                node.type_id.as_str(),
                ROUTE_START_NODE | CHUNK_NODE | ROUTE_END_NODE
            )
        })
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let disconnected = route_nodes
        .difference(&visited)
        .cloned()
        .collect::<Vec<_>>();
    if !disconnected.is_empty() {
        diagnostics.push(error(
            "platformer.disconnected_route_nodes",
            format!(
                "route nodes are disconnected from the start flow: {}",
                disconnected
                    .iter()
                    .map(StableId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(PlatformerGraphCompileError { diagnostics });
    }

    Ok(PlatformerRouteGraph {
        document: PlatformerRouteDocument {
            schema_version: PLATFORMER_ROUTE_SCHEMA_VERSION,
            id: graph.id.clone(),
            label: label.expect("required property was diagnosed"),
            brief: brief.unwrap_or_default(),
            theme: theme.expect("required property was diagnosed"),
            chunks: chunk_ids,
        },
        warnings: diagnostics,
    })
}

fn stable_id(value: &str) -> StableId {
    StableId::new(value).expect("built-in platformer graph IDs must be valid")
}

fn connect_nodes(graph: &mut GraphDocument, from: StableId, to: StableId) {
    graph.connections.insert(GraphConnection {
        from: PortRef {
            node_id: from,
            port_id: stable_id(NEXT_PORT),
        },
        to: PortRef {
            node_id: to,
            port_id: stable_id(PREVIOUS_PORT),
        },
    });
}

fn signal_port(id: &str, label: &str) -> PortDefinition {
    PortDefinition::new(stable_id(id), label, PortType::Signal)
}

fn nodes_of_type<'a>(graph: &'a GraphDocument, type_id: &str) -> Vec<&'a GraphNode> {
    graph
        .nodes
        .values()
        .filter(|node| node.type_id.as_str() == type_id)
        .collect()
}

fn route_adjacency(
    graph: &GraphDocument,
    diagnostics: &mut Vec<GraphDiagnostic>,
) -> BTreeMap<StableId, Vec<StableId>> {
    let mut adjacency = BTreeMap::<StableId, Vec<StableId>>::new();
    for connection in &graph.connections {
        if connection.from.port_id.as_str() != NEXT_PORT
            || connection.to.port_id.as_str() != PREVIOUS_PORT
        {
            continue;
        }
        let targets = adjacency
            .entry(connection.from.node_id.clone())
            .or_default();
        targets.push(connection.to.node_id.clone());
    }
    for targets in adjacency.values_mut() {
        targets.sort();
        targets.dedup();
    }
    if graph.connections.is_empty() {
        diagnostics.push(error(
            "platformer.route_has_no_connections",
            "route graph has no flow connections".into(),
        ));
    }
    adjacency
}

fn string_property(
    node: &GraphNode,
    property: &str,
    diagnostics: &mut Vec<GraphDiagnostic>,
) -> Option<String> {
    match node.properties.get(&stable_id(property)) {
        Some(GraphValue::String(value)) if !value.trim().is_empty() => Some(value.clone()),
        Some(_) => {
            diagnostics.push(error(
                "platformer.invalid_property_type",
                format!(
                    "node {} property {property} must be a non-empty string",
                    node.id
                ),
            ));
            None
        }
        None => {
            diagnostics.push(error(
                "platformer.missing_property",
                format!("node {} requires property {property}", node.id),
            ));
            None
        }
    }
}

fn optional_string_property(
    node: &GraphNode,
    property: &str,
    diagnostics: &mut Vec<GraphDiagnostic>,
) -> Option<String> {
    match node.properties.get(&stable_id(property)) {
        Some(GraphValue::String(value)) => Some(value.clone()),
        Some(_) => {
            diagnostics.push(error(
                "platformer.invalid_property_type",
                format!("node {} property {property} must be a string", node.id),
            ));
            None
        }
        None => None,
    }
}

fn stable_ref_property(
    node: &GraphNode,
    property: &str,
    diagnostics: &mut Vec<GraphDiagnostic>,
) -> Option<StableId> {
    match node.properties.get(&stable_id(property)) {
        Some(GraphValue::StableRef(value)) => Some(value.clone()),
        Some(_) => {
            diagnostics.push(error(
                "platformer.invalid_property_type",
                format!(
                    "node {} property {property} must be a stable reference",
                    node.id
                ),
            ));
            None
        }
        None => {
            diagnostics.push(error(
                "platformer.missing_property",
                format!("node {} requires property {property}", node.id),
            ));
            None
        }
    }
}

fn error(code: &'static str, message: String) -> GraphDiagnostic {
    GraphDiagnostic {
        severity: DiagnosticSeverity::Error,
        code,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::Vec3;
    use starfall_platformer::{ChunkPiece, ChunkRole, ChunkSocket, ChunkTheme};

    fn id(value: &str) -> StableId {
        StableId::new(value).unwrap()
    }

    fn node(node_id: &str, type_id: &str) -> GraphNode {
        GraphNode {
            id: id(node_id),
            type_id: id(type_id),
            type_version: 1,
            properties: BTreeMap::new(),
            position: [0.0, 0.0],
        }
    }

    fn connect(graph: &mut GraphDocument, from: &str, to: &str) {
        graph.connections.insert(GraphConnection {
            from: PortRef {
                node_id: id(from),
                port_id: id(NEXT_PORT),
            },
            to: PortRef {
                node_id: id(to),
                port_id: id(PREVIOUS_PORT),
            },
        });
    }

    fn example_graph() -> (GraphDocument, NodeRegistry) {
        let mut registry = NodeRegistry::default();
        register_platformer_nodes(&mut registry).unwrap();
        let mut graph = GraphDocument::new(id("heavy_water.demo_route"), GraphDomain::Platformer);

        let mut start = node("start", ROUTE_START_NODE);
        start
            .properties
            .insert(id(LABEL_PROPERTY), GraphValue::String("Demo Route".into()));
        start.properties.insert(
            id(BRIEF_PROPERTY),
            GraphValue::String("A portable authored route".into()),
        );
        start
            .properties
            .insert(id(THEME_PROPERTY), GraphValue::StableRef(id("cityscape")));
        graph.nodes.insert(start.id.clone(), start);

        for (node_id, chunk_id) in [
            ("arrival", "heavy_water.arrival"),
            ("arena", "heavy_water.arena"),
        ] {
            let mut chunk = node(node_id, CHUNK_NODE);
            chunk
                .properties
                .insert(id(CHUNK_ID_PROPERTY), GraphValue::StableRef(id(chunk_id)));
            graph.nodes.insert(chunk.id.clone(), chunk);
        }
        let end = node("end", ROUTE_END_NODE);
        graph.nodes.insert(end.id.clone(), end);
        connect(&mut graph, "start", "arrival");
        connect(&mut graph, "arrival", "arena");
        connect(&mut graph, "arena", "end");
        (graph, registry)
    }

    fn catalog(id: &str) -> Option<ChunkDef> {
        let role = match id {
            "heavy_water.arrival" => ChunkRole::Arrival,
            "heavy_water.arena" => ChunkRole::Arena,
            _ => return None,
        };
        Some(ChunkDef {
            id: if role == ChunkRole::Arrival {
                "heavy_water.arrival"
            } else {
                "heavy_water.arena"
            },
            label: "Test Chunk",
            theme: ChunkTheme::Cityscape,
            role,
            entry: ChunkSocket::new(-15.0, 0.5, 0.0),
            exit: ChunkSocket::new(15.0, 0.5, 0.0),
            pieces: vec![ChunkPiece::Solid {
                center: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(30.0, 1.0, 8.0),
            }],
        })
    }

    #[test]
    fn graph_compiles_round_trips_and_resolves_to_runtime_geometry() {
        let (graph, registry) = example_graph();
        let compiled = compile_platformer_graph(&graph, &registry).unwrap();
        assert!(compiled.warnings.is_empty());
        assert_eq!(compiled.document.chunks.len(), 2);

        let json = compiled.document.to_json_pretty().unwrap();
        let loaded = PlatformerRouteDocument::parse(&json).unwrap();
        assert_eq!(loaded, compiled.document);

        let runtime = loaded
            .compile_runtime([10.0, 2.0, -3.0], JumpEnvelope::standard(), catalog)
            .unwrap();
        assert_eq!(runtime.chunks.len(), 2);
        assert_eq!(
            runtime.chunks[0].world_exit(),
            runtime.chunks[1].world_entry()
        );
    }

    #[test]
    fn compiler_rejects_branches_and_disconnected_route_nodes() {
        let (mut graph, registry) = example_graph();
        let mut extra = node("extra", CHUNK_NODE);
        extra.properties.insert(
            id(CHUNK_ID_PROPERTY),
            GraphValue::StableRef(id("heavy_water.arena")),
        );
        graph.nodes.insert(extra.id.clone(), extra);
        connect(&mut graph, "start", "extra");

        let codes = compile_platformer_graph(&graph, &registry)
            .unwrap_err()
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("platformer.route_branch"));
        assert!(codes.contains("platformer.disconnected_route_nodes"));
    }

    #[test]
    fn runtime_compile_reports_unknown_catalog_entries() {
        let (graph, registry) = example_graph();
        let route = compile_platformer_graph(&graph, &registry)
            .unwrap()
            .document;
        assert!(matches!(
            route.compile_runtime([0.0; 3], JumpEnvelope::standard(), |_| None),
            Err(PlatformerRuntimeCompileError::UnknownChunk(_))
        ));
    }

    #[test]
    fn unsupported_route_document_schemas_are_rejected() {
        let (graph, registry) = example_graph();
        let mut route = compile_platformer_graph(&graph, &registry)
            .unwrap()
            .document;
        route.schema_version = PLATFORMER_ROUTE_SCHEMA_VERSION + 1;
        let json = route.to_json_pretty().unwrap();
        assert!(matches!(
            PlatformerRouteDocument::parse(&json),
            Err(PlatformerRouteLoadError::UnsupportedSchema { .. })
        ));

        route.schema_version = PLATFORMER_ROUTE_SCHEMA_VERSION - 1;
        let json = route.to_json_pretty().unwrap();
        assert!(matches!(
            PlatformerRouteDocument::parse(&json),
            Err(PlatformerRouteLoadError::UnsupportedSchema { .. })
        ));
    }
}
