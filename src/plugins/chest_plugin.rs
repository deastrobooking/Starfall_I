use bevy::prelude::*;
use rand::Rng;

use crate::components::player::{Player, PlayerStats};
use crate::components::world::{Chest, LootType};
use crate::damage::Health;
use crate::events::{ChestOpenedEvent, LootCollectedEvent};
use crate::rendering::PbrBundle;
use crate::resources::{PlaySessionTransition, PlayerScore};
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_chests)
            .add_systems(OnEnter(AppState::MainMenu), cleanup_chests_for_menu)
            .add_systems(OnExit(AppState::Playing), cleanup_chests)
            .add_systems(
                Update,
                chest_proximity_system.run_if(in_state(AppState::Playing)),
            );
    }
}

fn spawn_chests(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transition: Res<PlaySessionTransition>,
    existing_chests: Query<Entity, With<Chest>>,
) {
    if transition.resuming_from_pause || !existing_chests.is_empty() {
        return;
    }

    let mut rng = rand::thread_rng();
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

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.5, 1.2, 1.5))),
                material: MeshMaterial3d(gold_mat.clone()),
                transform: Transform::from_xyz(x, 0.6, z),
                ..default()
            },
            Chest::new(loot_type, amount),
            PointLight {
                color: Color::srgb(1.0, 0.85, 0.2),
                intensity: 5_000.0,
                range: 8.0,
                ..default()
            },
        ));
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
    mut commands: Commands,
    player_q: Query<(Entity, &Transform), With<Player>>,
    mut player_stats_q: Query<&mut PlayerStats, With<Player>>,
    mut player_health_q: Query<&mut Health, With<Player>>,
    mut chest_q: Query<(Entity, &Transform, &mut Chest)>,
    mut loot_ev: MessageWriter<LootCollectedEvent>,
    mut chest_ev: MessageWriter<ChestOpenedEvent>,
    mut score: ResMut<PlayerScore>,
) {
    for (entity, chest_transform, mut chest) in chest_q.iter_mut() {
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
                if let Ok(mut stats) = player_stats_q.get_mut(player_entity) {
                    stats.credits += amount;
                }
            }
            LootType::Health => {
                if let Ok(mut h) = player_health_q.get_mut(player_entity) {
                    h.heal(amount as f32);
                }
            }
            LootType::Armor => {
                if let Ok(mut stats) = player_stats_q.get_mut(player_entity) {
                    stats.armor = (stats.armor + amount as f32).min(stats.max_armor);
                }
            }
            LootType::Ammo | LootType::WeaponUpgrade => {}
        }

        loot_ev.write(LootCollectedEvent {
            loot_type: format!("{:?}", chest.loot_type),
            amount,
        });

        // Despawn after short delay (simplified: despawn immediately)
        commands.entity(entity).despawn();
    }
}
