use bevy::prelude::*;

use crate::engine::game_rng::GameRng;
use rand::Rng;

use crate::combat::damage::Health;
use crate::components::inventory::{max_stack_for, Inventory};
use crate::components::player::{Player, PlayerProgression, PlayerStats};
use crate::components::weapon::{WeaponInventory, WeaponType, MAX_WEAPON_RANK};
use crate::components::world::{Chest, LootType};
use crate::engine::rendering::PbrBundle;
use crate::engine::state::AppState;
use crate::events::{ChestOpenedEvent, InventoryChangedEvent, LootCollectedEvent};
use crate::resources::{PlaySessionTransition, PlayerScore};

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_chests)
            .add_systems(OnEnter(AppState::MainMenu), cleanup_chests_for_menu)
            .add_systems(OnExit(AppState::Playing), cleanup_chests)
            .add_systems(
                Update,
                (chest_proximity_system, animate_open_chests)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct ChestLid;

fn spawn_chests(
    mut game_rng: ResMut<GameRng>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transition: Res<PlaySessionTransition>,
    existing_chests: Query<Entity, With<Chest>>,
) {
    if transition.resuming_from_pause || !existing_chests.is_empty() {
        return;
    }

    let rng = game_rng.loot();
    let gold_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.7, 0.1),
        metallic: 0.8,
        perceptual_roughness: 0.2,
        emissive: LinearRgba::new(0.5, 0.35, 0.0, 1.0),
        ..default()
    });

    for _ in 0..20 {
        let x = rng.gen_range(-400.0f32..400.0);
        let z = rng.gen_range(-300.0f32..300.0);

        let loot_roll: f32 = rng.gen();
        let (loot_type, amount) = if loot_roll < 0.35 {
            (LootType::Credits, rng.gen_range(50..200u32))
        } else if loot_roll < 0.55 {
            (LootType::Health, rng.gen_range(25..75))
        } else if loot_roll < 0.70 {
            (LootType::Armor, rng.gen_range(20..60))
        } else if loot_roll < 0.90 {
            (LootType::Ammo, rng.gen_range(20..50))
        } else {
            (LootType::WeaponUpgrade, 1)
        };

        let body_mesh = meshes.add(Cuboid::new(1.5, 0.9, 1.5));
        let lid_mesh = meshes.add(Cuboid::new(1.62, 0.28, 1.58));
        commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(body_mesh),
                    material: MeshMaterial3d(gold_mat.clone()),
                    transform: Transform::from_xyz(x, 0.48, z),
                    ..default()
                },
                Chest::new(loot_type, amount),
                PointLight {
                    color: Color::srgb(1.0, 0.85, 0.2),
                    intensity: 5_000.0,
                    range: 8.0,
                    ..default()
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    PbrBundle {
                        mesh: Mesh3d(lid_mesh),
                        material: MeshMaterial3d(gold_mat.clone()),
                        transform: Transform::from_xyz(0.0, 0.58, 0.0),
                        ..default()
                    },
                    ChestLid,
                ));
            });
    }
}

fn cleanup_chests(
    mut commands: Commands,
    transition: Res<PlaySessionTransition>,
    chest_q: Query<Entity, With<Chest>>,
) {
    if transition.pausing {
        return;
    }

    for entity in chest_q.iter() {
        commands.entity(entity).despawn();
    }
}

fn cleanup_chests_for_menu(mut commands: Commands, chest_q: Query<Entity, With<Chest>>) {
    for entity in chest_q.iter() {
        commands.entity(entity).despawn();
    }
}

