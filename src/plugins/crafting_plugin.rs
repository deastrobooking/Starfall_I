use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::inventory::{max_stack_for, Inventory};
use crate::components::player::{Player, PlayerIndex, PlayerStats};
use crate::engine::state::AppState;
use crate::events::InventoryChangedEvent;

// ── Crafting Recipe ───────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct CraftingMaterial {
    pub item_id: &'static str,
    pub quantity: u32,
}

#[derive(Debug, Clone)]
pub struct CraftingRecipe {
    pub id: &'static str,
    pub name: &'static str,
    #[allow(dead_code)]
    pub category: RecipeCategory,
    pub materials: Vec<CraftingMaterial>,
    pub result_item: &'static str,
    pub result_quantity: u32,
    pub craft_time: f32,
    pub required_level: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeCategory {
    Weapon,
    Armor,
    Base,
    Consumable,
    Upgrade,
    Drone,
    Robot,
    City,
    Ship,
}

// ── Recipe Registry ───────────────────────────────────────────────────────────
pub fn all_recipes() -> Vec<CraftingRecipe> {
    use crate::world::heavy_economy::RecipeCategory as PortedCategory;

    crate::world::heavy_economy::RECIPE_CATALOG
        .iter()
        .map(|recipe| CraftingRecipe {
            id: recipe.id,
            name: recipe.name,
            category: match recipe.category {
                PortedCategory::Weapon => RecipeCategory::Weapon,
                PortedCategory::Armor => RecipeCategory::Armor,
                PortedCategory::Base => RecipeCategory::Base,
                PortedCategory::Upgrade => RecipeCategory::Upgrade,
                PortedCategory::Consumable => RecipeCategory::Consumable,
                PortedCategory::Drone => RecipeCategory::Drone,
                PortedCategory::Robot => RecipeCategory::Robot,
                PortedCategory::City => RecipeCategory::City,
                PortedCategory::Ship => RecipeCategory::Ship,
            },
            materials: recipe
                .materials
                .iter()
                .map(|material| CraftingMaterial {
                    item_id: material.item_id,
                    quantity: material.quantity,
                })
                .collect(),
            result_item: recipe.result.item_id,
            result_quantity: recipe.result.quantity,
            craft_time: recipe.craft_seconds as f32,
            required_level: u32::from(recipe.required_level),
        })
        .collect()
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingQueue>().add_systems(
            Update,
            crafting_queue_system.run_if(in_state(AppState::Playing)),
        );
    }
}

