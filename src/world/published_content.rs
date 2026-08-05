//! Game-side loader for Designer-published content (docs/PROJECT_PLAN.md P1).
//!
//! Reads the plain JSON the publish step baked into `assets/published/` and
//! makes it playable: published weapons become resolvable blades and shop
//! stock, published creatures become a spec pool for encounter systems. This
//! is the *read* half of the pipeline and ships in **both** editions — the
//! consumer Game edition has no writer, only this.
//!
//! A fresh install with no published content is a first-class state: missing
//! files mean empty sets, never errors.

use bevy::prelude::*;

use crate::engine_tools::publish::{published_dir, PublishedWeapon};
use crate::engine_tools::PublishedCreatureCatalog;
use crate::resources::{ShopCatalog, ShopCategory, ShopItem};
use crate::robots::creature::CreatureSpec;

pub struct PublishedContentPlugin;

impl Plugin for PublishedContentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_published_content);
    }
}

/// Parse a published file, treating "missing" as empty and only warning on
/// files that exist but do not parse (that is real corruption worth surfacing).
fn read_published<T: serde::de::DeserializeOwned>(file: &str) -> Vec<T> {
    let path = published_dir().join(file);
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
) {
    let weapons: Vec<PublishedWeapon> = read_published("weapons.json");
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
    let specs: Vec<CreatureSpec> = read_published("creatures.json");
    if !specs.is_empty() {
        let seeded = creature_catalog.seed_from_published(specs);
        if seeded > 0 {
            info!("published content: {seeded} creature(s) available to encounters");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::blades::blade_for_id;
    use crate::combat::weapon_forge::{GripStyle, WeaponSpec};

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

    #[test]
    fn a_missing_published_directory_is_a_first_class_empty_state() {
        // A fresh consumer install has no published content; that must read
        // as "no extra weapons", never as an error path.
        let missing: Vec<PublishedWeapon> = read_published("does_not_exist.json");
        assert!(missing.is_empty());
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