fn chest_proximity_system(
    player_q: Query<(Entity, &Transform), With<Player>>,
    mut player_loot_q: Query<
        (
            &mut PlayerStats,
            &mut Inventory,
            &mut WeaponInventory,
            &mut PlayerProgression,
        ),
        With<Player>,
    >,
    mut player_health_q: Query<&mut Health, With<Player>>,
    mut chest_q: Query<(Entity, &Transform, &mut Chest)>,
    mut loot_ev: MessageWriter<LootCollectedEvent>,
    mut chest_ev: MessageWriter<ChestOpenedEvent>,
    mut inventory_ev: MessageWriter<InventoryChangedEvent>,
    mut score: ResMut<PlayerScore>,
) {
    for (_entity, chest_transform, mut chest) in chest_q.iter_mut() {
        if chest.is_open {
            continue;
        }

        let Some((player_entity, _)) = player_q
            .iter()
            .filter_map(|(player_entity, player_transform)| {
                let dist = player_transform
                    .translation
                    .distance(chest_transform.translation);
                (dist <= 2.0).then_some((player_entity, dist))
            })
            .min_by(|(_, left_dist), (_, right_dist)| left_dist.total_cmp(right_dist))
        else {
            continue;
        };

        // Open the chest
        chest.is_open = true;
        score.chests_opened += 1;
        chest_ev.write(ChestOpenedEvent);

        let amount = chest.loot_amount;
        match chest.loot_type {
            LootType::Credits => {
                if let Ok((mut stats, _, _, _)) = player_loot_q.get_mut(player_entity) {
                    stats.credits += amount;
                }
            }
            LootType::Health => {
                if let Ok(mut h) = player_health_q.get_mut(player_entity) {
                    h.heal(amount as f32);
                }
            }
            LootType::Armor => {
                if let Ok((mut stats, _, _, _)) = player_loot_q.get_mut(player_entity) {
                    stats.armor = (stats.armor + amount as f32).min(stats.max_armor);
                }
            }
            LootType::Ammo => {
                if let Ok((_, mut inventory, mut weapons, _)) = player_loot_q.get_mut(player_entity)
                {
                    let active = weapons.active_mut();
                    active.ammo = active.ammo.saturating_add(amount).min(active.max_ammo);
                    let item_id = ammo_item_for(active.weapon_type);
                    let _ = inventory.add_item(item_id, amount, max_stack_for(item_id));
                    inventory_ev.write(InventoryChangedEvent);
                }
            }
            LootType::WeaponUpgrade => {
                if let Ok((_, mut inventory, mut weapons, mut progression)) =
                    player_loot_q.get_mut(player_entity)
                {
                    if !upgrade_active_weapon(&mut progression, &mut weapons) {
                        // A max-rank cache remains valuable instead of becoming
                        // a no-op: convert it into universal upgrade currency.
                        let _ = inventory.add_item("gear", 5, max_stack_for("gear"));
                        inventory_ev.write(InventoryChangedEvent);
                    }
                }
            }
        }

        loot_ev.write(LootCollectedEvent {
            loot_type: format!("{:?}", chest.loot_type),
            amount,
        });

        // `animate_open_chests` owns the readable lid/burst presentation and
        // despawns the hierarchy after it has had time to play.
    }
}

fn animate_open_chests(
    time: Res<Time>,
    mut commands: Commands,
    mut chest_q: Query<(Entity, &mut Chest, &mut PointLight)>,
    mut lid_q: Query<(&ChildOf, &mut Transform), With<ChestLid>>,
) {
    let dt = time.delta_secs();
    for (entity, mut chest, mut light) in chest_q.iter_mut() {
        if !chest.is_open {
            continue;
        }
        chest.open_timer += dt;
        let progress = (chest.open_timer / 0.55).clamp(0.0, 1.0);
        for (parent, mut lid_transform) in lid_q.iter_mut() {
            if parent.parent() != entity {
                continue;
            }
            lid_transform.rotation = Quat::from_rotation_x(-progress * 1.35);
            lid_transform.translation.y = 0.58 + progress * 0.18;
            lid_transform.translation.z = progress * 0.28;
        }
        light.intensity = if chest.open_timer < 0.22 {
            5_000.0 + chest.open_timer / 0.22 * 11_000.0
        } else {
            (16_000.0 * (1.0 - (chest.open_timer - 0.22) / 1.18)).max(0.0)
        };
        if chest.open_timer >= 1.4 {
            commands.entity(entity).despawn();
        }
    }
}

fn ammo_item_for(weapon_type: WeaponType) -> &'static str {
    match weapon_type {
        WeaponType::Pistol | WeaponType::Laser => "plasma_cell",
        WeaponType::Rifle | WeaponType::Shotgun => "kinetic_rounds",
        WeaponType::Rocket => "rocket_ammo",
        WeaponType::Grenade => "grenade_pack",
    }
}

fn upgrade_active_weapon(
    progression: &mut PlayerProgression,
    weapons: &mut WeaponInventory,
) -> bool {
    let slot = weapons.active_slot.min(weapons.slots.len() - 1);
    let Some(rank) = progression.weapon_ranks.ranks.get_mut(slot) else {
        return false;
    };
    if *rank >= MAX_WEAPON_RANK {
        return false;
    }
    *rank += 1;
    weapons.slots[slot].rank = *rank;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ammo_cache_maps_every_primary_weapon_to_real_inventory_ammo() {
        let pairs = [
            (WeaponType::Pistol, "plasma_cell"),
            (WeaponType::Rifle, "kinetic_rounds"),
            (WeaponType::Shotgun, "kinetic_rounds"),
            (WeaponType::Rocket, "rocket_ammo"),
            (WeaponType::Laser, "plasma_cell"),
            (WeaponType::Grenade, "grenade_pack"),
        ];
        for (weapon_type, expected) in pairs {
            assert_eq!(ammo_item_for(weapon_type), expected);
        }
    }

    #[test]
    fn weapon_upgrade_cache_advances_active_owner_rank_once() {
        let mut progression = PlayerProgression::default();
        let mut weapons = WeaponInventory {
            active_slot: 4,
            ..default()
        };

        assert!(upgrade_active_weapon(&mut progression, &mut weapons));
        assert_eq!(progression.weapon_ranks.ranks[4], 1);
        assert_eq!(weapons.slots[4].rank, 1);

        progression.weapon_ranks.ranks[4] = MAX_WEAPON_RANK;
        assert!(!upgrade_active_weapon(&mut progression, &mut weapons));
        assert_eq!(progression.weapon_ranks.ranks[4], MAX_WEAPON_RANK);
    }
}
