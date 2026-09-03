//! Durable Forge records for typed platformer route graphs.
//!
//! Route graphs temporarily live in `Ui` payloads for project-schema
//! compatibility, following the same typed-marker pattern as dialogue graphs.
//! Publishing compiles these sources into portable route documents; Game
//! builds load only that compiled output.

use starfall_graph::{GraphDocument, NodeRegistry, StableId};
use starfall_platformer_graph::{
    compile_platformer_graph, register_platformer_nodes, PlatformerRouteDocument,
    PlatformerRouteGraphBuilder,
};

use super::persistence::{
    ContentCategory, ContentPayload, ForgeProject, GenericRecipeDraft, CURRENT_PROJECT_SCHEMA,
};

const GRAPH_FIELD: &str = "platformer_route_graph";
const RECORD_KIND_FIELD: &str = "record_kind";
const RECORD_KIND: &str = "starfall_platformer_route_graph_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformerRouteRecordError {
    MissingRecord(String),
    WrongRecordKind(String),
    InvalidGraph(Vec<String>),
    Serialization(String),
}

/// Creates a route record with a deterministic linear graph.
pub fn create_platformer_route(
    project: &mut ForgeProject,
    display_name: &str,
    route_id: StableId,
    theme: StableId,
    chunks: impl IntoIterator<Item = StableId>,
) -> Result<String, PlatformerRouteRecordError> {
    let content_id = project
        .create_content(ContentCategory::Ui, display_name)
        .map_err(|error| PlatformerRouteRecordError::Serialization(format!("{error:?}")))?;
    let mut builder = PlatformerRouteGraphBuilder::new(route_id, display_name, theme);
    for (index, chunk_id) in chunks.into_iter().enumerate() {
        builder = builder.chunk(
            StableId::new(format!("chunk_{index}"))
                .expect("generated route node identities are valid"),
            chunk_id,
        );
    }
    save_platformer_route(project, &content_id, &builder.build())?;
    if let Some(record) = project
        .records
        .iter_mut()
        .find(|record| record.content_id == content_id)
    {
        record.tags = vec!["platformer".into(), "route".into(), "graph".into()];
    }
    Ok(content_id)
}

pub fn save_platformer_route(
    project: &mut ForgeProject,
    content_id: &str,
    graph: &GraphDocument,
) -> Result<(), PlatformerRouteRecordError> {
    compile_document(graph)?;
    let Some(ContentPayload::Ui(recipe)) = project.payloads.get_mut(content_id) else {
        return Err(PlatformerRouteRecordError::MissingRecord(content_id.into()));
    };
    recipe.schema_version = CURRENT_PROJECT_SCHEMA;
    recipe
        .fields
        .insert(RECORD_KIND_FIELD.into(), serde_json::json!(RECORD_KIND));
    recipe.fields.insert(
        GRAPH_FIELD.into(),
        serde_json::to_value(graph)
            .map_err(|error| PlatformerRouteRecordError::Serialization(error.to_string()))?,
    );
    Ok(())
}

pub fn load_platformer_route(
    project: &ForgeProject,
    content_id: &str,
) -> Result<GraphDocument, PlatformerRouteRecordError> {
    let Some(ContentPayload::Ui(GenericRecipeDraft { fields, .. })) =
        project.payloads.get(content_id)
    else {
        return Err(PlatformerRouteRecordError::MissingRecord(content_id.into()));
    };
    if fields
        .get(RECORD_KIND_FIELD)
        .and_then(serde_json::Value::as_str)
        != Some(RECORD_KIND)
    {
        return Err(PlatformerRouteRecordError::WrongRecordKind(
            content_id.into(),
        ));
    }
    let graph: GraphDocument = serde_json::from_value(
        fields
            .get(GRAPH_FIELD)
            .cloned()
            .ok_or_else(|| PlatformerRouteRecordError::WrongRecordKind(content_id.into()))?,
    )
    .map_err(|error| PlatformerRouteRecordError::Serialization(error.to_string()))?;
    compile_document(&graph)?;
    Ok(graph)
}

pub fn compile_record(
    project: &ForgeProject,
    content_id: &str,
) -> Result<PlatformerRouteDocument, PlatformerRouteRecordError> {
    compile_document(&load_platformer_route(project, content_id)?)
}

fn compile_document(
    graph: &GraphDocument,
) -> Result<PlatformerRouteDocument, PlatformerRouteRecordError> {
    let mut registry = NodeRegistry::default();
    register_platformer_nodes(&mut registry)
        .map_err(|error| PlatformerRouteRecordError::Serialization(error.to_string()))?;
    compile_platformer_graph(graph, &registry)
        .map(|compiled| compiled.document)
        .map_err(|error| {
            PlatformerRouteRecordError::InvalidGraph(
                error
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect(),
            )
        })
}

fn is_platformer_route_record(
    project: &ForgeProject,
    record: &super::persistence::ContentRecord,
) -> bool {
    record.category == ContentCategory::Ui
        && matches!(
            project.payloads.get(&record.content_id),
            Some(ContentPayload::Ui(recipe))
                if recipe.fields.get(RECORD_KIND_FIELD).and_then(serde_json::Value::as_str)
                    == Some(RECORD_KIND)
        )
}

/// Every route record as `(content_id, display_name)`, sorted for publishing.
pub fn platformer_route_entries(project: &ForgeProject) -> Vec<(String, String)> {
    let mut entries = project
        .records
        .iter()
        .filter(|record| is_platformer_route_record(project, record))
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

pub fn validate_platformer_route_records(project: &ForgeProject) -> Vec<String> {
    platformer_route_entries(project)
        .into_iter()
        .filter_map(|(content_id, _)| {
            compile_record(project, &content_id)
                .err()
                .map(|error| format!("{content_id}: {error:?}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> StableId {
        StableId::new(value).unwrap()
    }

    #[test]
    fn route_records_round_trip_and_compile_to_owned_documents() {
        let mut project = ForgeProject::default();
        let content_id = create_platformer_route(
            &mut project,
            "Rooftop Run",
            id("route_city_rooftops"),
            id("heavy_water.theme.cityscape"),
            [id("city_rooftop_arrival"), id("city_plaza_arena")],
        )
        .unwrap();

        let graph = load_platformer_route(&project, &content_id).unwrap();
        assert_eq!(graph.id.as_str(), "route_city_rooftops");
        let document = compile_record(&project, &content_id).unwrap();
        assert_eq!(document.chunks.len(), 2);
        assert_eq!(document.label, "Rooftop Run");
        assert_eq!(platformer_route_entries(&project).len(), 1);
        assert!(validate_platformer_route_records(&project).is_empty());
    }

    #[test]
    fn record_rename_does_not_change_the_runtime_route_identity() {
        let mut project = ForgeProject::default();
        let content_id = create_platformer_route(
            &mut project,
            "Route",
            id("my_game.stable_route"),
            id("theme.test"),
            [id("chunk.arrival"), id("chunk.arena")],
        )
        .unwrap();
        project
            .rename_content(&content_id, "ui.renamed_route_record")
            .unwrap();
        let document = compile_record(&project, "ui.renamed_route_record").unwrap();
        assert_eq!(document.id.as_str(), "my_game.stable_route");
    }
}
