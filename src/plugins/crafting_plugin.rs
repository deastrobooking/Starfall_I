use bevy::prelude::*;

use crate::components::inventory::Inventory;
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
    #[allow(dead_code)]
    Base,
    Consumable,
    Upgrade,
}

// ── Recipe Registry ───────────────────────────────────────────────────────────
pub fn all_recipes() -> Vec<CraftingRecipe> {
    vec![
        CraftingRecipe {
            id: "damage_mod",
            name: "Damage Mod",
            category: RecipeCategory::Weapon,
            materials: vec![
                CraftingMaterial {
                    item_id: "scrap_metal",
                    quantity: 5,
                },
                CraftingMaterial {
                    item_id: "energy_core",
                    quantity: 2,
                },
            ],
            result_item: "damage_amp",
            result_quantity: 1,
            craft_time: 3.0,
            required_level: 2,
        },
        CraftingRecipe {
            id: "basic_helmet",
            name: "Basic Helmet",
            category: RecipeCategory::Armor,
            materials: vec![
                CraftingMaterial {
                    item_id: "scrap_metal",
                    quantity: 6,
                },
                CraftingMaterial {
                    item_id: "nano_fiber",
                    quantity: 2,
                },
            ],
            result_item: "armor_shard",
            result_quantity: 3,
            craft_time: 5.0,
            required_level: 1,
        },
        CraftingRecipe {
            id: "health_pack_adv",
            name: "Advanced Health Pack",
            category: RecipeCategory::Consumable,
            materials: vec![
                CraftingMaterial {
                    item_id: "bio_sample",
                    quantity: 3,
                },
                CraftingMaterial {
                    item_id: "circuit_board",
                    quantity: 1,
                },
            ],
            result_item: "health_pack",
            result_quantity: 2,
            craft_time: 4.0,
            required_level: 3,
        },
        CraftingRecipe {
            id: "shield_bat",
            name: "Shield Battery",
            category: RecipeCategory::Consumable,
            materials: vec![
                CraftingMaterial {
                    item_id: "energy_core",
                    quantity: 3,
                },
                CraftingMaterial {
                    item_id: "circuit_board",
                    quantity: 2,
                },
            ],
            result_item: "shield_booster",
            result_quantity: 1,
            craft_time: 5.0,
            required_level: 4,
        },
        CraftingRecipe {
            id: "beam_core",
            name: "Star Sabre Core",
            category: RecipeCategory::Upgrade,
            materials: vec![
                CraftingMaterial {
                    item_id: "dark_matter",
                    quantity: 1,
                },
                CraftingMaterial {
                    item_id: "crystal_shard",
                    quantity: 5,
                },
                CraftingMaterial {
                    item_id: "energy_core",
                    quantity: 8,
                },
            ],
            result_item: "xp_chip",
            result_quantity: 10,
            craft_time: 15.0,
            required_level: 8,
        },
    ]
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
#[derive(Resource, Debug, Default)]
pub struct CraftingQueue {
    pub items: Vec<ActiveCraft>,
}

#[derive(Debug)]
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
    let mut finished = vec![];
    for (i, craft) in queue.items.iter_mut().enumerate() {
        craft.timer -= dt;
        if craft.timer <= 0.0 {
            finished.push(i);
        }
    }

    let mut completed = Vec::with_capacity(finished.len());
    for i in finished.into_iter().rev() {
        completed.push(queue.items.swap_remove(i));
    }

    for craft in completed {
        if let Some((_, mut inventory)) = player_q.iter_mut().find(|(idx, _)| idx.0 == craft.owner)
        {
            inventory.add_item(&craft.result_item, craft.result_qty, 10);
            inv_ev.write(InventoryChangedEvent);
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
            inventory.add_item("energy_core", 2, 50);
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
}
