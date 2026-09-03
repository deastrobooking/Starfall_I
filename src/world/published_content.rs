//! Game-side loader for Designer-published content (docs/PROJECT_PLAN.md P1).
//!
//! Resolves the generation manifest in `assets/published/`, reads its plain
//! JSON, and makes it playable: published weapons become resolvable blades and
//! shop stock, creatures feed encounter systems, and vehicle/spacecraft recipes
//! become stable runtime catalogs for gameplay adapters.
//! This is the *read* half of the pipeline and ships in **both** editions — the
//! consumer Game edition has no writer, only this.
//!
//! A fresh install with no published content is a first-class state: missing
//! files mean empty sets, never errors.

use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use std::path::PathBuf;

use crate::engine_tools::publish::{
    published_dir, published_generation_in, PublishedDialogue, PublishedGenerationSnapshot,
    PublishedSpaceship, PublishedVehicle, PublishedWeapon,
};
use crate::engine_tools::{PublishedCreatureCatalog, PublishedDialogueCatalog};
use crate::resources::{ShopCatalog, ShopCategory, ShopItem};
use crate::robots::creature::CreatureSpec;
use crate::spaceship_forge::PublishedSpacecraftCatalog;
use crate::vehicle_forge::PublishedVehicleCatalog;
use crate::world::platformer_chunk_library::chunk_by_id;
use crate::world::platformer_routes::PublishedPlatformerRouteCatalog;

pub struct PublishedContentPlugin;

/// Runtime-resolved published-content directory. Tests, portable packages,
/// and launchers can inject a root without depending on the source checkout.
#[derive(Resource, Debug, Clone)]
pub struct PublishedContentRoot(pub PathBuf);

impl Default for PublishedContentRoot {
    fn default() -> Self {
        Self(published_dir())
    }
}

/// Same-process publisher/editor bridge. The immutable generation manifest is
/// resolved once when handled, so every reloadable catalog advances together.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct ReloadPublishedContentCatalogs;

/// One-shot content load before Bevy's initial state transition. Direct-to-
/// Playing boots can therefore resolve authored craft during their first
/// gameplay reconciliation instead of racing the ordinary Startup schedule.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublishedContentBootstrap;

impl Plugin for PublishedContentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PublishedVehicleCatalog>()
            .init_resource::<PublishedSpacecraftCatalog>()
            .init_resource::<PublishedPlatformerRouteCatalog>()
            .init_resource::<PublishedContentRoot>()
            .add_message::<ReloadPublishedContentCatalogs>()
            .add_systems(PublishedContentBootstrap, load_published_content)
            .add_systems(Update, reload_published_content_catalogs);
        app.world_mut()
            .resource_mut::<MainScheduleOrder>()
            .insert_startup_before(StateTransition, PublishedContentBootstrap);
    }
}

fn reload_published_content_catalogs(
    mut requests: MessageReader<ReloadPublishedContentCatalogs>,
    root: Res<PublishedContentRoot>,
    mut vehicle_catalog: ResMut<PublishedVehicleCatalog>,
    mut spacecraft_catalog: ResMut<PublishedSpacecraftCatalog>,
    mut platformer_route_catalog: ResMut<PublishedPlatformerRouteCatalog>,
) {
    if requests.read().next().is_none() {
        return;
    }
    let snapshot = match published_generation_in(&root.0) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!("published craft reload failed: {error}; keeping current catalogs");
            return;
        }
    };
    let vehicles = read_published::<PublishedVehicle>(&snapshot, "vehicles.json")
        .into_iter()
        .filter_map(|published| {
            (published.spec.content_id == published.content_id)
                .then_some(published.spec)
                .or_else(|| {
                    warn!(
                        "published vehicle {} embeds a mismatched id; ignoring",
                        published.content_id
                    );
                    None
                })
        })
        .collect::<Vec<_>>();
    let spacecraft = read_published::<PublishedSpaceship>(&snapshot, "spaceships.json")
        .into_iter()
        .filter_map(|published| {
            (published.spec.content_id == published.content_id)
                .then_some(published.spec)
                .or_else(|| {
                    warn!(
                        "published spacecraft {} embeds a mismatched id; ignoring",
                        published.content_id
                    );
                    None
                })
        })
        .collect::<Vec<_>>();
    let vehicle_count = vehicle_catalog.replace(vehicles);
    let spacecraft_count = spacecraft_catalog.replace(spacecraft);
    let route_count = platformer_route_catalog.replace(valid_platformer_routes(&snapshot));
    info!(
        "published content reloaded: {vehicle_count} vehicle(s), {spacecraft_count} spacecraft, {route_count} platformer route(s)"
    );
}

