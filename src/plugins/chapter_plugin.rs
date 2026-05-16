//! Chapter director - the heart of the Starfall I gameplay loop.
//!
//! Replaces the old wave timer/boss-spawn logic. On `OnEnter(Playing)` the
//! director starts the chapter selected via `CurrentChapter`. Each frame, the
//! `chapter_director_system` advances through the script.

use bevy::prelude::*;

use crate::chapters::{get_chapter, ChapterId, EncounterStep};
use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleArchetype, PuzzleNode, PuzzleNodeKind,
    PuzzleRelicEncounter,
};
use crate::components::enemy::BossEnemy;
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::player::Player;
use crate::components::world::WorldAnchor;
use crate::events::*;
use crate::plugins::enemy_plugin::{random_spawn_pos, spawn_enemy_entity, spawn_named_enemy};
use crate::resources::{BiomePalette, ChapterProgress, CurrentChapter, WaveInfo};
use crate::state::AppState;

pub struct ChapterPlugin;

impl Plugin for ChapterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentChapter>()
            .init_resource::<BiomePalette>()
            .init_resource::<ChapterProgress>()
            .add_systems(OnEnter(AppState::Playing), start_chapter)
            .add_systems(
                Update,
                (
                    chapter_director_system,
                    track_kills_system,
                    chapter_complete_check,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Start Chapter ─────────────────────────────────────────────────────────────
fn start_chapter(
    mut current: ResMut<CurrentChapter>,
    mut palette: ResMut<BiomePalette>,
    mut started_ev: EventWriter<ChapterStartedEvent>,
    mut wave: ResMut<WaveInfo>,
) {
    let Some(def) = get_chapter(current.id) else {
        return;
    };
    current.biome = def.biome;
    current.difficulty_scale = def.difficulty_scale;
    current.step_index = 0;
    current.step_timer = 0.0;
    current.awaiting_kills = 0;
    current.awaiting_puzzle = false;
    current.completed = false;
    current.started = true;
    let (sky, fog, ground, accent) = def.biome.palette();
    *palette = BiomePalette {
        sky,
        fog,
        ground,
        accent,
    };
    *wave = WaveInfo::new();
    wave.wave_number = current.id.0 as u32;
    started_ev.send(ChapterStartedEvent {
        chapter: current.id.0,
    });
}

// ── Director ──────────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn chapter_director_system(
    time: Res<Time>,
    mut current: ResMut<CurrentChapter>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_q: Query<&Transform, With<Player>>,
    anchor_q: Query<(&WorldAnchor, &Transform)>,
    mut wave: ResMut<WaveInfo>,
    progress: Res<ChapterProgress>,
    mut radio_ev: EventWriter<RadioChatterEvent>,
    mut step_ev: EventWriter<EncounterStepAdvancedEvent>,
    mut completed_ev: EventWriter<ChapterCompletedEvent>,
    mut boss_spawned_ev: EventWriter<BossSpawnedEvent>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if !current.started || current.completed {
        return;
    }
    let Some(def) = get_chapter(current.id) else {
        return;
    };
    if current.step_index >= def.script.len() {
        current.completed = true;
        completed_ev.send(ChapterCompletedEvent {
            chapter: current.id.0,
        });
        msg_ev.send(UiMessageEvent {
            text: format!("CHAPTER {} COMPLETE — {}", current.id.0, def.title),
            duration: 6.0,
        });
        return;
    }

    current.step_timer += time.delta_secs();
    let step = def.script[current.step_index].clone();

    // If we're awaiting kills, hold until the count drops to zero.
    if current.awaiting_kills > 0 || current.awaiting_puzzle {
        return;
    }

    let Ok(player_transform) = player_q.get_single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let mut rng = rand::thread_rng();

    let mut advance = false;

    match step {
        EncounterStep::Dialogue {
            speaker,
            faction,
            line,
            hold,
        } => {
            if current.step_timer < 0.05 {
                radio_ev.send(RadioChatterEvent {
                    speaker: speaker.into(),
                    text: line.into(),
                    faction,
                    duration: hold + 1.0,
                });
            }
            if current.step_timer >= hold {
                advance = true;
            }
        }
        EncounterStep::SpawnGroup {
            faction,
            enemy_type,
            count,
            scale,
        } => {
            for _ in 0..count {
                let pos = random_spawn_pos(player_pos, &mut rng);
                spawn_enemy_entity(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    enemy_type,
                    pos,
                    scale * current.difficulty_scale,
                    Some(faction),
                );
                wave.enemy_count += 1;
            }
            current.awaiting_kills = count;
            advance = true;
        }
        EncounterStep::MidBoss {
            preset,
            name,
            faction,
            scale,
        } => {
            let pos = player_pos + Vec3::new(20.0, 0.0, 20.0);
            spawn_named_enemy(
                &mut commands,
                &mut meshes,
                &mut materials,
                preset,
                name,
                faction,
                pos,
                scale * current.difficulty_scale,
                false,
            );
            wave.enemy_count += 1;
            current.awaiting_kills = 1;
            boss_spawned_ev.send(BossSpawnedEvent {
                wave: wave.wave_number,
                position: pos,
            });
            radio_ev.send(RadioChatterEvent {
                speaker: name.into(),
                text: format!("{} approaches.", name),
                faction,
                duration: 3.0,
            });
            advance = true;
        }
        EncounterStep::BossFight {
            preset,
            name,
            faction,
            intro_line,
            scale,
        } => {
            let pos = player_pos + Vec3::new(25.0, 0.0, 25.0);
            spawn_named_enemy(
                &mut commands,
                &mut meshes,
                &mut materials,
                preset,
                name,
                faction,
                pos,
                scale * current.difficulty_scale * 1.5,
                true,
            );
            wave.enemy_count += 1;
            current.awaiting_kills = 1;
            radio_ev.send(RadioChatterEvent {
                speaker: name.into(),
                text: intro_line.into(),
                faction,
                duration: 5.0,
            });
            msg_ev.send(UiMessageEvent {
                text: format!("!! BOSS — {} !!", name),
                duration: 4.0,
            });
            advance = true;
        }
        EncounterStep::PlaceDiscoverable {
            kind,
            label,
            offset,
        } => {
            spawn_discoverable_beacon(
                &mut commands,
                &mut meshes,
                &mut materials,
                kind,
                label,
                player_pos + offset,
            );
            advance = true;
        }
        EncounterStep::PlaceRelicPuzzle {
            scientist,
            relic_id,
            label,
            hint,
            archetype,
            reward_anchor,
            node_anchors,
        } => {
            if progress.has_relic(scientist, relic_id) {
                msg_ev.send(UiMessageEvent {
                    text: format!("Recovered relic already secured: {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.send(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            }
            let Some(reward_position) = resolve_anchor_position(&anchor_q, reward_anchor) else {
                msg_ev.send(UiMessageEvent {
                    text: format!("Missing puzzle reward anchor: {}", reward_anchor),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.send(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            };
            let Some(node_positions) = resolve_anchor_positions(&anchor_q, &node_anchors) else {
                msg_ev.send(UiMessageEvent {
                    text: format!("Missing puzzle node anchors for {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.send(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            };
            spawn_relic_puzzle(
                &mut commands,
                &mut meshes,
                &mut materials,
                scientist,
                relic_id,
                label,
                hint,
                archetype,
                reward_position,
                &node_positions,
            );
            current.awaiting_puzzle = true;
            msg_ev.send(UiMessageEvent {
                text: format!("Puzzle unlocked: {}", hint),
                duration: 5.0,
            });
            advance = true;
        }
        EncounterStep::Outro { line } => {
            if current.step_timer < 0.05 {
                radio_ev.send(RadioChatterEvent {
                    speaker: "—".into(),
                    text: line.into(),
                    faction: Faction::HeroBrother,
                    duration: 5.0,
                });
            }
            if current.step_timer >= 4.5 {
                current.step_index = def.script.len(); // jump to end
                return;
            }
        }
    }

    if advance {
        current.step_index += 1;
        current.step_timer = 0.0;
        step_ev.send(EncounterStepAdvancedEvent {
            step_index: current.step_index,
        });
    }
}

fn resolve_anchor_position(
    anchor_q: &Query<(&WorldAnchor, &Transform)>,
    anchor_id: &'static str,
) -> Option<Vec3> {
    anchor_q
        .iter()
        .find(|(anchor, _)| anchor.id == anchor_id)
        .map(|(_, transform)| transform.translation)
}

fn resolve_anchor_positions(
    anchor_q: &Query<(&WorldAnchor, &Transform)>,
    anchor_ids: &[&'static str],
) -> Option<Vec<Vec3>> {
    let mut positions = Vec::with_capacity(anchor_ids.len());
    for anchor_id in anchor_ids {
        positions.push(resolve_anchor_position(anchor_q, anchor_id)?);
    }
    Some(positions)
}

// ── Discoverable beacon spawn ─────────────────────────────────────────────────
pub(crate) fn spawn_discoverable_beacon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kind: DiscoverableKind,
    label: &'static str,
    position: Vec3,
) {
    let color = match &kind {
        DiscoverableKind::Blueprint(_) => Color::srgb(0.2, 0.7, 1.0),
        DiscoverableKind::WeaponMod(_) => Color::srgb(1.0, 0.5, 0.0),
        DiscoverableKind::ArmorMod(_) => Color::srgb(0.3, 1.0, 0.5),
        DiscoverableKind::CompanionRecruit(_) => Color::srgb(1.0, 0.85, 0.3),
        DiscoverableKind::BeamSabreUnlock => Color::srgb(0.8, 0.1, 1.0),
        DiscoverableKind::ScientistRelic { .. } => Color::srgb(1.0, 0.95, 0.45),
        DiscoverableKind::LoreFragment(_) => Color::srgb(0.7, 0.7, 0.9),
    };
    let mat = materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(
            color.to_srgba().red * 4.0,
            color.to_srgba().green * 4.0,
            color.to_srgba().blue * 4.0,
            1.0,
        ),
        unlit: false,
        metallic: 0.6,
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.7))),
            material: MeshMaterial3d(mat),
            transform: Transform::from_translation(Vec3::new(
                position.x,
                position.y + 1.0,
                position.z,
            )),
            ..default()
        },
        Discoverable::new(kind, label),
    ));
}

fn spawn_relic_puzzle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    scientist: &'static str,
    relic_id: &'static str,
    label: &'static str,
    hint: &'static str,
    archetype: PuzzleArchetype,
    reward_position: Vec3,
    node_positions: &[Vec3],
) {
    let node_kind = match archetype {
        PuzzleArchetype::OrderedSwitches => PuzzleNodeKind::SwitchPylon,
        PuzzleArchetype::TimedCrystalChain { .. } => PuzzleNodeKind::Crystal,
        PuzzleArchetype::CoOpFloorPlates { .. } => PuzzleNodeKind::FloorPlate,
        PuzzleArchetype::BeamRouting => PuzzleNodeKind::Relay,
    };
    let initial_active_nodes = if matches!(archetype, PuzzleArchetype::BeamRouting) {
        1.min(node_positions.len())
    } else {
        0
    };
    let timer_remaining = match archetype {
        PuzzleArchetype::TimedCrystalChain { window_secs } => window_secs,
        _ => 0.0,
    };

    commands.spawn(PuzzleRelicEncounter {
        relic_id,
        scientist,
        kind: DiscoverableKind::ScientistRelic {
            scientist,
            relic_id,
        },
        label,
        hint,
        archetype: archetype.clone(),
        reward_position,
        total_nodes: node_positions.len(),
        active_nodes: initial_active_nodes,
        next_switch_index: initial_active_nodes,
        timer_remaining,
        hold_progress: 0.0,
        solved: false,
        reward_spawned: false,
    });

    for (order, position) in node_positions.iter().enumerate() {
        let active = matches!(archetype, PuzzleArchetype::BeamRouting) && order == 0;
        let material = materials.add(puzzle_node_material(node_kind, active));
        let (mesh, lift) = puzzle_node_mesh(meshes, node_kind, order == 0 && active);
        commands.spawn((
            PbrBundle {
                mesh,
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(*position + Vec3::Y * lift),
                ..default()
            },
            PuzzleNode {
                relic_id,
                scientist,
                order,
                kind: node_kind,
                active,
                bob_phase: order as f32 * 0.8,
            },
        ));
    }
}

fn puzzle_node_mesh(
    meshes: &mut Assets<Mesh>,
    node_kind: PuzzleNodeKind,
    is_source: bool,
) -> (Mesh3d, f32) {
    match node_kind {
        PuzzleNodeKind::SwitchPylon => (Mesh3d(meshes.add(Cylinder::new(0.55, 1.4))), 0.7),
        PuzzleNodeKind::Crystal => (
            Mesh3d(meshes.add(Cone {
                radius: 0.95,
                height: if is_source { 2.4 } else { 2.0 },
            })),
            1.0,
        ),
        PuzzleNodeKind::FloorPlate => (Mesh3d(meshes.add(Cuboid::new(2.8, 0.35, 2.8))), 0.18),
        PuzzleNodeKind::Relay => (
            Mesh3d(meshes.add(Cylinder::new(if is_source { 0.75 } else { 0.55 }, 1.8))),
            0.9,
        ),
    }
}

fn puzzle_node_material(node_kind: PuzzleNodeKind, active: bool) -> StandardMaterial {
    let (base, emissive) = match (node_kind, active) {
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
    StandardMaterial {
        base_color: base,
        emissive,
        metallic: 0.45,
        perceptual_roughness: 0.25,
        ..default()
    }
}

// ── Track Kills (decrements awaiting_kills) ───────────────────────────────────
fn track_kills_system(
    mut killed_ev: EventReader<EnemyKilledEvent>,
    mut current: ResMut<CurrentChapter>,
    mut boss_def_ev: EventWriter<BossDefeatedEvent>,
    boss_q: Query<&NamedCharacter, With<BossEnemy>>,
) {
    for ev in killed_ev.read() {
        if current.awaiting_kills > 0 {
            current.awaiting_kills -= 1;
        }
        // Heuristic: notify boss-defeat when a NamedCharacter on a BossEnemy died.
        // (Death is handled via Health=0 elsewhere; we just emit story event when
        // count goes to zero on a boss step.)
        if current.awaiting_kills == 0 && !boss_q.is_empty() {
            for nc in boss_q.iter() {
                boss_def_ev.send(BossDefeatedEvent {
                    name: nc.display_name.into(),
                    chapter: current.id.0,
                });
            }
        }
        let _ = ev; // (use ev to avoid unused warning if not consumed)
    }
}

// ── Chapter complete → mark progress ──────────────────────────────────────────
fn chapter_complete_check(
    mut completed_ev: EventReader<ChapterCompletedEvent>,
    mut progress: ResMut<ChapterProgress>,
) {
    for ev in completed_ev.read() {
        progress.mark_completed(ChapterId(ev.chapter));
    }
}

/// Public helper — list completed chapter ids (for save/load).
pub fn completed_chapters(progress: &ChapterProgress) -> Vec<u8> {
    progress.completed.clone()
}
