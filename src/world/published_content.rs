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

use bevy::prelude::*;

use crate::engine_tools::publish::{
    published_dir, published_generation_in, PublishedGenerationSnapshot, PublishedSpaceship,
    PublishedVehicle, PublishedWeapon,
};
use crate::engine_tools::PublishedCreatureCatalog;
use crate::resources::{ShopCatalog, ShopCategory, ShopItem};
use crate::robots::creature::CreatureSpec;
use crate::spaceship_forge::PublishedSpacecraftCatalog;
use crate::vehicle_forge::PublishedVehicleCatalog;

pub struct PublishedContentPlugin;

impl Plugin for PublishedContentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PublishedVehicleCatalog>()
            .init_resource::<PublishedSpacecraftCatalog>()
            .add_systems(Startup, load_published_content);
    }
}

/// Parse a published file, treating "missing" as empty and only warning on
/// files that exist but do not parse (that is real corruption worth surfacing).
fn read_published<T: serde::de::DeserializeOwned>(
    snapshot: &PublishedGenerationSnapshot,
    file: &str,
) -> Vec<T> {
    let path = snapshot.file(file);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
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
    mut shop: ResMut<ShopCatalog>,
    mut creature_catalog: ResMut<PublishedCreatureCatalog>,
    mut vehicle_catalog: ResMut<PublishedVehicleCatalog>,
    mut spacecraft_catalog: ResMut<PublishedSpacecraftCatalog>,
) {
    // Keep one immutable manifest resolution for the whole logical load. A
    // publish that commits concurrently is picked up on the next load, never
    // halfway through this cross-category content set.
    let root = published_dir();
    let snapshot = match published_generation_in(&root) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::blades::blade_for_id;
    use crate::combat::weapon_forge::{GripStyle, WeaponSpec};
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