/// Parse a published file, treating "missing" as empty and only warning on
/// files that exist but do not parse (that is real corruption worth surfacing).
fn read_published<T: serde::de::DeserializeOwned>(
    snapshot: &PublishedGenerationSnapshot,
    file: &str,
) -> Vec<T> {
    let path = snapshot.file(file);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            warn!(
                "published content {} unreadable ({error}); ignoring",
                path.display()
            );
            return Vec::new();
        }
    };
    match serde_json::from_str(&text) {
        Ok(items) => items,
        Err(error) => {
            warn!(
                "published content {} unparsable ({error}); ignoring",
                path.display()
            );
            Vec::new()
        }
    }
}

fn valid_platformer_routes(
    snapshot: &PublishedGenerationSnapshot,
) -> Vec<crate::platformer_graph::PlatformerRouteDocument> {
    read_published(snapshot, "platformer_routes.json")
        .into_iter()
        .filter_map(|route: crate::platformer_graph::PlatformerRouteDocument| {
            if let Err(error) = route.validate_schema() {
                warn!(
                    "published platformer route {} is invalid ({error}); ignoring",
                    route.id
                );
                return None;
            }
            if let Err(error) = route.compile_runtime(
                [0.0; 3],
                crate::platformer::JumpEnvelope::standard(),
                chunk_by_id,
            ) {
                warn!(
                    "published platformer route {} is invalid ({error}); ignoring",
                    route.id
                );
                return None;
            }
            Some(route)
        })
        .collect()
}

/// The shop entry for a published weapon. Priced by the same derivation the
/// forge showed the designer, so the shop can never disagree with the tool.
fn shop_item_for_published(weapon: &PublishedWeapon) -> ShopItem {
    let profile = weapon.spec.to_published_profile(&weapon.content_id);
    ShopItem::new(
        profile.id,
        profile.name,
        ShopCategory::Weapons,
        profile.summary,
        profile.price_credits,
        None,
    )
}