// ── Craft Queue ───────────────────────────────────────────────────────────────
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct CraftingQueue {
    pub items: Vec<ActiveCraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCraft {
    pub owner: u8,
    #[allow(dead_code)]
    // Kept for crafting UI progress display (recipe name + fraction) planned in the garage/store pass.
    pub recipe_id: String,
    pub timer: f32,
    #[allow(dead_code)]
    pub duration: f32,
    pub result_item: String,
    pub result_qty: u32,
}

fn crafting_queue_system(
    time: Res<Time>,
    mut queue: ResMut<CraftingQueue>,
    mut player_q: Query<(&PlayerIndex, &mut Inventory), With<Player>>,
    mut inv_ev: MessageWriter<InventoryChangedEvent>,
) {
    let dt = time.delta_secs();
    let mut ready = vec![];
    for (i, craft) in queue.items.iter_mut().enumerate() {
        craft.timer -= dt;
        if craft.timer <= 0.0 {
            ready.push(i);
        }
    }

    // A finished craft remains queued until every result item has entered the
    // owner's inventory. This turns a full bag into a visible pending delivery
    // instead of deleting rare jewels, vehicle frames, or building prefabs.
    for i in ready.into_iter().rev() {
        let owner = queue.items[i].owner;
        let Some((_, mut inventory)) = player_q.iter_mut().find(|(idx, _)| idx.0 == owner) else {
            continue;
        };
        let result_item = queue.items[i].result_item.clone();
        let result_qty = queue.items[i].result_qty;
        let leftover = inventory.add_item(&result_item, result_qty, max_stack_for(&result_item));
        if leftover == 0 {
            queue.items.swap_remove(i);
            inv_ev.write(InventoryChangedEvent);
        } else {
            queue.items[i].timer = 0.0;
            queue.items[i].result_qty = leftover;
        }
    }
}

/// Attempt to start crafting a recipe. Returns Ok(()) on success or Err message.
pub fn start_craft(
    recipe_id: &str,
    owner: u8,
    inventory: &mut Inventory,
    stats: &PlayerStats,
    queue: &mut CraftingQueue,
) -> Result<(), &'static str> {
    let recipes = all_recipes();
    let recipe = recipes
        .iter()
        .find(|r| r.id == recipe_id)
        .ok_or("Unknown recipe")?;

    if stats.level < recipe.required_level {
        return Err("Level too low");
    }

    // Check materials
    for mat in &recipe.materials {
        if !inventory.has(mat.item_id, mat.quantity) {
            return Err("Insufficient materials");
        }
    }

    // Consume materials
    for mat in &recipe.materials {
        inventory.remove_item(mat.item_id, mat.quantity);
    }

    queue.items.push(ActiveCraft {
        owner,
        recipe_id: recipe_id.to_string(),
        timer: recipe.craft_time,
        duration: recipe.craft_time,
        result_item: recipe.result_item.to_string(),
        result_qty: recipe.result_quantity,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crafting_queue_preserves_all_four_player_owners() {
        let mut queue = CraftingQueue::default();
        let stats = PlayerStats {
            level: 2,
            ..default()
        };

        for owner in 0..4 {
            let mut inventory = Inventory::default();
            inventory.add_item("scrap_metal", 5, 99);
            inventory.add_item("circuit_board", 2, 50);
            start_craft("damage_mod", owner, &mut inventory, &stats, &mut queue)
                .expect("every player can own a craft");
        }

        assert_eq!(queue.items.len(), 4);
        assert_eq!(
            queue
                .items
                .iter()
                .map(|craft| craft.owner)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn failed_craft_does_not_consume_partial_materials() {
        let mut queue = CraftingQueue::default();
        let stats = PlayerStats {
            level: 2,
            ..default()
        };
        let mut inventory = Inventory::default();
        inventory.add_item("scrap_metal", 5, 99);

        assert_eq!(
            start_craft("damage_mod", 3, &mut inventory, &stats, &mut queue),
            Err("Insufficient materials")
        );
        assert!(inventory.has("scrap_metal", 5));
        assert!(queue.items.is_empty());
    }

    #[test]
    fn executable_heavy_water_recipe_catalog_is_fully_player_craftable() {
        let recipes = all_recipes();
        assert_eq!(recipes.len(), 25);
        let mut ids = std::collections::HashSet::new();
        assert!(recipes.iter().all(|recipe| ids.insert(recipe.id)));
        for recipe in &recipes {
            assert!(
                crate::components::inventory::item_definition(recipe.result_item).is_some(),
                "missing result definition for {}",
                recipe.id
            );
            assert!(recipe.craft_time > 0.0);
            assert!(!recipe.materials.is_empty());
        }
    }

    #[test]
    fn finished_craft_waits_when_inventory_is_full_instead_of_losing_result() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<InventoryChangedEvent>()
            .insert_resource(CraftingQueue {
                items: vec![ActiveCraft {
                    owner: 0,
                    recipe_id: "rare_result".to_string(),
                    timer: 0.0,
                    duration: 1.0,
                    result_item: "power_jewel_flawless".to_string(),
                    result_qty: 1,
                }],
            })
            .add_systems(Update, crafting_queue_system);
        let mut inventory = Inventory {
            slots: vec![
                Some(crate::components::inventory::InventorySlot {
                    item_id: "scrap_metal".to_string(),
                    quantity: 99,
                });
                100
            ],
            max_slots: 100,
        };
        inventory.ensure_capacity(100);
        let player = app
            .world_mut()
            .spawn((Player, PlayerIndex(0), inventory))
            .id();

        app.update();
        assert_eq!(app.world().resource::<CraftingQueue>().items.len(), 1);
        assert_eq!(
            app.world()
                .get::<Inventory>(player)
                .unwrap()
                .count("power_jewel_flawless"),
            0
        );

        app.world_mut()
            .get_mut::<Inventory>(player)
            .unwrap()
            .remove_item("scrap_metal", 99);
        app.update();
        assert!(app.world().resource::<CraftingQueue>().items.is_empty());
        assert_eq!(
            app.world()
                .get::<Inventory>(player)
                .unwrap()
                .count("power_jewel_flawless"),
            1
        );
    }
}
