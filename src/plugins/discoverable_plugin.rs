//! Discoverable pickup plugin — collects beacons placed by the chapter director,
//! applies their effect (blueprint, mod, companion recruit, beam-sabre unlock).

use bevy::prelude::*;

use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleArchetype, PuzzleNode, PuzzleNodeKind,
    PuzzleRelicEncounter, RelicFragmentObstacle, RelicFragmentPuzzlePiece,
};
use crate::components::mods::{ArmorMod, PlayerLoadout, WeaponMod};
use crate::components::player::{Player, PlayerIndex};
use crate::components::weapon::{BeamSabre, BeamSabreLocked};
use crate::events::*;
use crate::plugins::chapter_plugin::spawn_discoverable_beacon;
use crate::resources::{ChapterProgress, CurrentChapter};
use crate::state::AppState;

pub struct DiscoverablePlugin;

impl Plugin for DiscoverablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerLoadout>().add_systems(
            Update,
            (
                beacon_bob_system,
                puzzle_switch_bob_system,
                fragment_obstacle_system,
                relic_puzzle_system,
                discoverable_pickup_system,
            )
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn beacon_bob_system(time: Res<Time>, mut q: Query<(&mut Transform, &mut Discoverable)>) {
    let dt = time.delta_secs();
    for (mut t, mut d) in q.iter_mut() {
        d.bob_phase += dt * 2.5;
        let bob = d.bob_phase.sin() * 0.25;
        t.translation.y = t.translation.y * 0.99 + (d.base_y + bob) * 0.01;
        t.rotation = Quat::from_rotation_y(d.bob_phase);
    }
}

fn puzzle_switch_bob_system(time: Res<Time>, mut q: Query<(&mut Transform, &mut PuzzleNode)>) {
    let dt = time.delta_secs();
    for (mut t, mut node) in q.iter_mut() {
        node.bob_phase += dt * 1.8;
        match node.kind {
            PuzzleNodeKind::FloorPlate => {
                let target = if node.active { 0.08 } else { 0.18 };
                t.translation.y = t.translation.y * 0.88 + target * 0.12;
            }
            _ => {
                let bob = if node.active { 0.35 } else { 0.15 } + node.bob_phase.sin() * 0.10;
                t.translation.y = t.translation.y * 0.94 + (0.9 + bob) * 0.06;
                t.rotate_y(dt * 0.8);
            }
        }
    }
}

fn fragment_obstacle_system(
    time: Res<Time>,
    mut q: Query<(&RelicFragmentObstacle, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (obstacle, mut transform) in q.iter_mut() {
        let wave = (elapsed * obstacle.speed + obstacle.phase).sin();
        transform.translation = obstacle.base + obstacle.travel * wave;
        if obstacle.spin_speed.abs() > f32::EPSILON {
            transform.rotation =
                Quat::from_rotation_y(elapsed * obstacle.spin_speed + obstacle.phase);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn relic_puzzle_system(
    mut commands: Commands,
    time: Res<Time>,
    player_q: Query<&Transform, With<Player>>,
    mut node_q: Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut encounter_q: Query<&mut PuzzleRelicEncounter>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut msg_ev: EventWriter<UiMessageEvent>,
    mut radio_ev: EventWriter<RadioChatterEvent>,
) {
    let Ok(mut encounter) = encounter_q.get_single_mut() else {
        return;
    };
    let player_positions: Vec<Vec3> = player_q.iter().map(|t| t.translation).collect();
    if player_positions.is_empty() {
        return;
    }

    if encounter.solved {
        if !encounter.reward_spawned {
            spawn_discoverable_beacon(
                &mut commands,
                &mut meshes,
                &mut materials,
                encounter.kind.clone(),
                encounter.label,
                encounter.reward_position,
            );
            encounter.reward_spawned = true;
            radio_ev.send(RadioChatterEvent {
                speaker: encounter.scientist.into(),
                text: format!(
                    "You restored my relic. Grab {} before the thieves regroup.",
                    encounter.label
                ),
                faction: crate::components::faction::Faction::WizardScientist,
                duration: 4.0,
            });
        }
        return;
    }

    match encounter.archetype.clone() {
        PuzzleArchetype::OrderedSwitches => update_ordered_switches(
            &player_positions,
            &mut node_q,
            &mut encounter,
            &mut materials,
            &mut msg_ev,
        ),
        PuzzleArchetype::TimedCrystalChain { window_secs } => update_timed_chain(
            &player_positions,
            &mut node_q,
            &mut encounter,
            &mut materials,
            &mut msg_ev,
            time.delta_secs(),
            window_secs,
        ),
        PuzzleArchetype::CoOpFloorPlates {
            hold_secs,
            required_players,
        } => update_floor_plates(
            &player_positions,
            &mut node_q,
            &mut encounter,
            &mut materials,
            time.delta_secs(),
            hold_secs,
            required_players,
        ),
        PuzzleArchetype::BeamRouting => update_beam_routing(
            &player_positions,
            &mut node_q,
            &mut encounter,
            &mut materials,
            &mut msg_ev,
        ),
    }

    if encounter.active_nodes >= encounter.total_nodes
        || matches!(
            encounter.archetype,
            PuzzleArchetype::CoOpFloorPlates { .. } if encounter.solved
        )
    {
        encounter.solved = true;
    }

    if encounter.solved {
        msg_ev.send(UiMessageEvent {
            text: format!("Puzzle solved: {}", encounter.label),
            duration: 4.0,
        });
        radio_ev.send(RadioChatterEvent {
            speaker: encounter.scientist.into(),
            text: format!("Excellent. The path to {} is open.", encounter.label),
            faction: crate::components::faction::Faction::WizardScientist,
            duration: 3.5,
        });
        for (entity, _, node, _) in node_q.iter_mut() {
            if node.relic_id == encounter.relic_id && node.scientist == encounter.scientist {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

fn update_ordered_switches(
    player_positions: &[Vec3],
    node_q: &mut Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    encounter: &mut PuzzleRelicEncounter,
    materials: &mut Assets<StandardMaterial>,
    msg_ev: &mut EventWriter<UiMessageEvent>,
) {
    let mut expected_node: Option<Entity> = None;
    let mut wrong_node_hit = false;
    for (entity, transform, node, _) in node_q.iter_mut() {
        if node.relic_id != encounter.relic_id || node.scientist != encounter.scientist {
            continue;
        }
        let touched = touching_node(transform.translation, player_positions, 2.4);
        if !touched || node.active {
            continue;
        }
        if node.order == encounter.next_switch_index {
            expected_node = Some(entity);
            break;
        }
        wrong_node_hit = true;
    }

    if wrong_node_hit && encounter.next_switch_index > 0 {
        reset_nodes(node_q, materials, encounter, false);
        msg_ev.send(UiMessageEvent {
            text: format!("{} reset. {}", encounter.label, encounter.hint),
            duration: 3.5,
        });
        return;
    }

    if let Some(target) = expected_node {
        if let Ok((_, _, mut node, material)) = node_q.get_mut(target) {
            node.active = true;
            set_node_material(materials, material, node.kind, true);
            encounter.next_switch_index += 1;
            encounter.active_nodes = encounter.next_switch_index;
            msg_ev.send(UiMessageEvent {
                text: format!(
                    "{} switch {}/{} aligned",
                    encounter.scientist, encounter.active_nodes, encounter.total_nodes
                ),
                duration: 2.2,
            });
        }
    }
}

fn update_timed_chain(
    player_positions: &[Vec3],
    node_q: &mut Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    encounter: &mut PuzzleRelicEncounter,
    materials: &mut Assets<StandardMaterial>,
    msg_ev: &mut EventWriter<UiMessageEvent>,
    dt: f32,
    window_secs: f32,
) {
    if encounter.active_nodes > 0 {
        encounter.timer_remaining -= dt;
        if encounter.timer_remaining <= 0.0 {
            reset_nodes(node_q, materials, encounter, false);
            encounter.timer_remaining = window_secs;
            msg_ev.send(UiMessageEvent {
                text: format!("{} faded out. Restart the crystal chain.", encounter.label),
                duration: 3.0,
            });
            return;
        }
    }

    let mut changed = false;
    for (_, transform, mut node, material) in node_q.iter_mut() {
        if node.relic_id != encounter.relic_id || node.scientist != encounter.scientist {
            continue;
        }
        if node.active || !touching_node(transform.translation, player_positions, 2.4) {
            continue;
        }
        node.active = true;
        set_node_material(materials, material, node.kind, true);
        encounter.active_nodes += 1;
        changed = true;
    }
    if changed && encounter.active_nodes == 1 {
        encounter.timer_remaining = window_secs;
    }
}

fn update_floor_plates(
    player_positions: &[Vec3],
    node_q: &mut Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    encounter: &mut PuzzleRelicEncounter,
    materials: &mut Assets<StandardMaterial>,
    dt: f32,
    hold_secs: f32,
    required_players: usize,
) {
    let required_active = required_players
        .min(player_positions.len().max(1))
        .min(encounter.total_nodes)
        .max(1);
    let mut active_now = 0;

    for (_, transform, mut node, material) in node_q.iter_mut() {
        if node.relic_id != encounter.relic_id || node.scientist != encounter.scientist {
            continue;
        }
        let active = touching_node(transform.translation, player_positions, 1.8);
        if node.active != active {
            node.active = active;
            set_node_material(materials, material, node.kind, active);
        }
        if active {
            active_now += 1;
        }
    }

    encounter.active_nodes = active_now;
    if active_now >= required_active {
        encounter.hold_progress += dt;
        if encounter.hold_progress >= hold_secs {
            encounter.solved = true;
            encounter.active_nodes = encounter.total_nodes;
        }
    } else {
        encounter.hold_progress = (encounter.hold_progress - dt * 0.5).max(0.0);
    }
}

fn update_beam_routing(
    player_positions: &[Vec3],
    node_q: &mut Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    encounter: &mut PuzzleRelicEncounter,
    materials: &mut Assets<StandardMaterial>,
    msg_ev: &mut EventWriter<UiMessageEvent>,
) {
    let mut next_node: Option<Entity> = None;
    let mut wrong_node_hit = false;

    for (entity, transform, node, _) in node_q.iter_mut() {
        if node.relic_id != encounter.relic_id || node.scientist != encounter.scientist {
            continue;
        }
        if !touching_node(transform.translation, player_positions, 2.6) || node.active {
            continue;
        }
        if node.order == encounter.next_switch_index {
            next_node = Some(entity);
            break;
        }
        wrong_node_hit = true;
    }

    if wrong_node_hit && encounter.next_switch_index > 1 {
        reset_nodes(node_q, materials, encounter, true);
        msg_ev.send(UiMessageEvent {
            text: format!(
                "Beam route collapsed. Re-align the relays for {}.",
                encounter.label
            ),
            duration: 3.5,
        });
        return;
    }

    if let Some(target) = next_node {
        if let Ok((_, _, mut node, material)) = node_q.get_mut(target) {
            node.active = true;
            set_node_material(materials, material, node.kind, true);
            encounter.next_switch_index += 1;
            encounter.active_nodes = encounter.next_switch_index;
        }
    }
}

fn touching_node(position: Vec3, player_positions: &[Vec3], radius: f32) -> bool {
    player_positions
        .iter()
        .any(|player_position| player_position.distance(position) <= radius)
}

fn reset_nodes(
    node_q: &mut Query<(
        Entity,
        &Transform,
        &mut PuzzleNode,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    materials: &mut Assets<StandardMaterial>,
    encounter: &mut PuzzleRelicEncounter,
    keep_source_active: bool,
) {
    encounter.active_nodes = if keep_source_active {
        1.min(encounter.total_nodes)
    } else {
        0
    };
    encounter.next_switch_index = encounter.active_nodes;
    encounter.hold_progress = 0.0;
    for (_, _, mut node, material) in node_q.iter_mut() {
        if node.relic_id != encounter.relic_id || node.scientist != encounter.scientist {
            continue;
        }
        node.active = keep_source_active && node.order == 0;
        set_node_material(materials, material, node.kind, node.active);
    }
}

fn set_node_material(
    materials: &mut Assets<StandardMaterial>,
    material: &MeshMaterial3d<StandardMaterial>,
    node_kind: PuzzleNodeKind,
    active: bool,
) {
    if let Some(mat) = materials.get_mut(&material.0) {
        let (base_color, emissive) = match (node_kind, active) {
            (PuzzleNodeKind::SwitchPylon, false) => (
                Color::srgb(0.16, 0.24, 0.85),
                LinearRgba::new(0.3, 0.5, 2.8, 1.0),
            ),
            (PuzzleNodeKind::SwitchPylon, true) => (
                Color::srgb(0.95, 0.78, 0.22),
                LinearRgba::new(4.2, 3.0, 0.3, 1.0),
            ),
            (PuzzleNodeKind::Crystal, false) => (
                Color::srgb(0.50, 0.18, 0.95),
                LinearRgba::new(1.0, 0.3, 3.8, 1.0),
            ),
            (PuzzleNodeKind::Crystal, true) => (
                Color::srgb(0.20, 0.90, 1.0),
                LinearRgba::new(0.8, 3.0, 4.5, 1.0),
            ),
            (PuzzleNodeKind::FloorPlate, false) => (
                Color::srgb(0.12, 0.38, 0.42),
                LinearRgba::new(0.1, 0.5, 0.6, 1.0),
            ),
            (PuzzleNodeKind::FloorPlate, true) => (
                Color::srgb(0.25, 0.95, 0.70),
                LinearRgba::new(0.5, 3.5, 2.0, 1.0),
            ),
            (PuzzleNodeKind::Relay, false) => (
                Color::srgb(0.90, 0.35, 0.10),
                LinearRgba::new(2.2, 0.7, 0.1, 1.0),
            ),
            (PuzzleNodeKind::Relay, true) => (
                Color::srgb(1.0, 0.85, 0.25),
                LinearRgba::new(4.4, 2.8, 0.3, 1.0),
            ),
        };
        mat.base_color = base_color;
        mat.emissive = emissive;
    }
}

#[allow(clippy::too_many_arguments)]
fn discoverable_pickup_system(
    mut commands: Commands,
    player_q: Query<(Entity, &PlayerIndex, &Transform), With<Player>>,
    disc_q: Query<(Entity, &Transform, &Discoverable)>,
    encounter_q: Query<(Entity, &PuzzleRelicEncounter)>,
    fragment_piece_q: Query<(Entity, &RelicFragmentPuzzlePiece)>,
    mut beam_q: Query<&mut BeamSabre>,
    mut progress: ResMut<ChapterProgress>,
    mut current: ResMut<CurrentChapter>,
    mut loadout: ResMut<PlayerLoadout>,
    mut msg_ev: EventWriter<UiMessageEvent>,
    mut radio_ev: EventWriter<RadioChatterEvent>,
    mut disc_ev: EventWriter<DiscoverableCollectedEvent>,
    mut companion_ev: EventWriter<CompanionRecruitedEvent>,
) {
    for (e, t, d) in disc_q.iter() {
        let Some((player_entity, player_index, _)) =
            player_q.iter().find(|(_, _, player_transform)| {
                player_transform.translation.distance(t.translation) <= 2.5
            })
        else {
            continue;
        };
        match &d.kind {
            DiscoverableKind::Blueprint(id) => {
                loadout.add_blueprint(*id);
                progress.unlock(id);
                msg_ev.send(UiMessageEvent {
                    text: format!("Blueprint acquired: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::WeaponMod(id) => {
                let m = match *id {
                    "homing_star" => WeaponMod::homing_star(),
                    "piercing_rounds" => WeaponMod::piercing_rounds(),
                    _ => WeaponMod::piercing_rounds(),
                };
                loadout.equip_weapon_mod(crate::components::weapon::WeaponType::Rifle, m);
                progress.unlock(id);
                msg_ev.send(UiMessageEvent {
                    text: format!("Weapon mod: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::ArmorMod(id) => {
                let m = match *id {
                    "reactive_plating" => ArmorMod::reactive_plating(),
                    "coolant_weave" => ArmorMod::coolant_weave(),
                    _ => ArmorMod::reactive_plating(),
                };
                loadout.add_armor_mod(m);
                progress.unlock(id);
                msg_ev.send(UiMessageEvent {
                    text: format!("Armor mod: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::CompanionRecruit(name) => {
                progress.recruit(name);
                companion_ev.send(CompanionRecruitedEvent {
                    name: (*name).into(),
                    player_index: player_index.0,
                });
                radio_ev.send(RadioChatterEvent {
                    speaker: (*name).into(),
                    text: format!("{} stands with you.", name),
                    faction: crate::components::faction::Faction::HeroBrother,
                    duration: 3.0,
                });
            }
            DiscoverableKind::BeamSabreUnlock => {
                if let Ok(mut beam) = beam_q.get_single_mut() {
                    beam.unlocked = true;
                }
                commands.entity(player_entity).remove::<BeamSabreLocked>();
                progress.unlock("star_sabre");
                msg_ev.send(UiMessageEvent {
                    text: "Star Sabre online - press T".into(),
                    duration: 4.0,
                });
            }
            DiscoverableKind::ScientistRelic {
                scientist,
                relic_id,
            } => {
                progress.recover_relic(scientist, relic_id);
                progress.unlock(relic_id);
                current.awaiting_puzzle = false;
                msg_ev.send(UiMessageEvent {
                    text: format!("Recovered relic: {}", d.label),
                    duration: 4.0,
                });
                radio_ev.send(RadioChatterEvent {
                    speaker: (*scientist).into(),
                    text: format!(
                        "The {} is back in our hands. One more stolen treasure reclaimed.",
                        d.label
                    ),
                    faction: crate::components::faction::Faction::WizardScientist,
                    duration: 4.0,
                });
                for (encounter_entity, encounter) in encounter_q.iter() {
                    if encounter.scientist == *scientist && encounter.relic_id == *relic_id {
                        commands.entity(encounter_entity).despawn_recursive();
                    }
                }
            }
            DiscoverableKind::RelicFragment {
                scientist,
                relic_id,
                piece,
                total,
            } => {
                let was_new = progress.recover_relic_fragment(scientist, relic_id, *piece);
                let recovered = progress.relic_fragment_count(scientist, relic_id);
                if recovered >= *total as usize {
                    progress.recover_relic(scientist, relic_id);
                    progress.unlock(relic_id);
                    current.awaiting_puzzle = false;
                    msg_ev.send(UiMessageEvent {
                        text: format!("Assembled relic: {} ({}/{})", d.label, total, total),
                        duration: 4.5,
                    });
                    radio_ev.send(RadioChatterEvent {
                        speaker: (*scientist).into(),
                        text: format!(
                            "All five fragments of {} are back together. Bring it home.",
                            d.label
                        ),
                        faction: crate::components::faction::Faction::WizardScientist,
                        duration: 4.0,
                    });
                    for (piece_entity, puzzle_piece) in fragment_piece_q.iter() {
                        if puzzle_piece.scientist == *scientist
                            && puzzle_piece.relic_id == *relic_id
                        {
                            commands.entity(piece_entity).despawn_recursive();
                        }
                    }
                } else if was_new {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Relic fragment {}/{}: {}", recovered, total, d.label),
                        duration: 3.0,
                    });
                } else {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Relic fragment already recovered: {}", d.label),
                        duration: 2.5,
                    });
                }
            }
            DiscoverableKind::SecretCave { chapter, cave_id } => {
                let was_new = !progress.has_discoverable(cave_id);
                progress.unlock(cave_id);
                if was_new {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Secret cave discovered: {}", d.label),
                        duration: 4.0,
                    });
                    radio_ev.send(RadioChatterEvent {
                        speaker: "Giacoma".into(),
                        text: format!(
                            "Chapter {} cave charted. Marking {} on the family map.",
                            chapter, d.label
                        ),
                        faction: crate::components::faction::Faction::WizardScientist,
                        duration: 4.0,
                    });
                } else {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Secret cave already charted: {}", d.label),
                        duration: 2.5,
                    });
                }
            }
            DiscoverableKind::LoreFragment(text) => {
                msg_ev.send(UiMessageEvent {
                    text: format!("LORE: {}", text),
                    duration: 5.0,
                });
            }
        }
        disc_ev.send(DiscoverableCollectedEvent {
            kind_label: d.label.into(),
            raw_id: format!("{:?}", d.kind),
        });
        commands.entity(e).despawn_recursive();
    }
}