fn load_published_content(
    root: Res<PublishedContentRoot>,
    mut shop: ResMut<ShopCatalog>,
    mut creature_catalog: ResMut<PublishedCreatureCatalog>,
    mut dialogue_catalog: ResMut<PublishedDialogueCatalog>,
    mut vehicle_catalog: ResMut<PublishedVehicleCatalog>,
    mut spacecraft_catalog: ResMut<PublishedSpacecraftCatalog>,
    mut platformer_route_catalog: ResMut<PublishedPlatformerRouteCatalog>,
) {
    // Keep one immutable manifest resolution for the whole logical load. A
    // publish that commits concurrently is picked up on the next load, never
    // halfway through this cross-category content set.
    let snapshot = match published_generation_in(&root.0) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!("{error}; ignoring published content");
            return;
        }
    };

    let weapons: Vec<PublishedWeapon> = read_published(&snapshot, "weapons.json");
    if !weapons.is_empty() {
        // Blades register into the global resolver (pure helpers consult it),
        // and each published weapon goes on sale beside the built-in catalog.
        let profiles = weapons
            .iter()
            .map(|weapon| weapon.spec.to_published_profile(&weapon.content_id))
            .collect();
        crate::combat::blades::register_published_blades(profiles);
        for weapon in &weapons {
            // Re-publishing must not duplicate shop rows across hot reloads.
            if !shop.items.iter().any(|item| item.id == weapon.content_id) {
                shop.items.push(shop_item_for_published(weapon));
            }
        }
        info!("published content: {} weapon(s) on sale", weapons.len());
    }

    // Creatures feed the same catalog the dungeon spawners already consult
    // (`PublishedCreatureCatalog`). In a Designer session the editor
    // workspace later rebuilds that catalog from the live project store —
    // drafts included — so the baked seed only fills an empty catalog and
    // never clobbers the authoritative one.
    let specs: Vec<CreatureSpec> = read_published(&snapshot, "creatures.json");
    if !specs.is_empty() {
        let seeded = creature_catalog.seed_from_published(specs);
        if seeded > 0 {
            info!("published content: {seeded} creature(s) available to encounters");
        }
    }

    // Dialogue graphs feed the runtime director. Each graph is re-validated
    // on the consumer side: a hand-edited or corrupt published file degrades
    // to a skipped conversation, never a runtime panic.
    let dialogues: Vec<PublishedDialogue> = read_published(&snapshot, "dialogues.json");
    if !dialogues.is_empty() {
        let valid = dialogues.into_iter().filter_map(|published| {
            let errors = crate::engine_tools::dialogue_records::validate_dialogue(&published.graph);
            if errors.is_empty() {
                Some((published.content_id.clone(), published.graph))
            } else {
                warn!(
                    "published dialogue {} failed validation ({}); ignoring",
                    published.content_id,
                    errors.join("; ")
                );
                None
            }
        });
        let seeded = dialogue_catalog.seed_from_published(valid);
        if seeded > 0 {
            info!("published content: {seeded} dialogue graph(s) available");
        }
    }

    let vehicles: Vec<PublishedVehicle> = read_published(&snapshot, "vehicles.json");
    if !vehicles.is_empty() {
        let specs = vehicles.into_iter().filter_map(|published| {
            if published.spec.content_id != published.content_id {
                warn!(
                    "published vehicle {} embeds mismatched id {}; ignoring",
                    published.content_id, published.spec.content_id
                );
                None
            } else {
                Some(published.spec)
            }
        });
        let seeded = vehicle_catalog.seed_from_published(specs);
        if seeded > 0 {
            info!("published content: {seeded} vehicle design(s) available");
        }
    }

    let spaceships: Vec<PublishedSpaceship> = read_published(&snapshot, "spaceships.json");
    if !spaceships.is_empty() {
        let specs = spaceships.into_iter().filter_map(|published| {
            if published.spec.content_id != published.content_id {
                warn!(
                    "published spacecraft {} embeds mismatched id {}; ignoring",
                    published.content_id, published.spec.content_id
                );
                None
            } else {
                Some(published.spec)
            }
        });
        let seeded = spacecraft_catalog.seed_from_published(specs);
        if seeded > 0 {
            info!("published content: {seeded} spacecraft design(s) available");
        }
    }

    // Route documents are compiled again against the consumer's real chunk
    // catalog and movement envelope. Structurally valid Forge output that
    // references unavailable game content is skipped rather than becoming a
    // broken selectable level.
    let routes = valid_platformer_routes(&snapshot);
    if !routes.is_empty() {
        let seeded = platformer_route_catalog.seed_from_published(routes);
        if seeded > 0 {
            info!("published content: {seeded} platformer route(s) available");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::blades::blade_for_id;
    use crate::combat::weapon_forge::{GripStyle, WeaponSpec};
    use crate::engine::state::AppState;
    use crate::spaceship_forge::{SpacecraftClass, SpacecraftSpec};
    use crate::vehicle_forge::{VehicleClass, VehicleSpec};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn published(name: &str, id: &str) -> PublishedWeapon {
        PublishedWeapon {
            content_id: id.to_string(),
            spec: WeaponSpec {
                name: name.to_string(),
                grip: GripStyle::Extended,
                blade_length: 1.6,
                ..Default::default()
            },
        }
    }

    fn test_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "starfall_published_catalog_{label}_{}_{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_craft_generation(root: &std::path::Path) {
        let generation = "cccccccccccccccc";
        let directory = root.join("generations").join(generation);
        std::fs::create_dir_all(&directory).unwrap();

        let mut vehicle = VehicleSpec::preset(VehicleClass::Car);
        vehicle.content_id = "starfall.vehicle.bootstrap_car".into();
        let vehicles = [PublishedVehicle {
            content_id: vehicle.content_id.clone(),
            spec: vehicle,
        }];
        let mut spacecraft = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        spacecraft.content_id = "starfall.spaceship.bootstrap_fighter".into();
        let spacecraft = [PublishedSpaceship {
            content_id: spacecraft.content_id.clone(),
            spec: spacecraft,
        }];
        std::fs::write(
            directory.join("vehicles.json"),
            serde_json::to_vec_pretty(&vehicles).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("spaceships.json"),
            serde_json::to_vec_pretty(&spacecraft).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("current.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "generation": generation,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[derive(Resource, Default)]
    struct CraftBootstrapProbe {
        entries: usize,
        vehicles: usize,
        spacecraft: usize,
    }

    fn probe_craft_catalogs_on_playing_entry(
        vehicles: Res<PublishedVehicleCatalog>,
        spacecraft: Res<PublishedSpacecraftCatalog>,
        mut probe: ResMut<CraftBootstrapProbe>,
    ) {
        probe.entries += 1;
        probe.vehicles = vehicles.len();
        probe.spacecraft = spacecraft.len();
    }

    fn craft_bootstrap_app(root: std::path::PathBuf) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<AppState>()
            .init_resource::<ShopCatalog>()
            .init_resource::<PublishedCreatureCatalog>()
            .init_resource::<PublishedDialogueCatalog>()
            .init_resource::<CraftBootstrapProbe>()
            .add_plugins(PublishedContentPlugin)
            .insert_resource(PublishedContentRoot(root))
            .add_systems(
                OnEnter(AppState::Playing),
                probe_craft_catalogs_on_playing_entry,
            );
        app
    }

    #[test]
    fn direct_playing_boot_observes_craft_catalogs_on_first_entry() {
        let root = test_dir("direct_playing_boot");
        write_craft_generation(&root);
        let mut app = craft_bootstrap_app(root.clone());
        app.insert_state(AppState::Playing);
        app.update();

        let probe = app.world().resource::<CraftBootstrapProbe>();
        assert_eq!((probe.entries, probe.vehicles, probe.spacecraft), (1, 1, 1));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn menu_to_playing_uses_the_same_bootstrapped_craft_snapshot() {
        let root = test_dir("menu_playing_boot");
        write_craft_generation(&root);
        let mut app = craft_bootstrap_app(root.clone());
        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        let probe = app.world().resource::<CraftBootstrapProbe>();
        assert_eq!((probe.entries, probe.vehicles, probe.spacecraft), (1, 1, 1));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_dialogues_seed_the_catalog_and_invalid_graphs_are_skipped() {
        use crate::engine_tools::dialogue_records::{DialogueChoice, DialogueGraph, DialogueNode};

        let root = test_dir("dialogue_bootstrap");
        let generation = "dddddddddddddddd";
        let directory = root.join("generations").join(generation);
        std::fs::create_dir_all(&directory).unwrap();

        let mut broken = DialogueGraph::default();
        broken.nodes[0].choices.push(DialogueChoice {
            label: "Leave".into(),
            target_node: "missing_node".into(),
            required_flag: None,
            set_flag: None,
        });
        let mut branched = DialogueGraph {
            entry_node: "opening".into(),
            ..DialogueGraph::default()
        };
        branched.nodes[0].next_node = Some("farewell".into());
        branched.nodes.push(DialogueNode::speech(
            "farewell",
            "Narrator",
            "Safe travels.",
        ));
        let dialogues = vec![
            PublishedDialogue {
                content_id: "starfall.dialogue.broken".into(),
                graph: broken,
            },
            PublishedDialogue {
                content_id: "starfall.dialogue.branched".into(),
                graph: branched,
            },
        ];
        std::fs::write(
            directory.join("dialogues.json"),
            serde_json::to_vec_pretty(&dialogues).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("current.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "generation": generation,
            }))
            .unwrap(),
        )
        .unwrap();

        let mut app = craft_bootstrap_app(root.clone());
        app.insert_state(AppState::Playing);
        app.update();

        let catalog = app.world().resource::<PublishedDialogueCatalog>();
        assert_eq!(catalog.len(), 1, "only the valid graph may seed");
        let graph = catalog
            .get("starfall.dialogue.branched")
            .expect("valid dialogue must resolve by content id");
        assert_eq!(graph.nodes.len(), 2);
        assert!(!catalog.contains("starfall.dialogue.broken"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_platformer_routes_resolve_the_game_catalog_and_skip_broken_entries() {
        use crate::graph::StableId;
        use crate::platformer_graph::{
            compile_platformer_graph, register_platformer_nodes, PlatformerRouteGraphBuilder,
        };

        let root = test_dir("platformer_route_bootstrap");
        let generation = "eeeeeeeeeeeeeeee";
        let directory = root.join("generations").join(generation);
        std::fs::create_dir_all(&directory).unwrap();
        let id = |value: &str| StableId::new(value).unwrap();
        let compile = |route_id: &str, chunk_ids: &[&str]| {
            let mut builder = PlatformerRouteGraphBuilder::new(
                id(route_id),
                route_id,
                id("heavy_water.theme.cityscape"),
            );
            for (index, chunk_id) in chunk_ids.iter().enumerate() {
                builder = builder.chunk(id(&format!("chunk_{index}")), id(chunk_id));
            }
            let graph = builder.build();
            let mut registry = crate::graph::NodeRegistry::default();
            register_platformer_nodes(&mut registry).unwrap();
            compile_platformer_graph(&graph, &registry)
                .unwrap()
                .document
        };
        let routes = [
            compile(
                "route_city_rooftops",
                &["city_rooftop_arrival", "city_plaza_arena"],
            ),
            compile(
                "route_broken_catalog_reference",
                &["missing_arrival", "city_plaza_arena"],
            ),
        ];
        std::fs::write(
            directory.join("platformer_routes.json"),
            serde_json::to_vec_pretty(&routes).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("current.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "generation": generation,
            }))
            .unwrap(),
        )
        .unwrap();

        let mut app = craft_bootstrap_app(root.clone());
        app.insert_state(AppState::Playing);
        app.update();
        let catalog = app.world().resource::<PublishedPlatformerRouteCatalog>();
        assert_eq!(catalog.len(), 1);
        assert!(catalog.get("route_city_rooftops").is_some());
        assert!(catalog.get("route_broken_catalog_reference").is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_published_directory_is_a_first_class_empty_state() {
        // A fresh consumer install has no published content; that must read
        // as "no extra weapons", never as an error path.
        let missing_root =
            std::env::temp_dir().join(format!("starfall_missing_published_{}", std::process::id()));
        let snapshot = published_generation_in(&missing_root).unwrap();
        let missing: Vec<PublishedWeapon> = read_published(&snapshot, "does_not_exist.json");
        assert!(missing.is_empty());
    }

    #[test]
    fn one_catalog_load_cannot_mix_generations_when_manifest_changes_mid_read() {
        let root = test_dir("snapshot");
        let generations = root.join("generations");
        let generation_a = "aaaaaaaaaaaaaaaa";
        let generation_b = "bbbbbbbbbbbbbbbb";

        for (generation, label) in [(generation_a, "a"), (generation_b, "b")] {
            let dir = generations.join(generation);
            std::fs::create_dir_all(&dir).unwrap();
            let weapons = vec![published(
                &format!("Generation {label}"),
                &format!("starfall.weapon.{label}"),
            )];
            let creatures = vec![CreatureSpec {
                content_id: format!("starfall.creature.{label}"),
                display_name: format!("Generation {label}"),
                ..Default::default()
            }];
            std::fs::write(
                dir.join("weapons.json"),
                serde_json::to_vec_pretty(&weapons).unwrap(),
            )
            .unwrap();
            std::fs::write(
                dir.join("creatures.json"),
                serde_json::to_vec_pretty(&creatures).unwrap(),
            )
            .unwrap();
        }

        let manifest = |generation: &str| {
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "generation": generation,
            }))
            .unwrap()
        };
        std::fs::write(root.join("current.json"), manifest(generation_a)).unwrap();

        let snapshot = published_generation_in(&root).unwrap();
        let weapons: Vec<PublishedWeapon> = read_published(&snapshot, "weapons.json");
        assert_eq!(weapons[0].content_id, "starfall.weapon.a");

        // A concurrent publish becomes visible between the two logical reads.
        std::fs::write(root.join("current.json"), manifest(generation_b)).unwrap();
        let creatures: Vec<CreatureSpec> = read_published(&snapshot, "creatures.json");

        assert_eq!(creatures[0].content_id, "starfall.creature.a");
        assert_eq!(
            published_generation_in(&root)
                .unwrap()
                .file("creatures.json"),
            generations.join(generation_b).join("creatures.json"),
            "a later catalog load observes the new commit"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_weapons_resolve_through_the_blade_lookup() {
        let weapon = published("Registry Blade", "starfall.weapon.registry_blade");
        let profile = weapon.spec.to_published_profile(&weapon.content_id);
        crate::combat::blades::register_published_blades(vec![profile]);

        // The equip path resolves the published id exactly like a catalog id…
        let resolved = blade_for_id(Some("starfall.weapon.registry_blade"));
        assert_eq!(resolved.name, "Registry Blade");
        assert!(
            resolved.slash_damage_mult > 1.0,
            "the big blade's derived stats survive publishing"
        );
        // …while unknown ids still fall back to the starter.
        assert_eq!(
            blade_for_id(Some("starfall.weapon.never_published")).id,
            "blade_standard_issue"
        );
    }

    #[test]
    fn baked_creatures_seed_only_an_empty_catalog() {
        use crate::robots::creature::CreatureSpec;
        let mut catalog = PublishedCreatureCatalog::default();

        let first = CreatureSpec {
            content_id: "starfall.creature.baked".to_string(),
            ..Default::default()
        };
        assert_eq!(catalog.seed_from_published(vec![first]), 1);
        assert!(catalog.contains("starfall.creature.baked"));

        // A Designer session's workspace rebuild owns a non-empty catalog;
        // a second seed (stale baked files) must not clobber it.
        let second = CreatureSpec {
            content_id: "starfall.creature.stale".to_string(),
            ..Default::default()
        };
        assert_eq!(catalog.seed_from_published(vec![second]), 0);
        assert!(!catalog.contains("starfall.creature.stale"));
    }

    #[test]
    fn vehicle_and_spacecraft_catalogs_accept_only_valid_matching_wrappers() {
        let mut vehicle = VehicleSpec::preset(VehicleClass::Truck);
        vehicle.content_id = "starfall.vehicle.hauler".into();
        let good_vehicle = PublishedVehicle {
            content_id: vehicle.content_id.clone(),
            spec: vehicle.clone(),
        };
        let bad_vehicle = PublishedVehicle {
            content_id: "starfall.vehicle.impostor".into(),
            spec: vehicle,
        };
        let mut vehicles = PublishedVehicleCatalog::default();
        let valid_vehicles = [good_vehicle, bad_vehicle]
            .into_iter()
            .filter(|published| published.content_id == published.spec.content_id)
            .map(|published| published.spec);
        assert_eq!(vehicles.seed_from_published(valid_vehicles), 1);
        assert!(vehicles.contains("starfall.vehicle.hauler"));
        assert!(!vehicles.contains("starfall.vehicle.impostor"));

        let mut spacecraft = SpacecraftSpec::preset(SpacecraftClass::Bomber);
        spacecraft.content_id = "starfall.spaceship.night_bomber".into();
        let good_spacecraft = PublishedSpaceship {
            content_id: spacecraft.content_id.clone(),
            spec: spacecraft.clone(),
        };
        let bad_spacecraft = PublishedSpaceship {
            content_id: "starfall.spaceship.impostor".into(),
            spec: spacecraft,
        };
        let mut spaceships = PublishedSpacecraftCatalog::default();
        let valid_spacecraft = [good_spacecraft, bad_spacecraft]
            .into_iter()
            .filter(|published| published.content_id == published.spec.content_id)
            .map(|published| published.spec);
        assert_eq!(spaceships.seed_from_published(valid_spacecraft), 1);
        assert!(spaceships.get("starfall.spaceship.night_bomber").is_some());
        assert!(spaceships.get("starfall.spaceship.impostor").is_none());
    }

    #[test]
    fn shop_rows_price_published_weapons_by_the_forge_derivation() {
        let weapon = published("Priced Blade", "starfall.weapon.priced_blade");
        let item = shop_item_for_published(&weapon);
        assert_eq!(item.id, "starfall.weapon.priced_blade");
        assert_eq!(item.name, "Priced Blade");
        assert_eq!(item.category, ShopCategory::Weapons);
        assert_eq!(
            item.price_credits,
            weapon.spec.estimated_value(),
            "the shop must charge what the forge displayed"
        );
    }
}
