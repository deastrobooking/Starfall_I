//! Chapter director - the heart of the Starfall I gameplay loop.
//!
//! Replaces the old wave timer/boss-spawn logic. On `OnEnter(Playing)` the
//! director starts the chapter selected via `CurrentChapter`. Each frame, the
//! `chapter_director_system` advances through the script.

use bevy::prelude::*;

use crate::chapters::{
    chapter_map_location, get_chapter, secret_cave_location, ChapterId, EncounterStep,
};
use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleArchetype, PuzzleNode, PuzzleNodeKind,
    PuzzleRelicEncounter, RelicFragmentObstacle, RelicFragmentPuzzlePiece,
};
use crate::components::enemy::BossEnemy;
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::player::{Player, PlayerIndex, PlayerMovement};
use crate::components::world::{
    BoatPassenger, LaserTurret, MovingPlatform, WalkableSurface, WorldAnchor, WorldGeometry,
};
use crate::events::*;
use crate::game_rng::GameRng;
use crate::physics::prelude::{Collider, RigidBody};
use crate::plugins::enemy_plugin::{random_spawn_pos, spawn_enemy_entity, spawn_named_enemy};
use crate::rendering::PbrBundle;
use crate::resources::{
    BiomePalette, ChapterProgress, CurrentChapter, DungeonCrawlState, FastTravelDestination,
    PlaySessionTransition, WaveInfo,
};
use crate::state::AppState;

pub struct ChapterPlugin;

#[derive(Component)]
struct AirshipLevelPiece;

#[derive(Resource, Default)]
struct PendingChapterTravel {
    chapter: Option<ChapterId>,
    world_anchor: Option<&'static str>,
    world_label: Option<&'static str>,
    enter_dungeon: bool,
}

impl Plugin for ChapterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentChapter>()
            .init_resource::<BiomePalette>()
            .init_resource::<ChapterProgress>()
            .init_resource::<FastTravelDestination>()
            .init_resource::<PendingChapterTravel>()
            .add_systems(OnEnter(AppState::Playing), start_chapter)
            .add_systems(
                Update,
                (
                    apply_pending_chapter_travel,
                    chapter_director_system,
                    track_kills_system,
                    chapter_complete_check,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Start Chapter ─────────────────────────────────────────────────────────────
fn start_chapter(
    mut commands: Commands,
    mut current: ResMut<CurrentChapter>,
    mut palette: ResMut<BiomePalette>,
    transition: Res<PlaySessionTransition>,
    mut started_ev: MessageWriter<ChapterStartedEvent>,
    mut wave: ResMut<WaveInfo>,
    mut pending_travel: ResMut<PendingChapterTravel>,
    mut fast_travel: ResMut<FastTravelDestination>,
    mut dungeon: ResMut<DungeonCrawlState>,
    airship_q: Query<Entity, With<AirshipLevelPiece>>,
) {
    if transition.resuming_from_pause {
        return;
    }

    for entity in airship_q.iter() {
        commands.entity(entity).despawn();
    }

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
    dungeon.clear();
    if let (Some(anchor), Some(label)) = (fast_travel.anchor_id, fast_travel.label) {
        pending_travel.chapter = None;
        pending_travel.world_anchor = Some(anchor);
        pending_travel.world_label = Some(label);
        pending_travel.enter_dungeon = fast_travel.enter_dungeon;
    } else {
        pending_travel.chapter = Some(current.id);
        pending_travel.world_anchor = None;
        pending_travel.world_label = None;
        pending_travel.enter_dungeon = false;
    }
    fast_travel.clear();
    started_ev.write(ChapterStartedEvent {
        chapter: current.id.0,
    });
}

fn apply_pending_chapter_travel(
    mut commands: Commands,
    mut pending_travel: ResMut<PendingChapterTravel>,
    mut player_q: Query<(Entity, &PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    anchor_q: Query<(&WorldAnchor, &Transform), Without<Player>>,
    mut dungeon: ResMut<DungeonCrawlState>,
) {
    if let Some(anchor_id) = pending_travel.world_anchor {
        let Some(anchor) = resolve_anchor_position(&anchor_q, anchor_id) else {
            return;
        };
        let cave = if pending_travel.enter_dungeon {
            let Some(chapter) = crate::chapters::SECRET_CAVE_LOCATIONS
                .iter()
                .find(|cave| cave.anchor_id == anchor_id)
                .map(|cave| cave.chapter)
            else {
                return;
            };
            let Some(cave) = secret_cave_location(chapter) else {
                return;
            };
            Some(cave)
        } else {
            None
        };
        move_players_to_world_anchor(&mut commands, &mut player_q, anchor);
        if let Some(cave) = cave {
            dungeon.activate(
                cave.anchor_id,
                cave.chapter,
                pending_travel.world_label.unwrap_or(cave.label),
                anchor,
                anchor,
                64.0,
            );
        }
        pending_travel.world_anchor = None;
        pending_travel.world_label = None;
        pending_travel.enter_dungeon = false;
        return;
    }
    let Some(chapter_id) = pending_travel.chapter else {
        return;
    };
    if move_players_to_chapter_location(&mut commands, &mut player_q, &anchor_q, chapter_id) {
        pending_travel.chapter = None;
    }
}

fn move_players_to_world_anchor(
    commands: &mut Commands,
    player_q: &mut Query<(Entity, &PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    anchor: Vec3,
) {
    const OFFSETS: [Vec3; 4] = [
        Vec3::new(-3.0, 1.3, -3.0),
        Vec3::new(3.0, 1.3, -3.0),
        Vec3::new(-3.0, 1.3, 3.0),
        Vec3::new(3.0, 1.3, 3.0),
    ];
    for (entity, index, mut transform, mut movement) in player_q.iter_mut() {
        transform.translation = anchor + OFFSETS[index.0.min(3) as usize];
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.clear_motor_delivery();
        movement.is_grounded = false;
        commands.entity(entity).remove::<BoatPassenger>();
    }
}

// ── Director ──────────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn chapter_director_system(
    mut game_rng: ResMut<GameRng>,
    time: Res<Time>,
    mut current: ResMut<CurrentChapter>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut player_q: Query<(&PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    anchor_q: Query<(&WorldAnchor, &Transform), Without<Player>>,
    mut wave: ResMut<WaveInfo>,
    mut progress: ResMut<ChapterProgress>,
    mut radio_ev: MessageWriter<RadioChatterEvent>,
    mut step_ev: MessageWriter<EncounterStepAdvancedEvent>,
    mut completed_ev: MessageWriter<ChapterCompletedEvent>,
    mut boss_spawned_ev: MessageWriter<BossSpawnedEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !current.started || current.completed {
        return;
    }
    let Some(def) = get_chapter(current.id) else {
        return;
    };
    if current.step_index >= def.script.len() {
        current.completed = true;
        completed_ev.write(ChapterCompletedEvent {
            chapter: current.id.0,
        });
        msg_ev.write(UiMessageEvent {
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

    let Some(player_pos) = party_anchor_position(&mut player_q) else {
        return;
    };
    let rng = game_rng.world();

    let mut advance = false;

    match step {
        EncounterStep::Dialogue {
            speaker,
            faction,
            line,
            hold,
        } => {
            if current.step_timer < 0.05 {
                radio_ev.write(RadioChatterEvent {
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
                let pos = random_spawn_pos(player_pos, rng);
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
            boss_spawned_ev.write(BossSpawnedEvent {
                wave: wave.wave_number,
                position: pos,
            });
            radio_ev.write(RadioChatterEvent {
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
            radio_ev.write(RadioChatterEvent {
                speaker: name.into(),
                text: intro_line.into(),
                faction,
                duration: 5.0,
            });
            msg_ev.write(UiMessageEvent {
                text: format!("!! BOSS — {} !!", name),
                duration: 4.0,
            });
            advance = true;
        }
        EncounterStep::AirshipEscape {
            boss_name,
            faction,
            airship_label,
            line,
            hold,
        } => {
            if current.step_timer < 0.05 {
                let airship_pos = player_pos + Vec3::new(0.0, 18.0, 46.0);
                spawn_airship_escape_prop(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    airship_pos,
                    faction,
                );
                radio_ev.write(RadioChatterEvent {
                    speaker: boss_name.into(),
                    text: line.into(),
                    faction,
                    duration: hold + 1.0,
                });
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "{} appears. Board it before {} escapes!",
                        airship_label, boss_name
                    ),
                    duration: hold + 1.0,
                });
            }
            if current.step_timer >= hold {
                advance = true;
            }
        }
        EncounterStep::AirshipDeckRaid {
            faction,
            enemy_type,
            count,
            scale,
        } => {
            let deck_center = player_pos + Vec3::new(0.0, 36.0, 86.0);
            spawn_airship_arena(
                &mut commands,
                &mut meshes,
                &mut materials,
                deck_center,
                faction,
            );
            move_players_to_airship_deck(&mut player_q, deck_center);
            for i in 0..count {
                let angle = i as f32 / count.max(1) as f32 * std::f32::consts::TAU;
                let radius_x = 22.0 + (i % 2) as f32 * 4.0;
                let radius_z = 12.0 + (i % 3) as f32 * 2.0;
                let pos =
                    deck_center + Vec3::new(angle.cos() * radius_x, 4.0, angle.sin() * radius_z);
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
            radio_ev.write(RadioChatterEvent {
                speaker: "Airship".into(),
                text: "Clear the deck, then force the boss back out.".into(),
                faction,
                duration: 4.0,
            });
            msg_ev.write(UiMessageEvent {
                text: "AIRSHIP LEVEL - clear the deck guards".into(),
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
        EncounterStep::PlaceSecretCave {
            chapter,
            cave_id,
            label,
            anchor,
        } => {
            if progress.has_discoverable(cave_id) {
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            }
            let Some(position) = resolve_anchor_position(&anchor_q, anchor) else {
                msg_ev.write(UiMessageEvent {
                    text: format!("Missing secret cave anchor: {}", anchor),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            };
            spawn_discoverable_beacon(
                &mut commands,
                &mut meshes,
                &mut materials,
                DiscoverableKind::SecretCave { chapter, cave_id },
                label,
                position,
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
                msg_ev.write(UiMessageEvent {
                    text: format!("Recovered relic already secured: {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            }
            let Some(reward_position) = resolve_anchor_position(&anchor_q, reward_anchor) else {
                msg_ev.write(UiMessageEvent {
                    text: format!("Missing puzzle reward anchor: {}", reward_anchor),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            };
            let Some(node_positions) = resolve_anchor_positions(&anchor_q, &node_anchors) else {
                msg_ev.write(UiMessageEvent {
                    text: format!("Missing puzzle node anchors for {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
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
            msg_ev.write(UiMessageEvent {
                text: format!("Puzzle unlocked: {}", hint),
                duration: 5.0,
            });
            advance = true;
        }
        EncounterStep::PlaceRelicFragmentPuzzle {
            scientist,
            relic_id,
            label,
            hint,
            total,
            center_offset,
        } => {
            if progress.has_relic(scientist, relic_id) {
                msg_ev.write(UiMessageEvent {
                    text: format!("Fragment relic already assembled: {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            }
            if progress.relic_fragment_count(scientist, relic_id) >= total as usize {
                progress.recover_relic(scientist, relic_id);
                progress.unlock(relic_id);
                msg_ev.write(UiMessageEvent {
                    text: format!("Fragment relic assembled from saved pieces: {}", label),
                    duration: 4.0,
                });
                advance = true;
                if advance {
                    current.step_index += 1;
                    current.step_timer = 0.0;
                    step_ev.write(EncounterStepAdvancedEvent {
                        step_index: current.step_index,
                    });
                }
                return;
            }
            let center = player_pos + center_offset;
            spawn_relic_fragment_puzzle(
                &mut commands,
                &mut meshes,
                &mut materials,
                &progress,
                scientist,
                relic_id,
                label,
                total,
                center,
            );
            current.awaiting_puzzle = true;
            msg_ev.write(UiMessageEvent {
                text: format!("Fragment puzzle: {}", hint),
                duration: 5.0,
            });
            radio_ev.write(RadioChatterEvent {
                speaker: scientist.into(),
                text: format!("That relic broke into {} pieces. Find them all.", total),
                faction: Faction::WizardScientist,
                duration: 4.0,
            });
            advance = true;
        }
        EncounterStep::Outro { line } => {
            if current.step_timer < 0.05 {
                radio_ev.write(RadioChatterEvent {
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
        step_ev.write(EncounterStepAdvancedEvent {
            step_index: current.step_index,
        });
    }
}

fn resolve_anchor_position<AnchorFilter: bevy::ecs::query::QueryFilter>(
    anchor_q: &Query<(&WorldAnchor, &Transform), AnchorFilter>,
    anchor_id: &'static str,
) -> Option<Vec3> {
    anchor_q
        .iter()
        .find(|(anchor, _)| anchor.id == anchor_id)
        .map(|(_, transform)| transform.translation)
}

fn resolve_anchor_positions<AnchorFilter: bevy::ecs::query::QueryFilter>(
    anchor_q: &Query<(&WorldAnchor, &Transform), AnchorFilter>,
    anchor_ids: &[&'static str],
) -> Option<Vec<Vec3>> {
    let mut positions = Vec::with_capacity(anchor_ids.len());
    for anchor_id in anchor_ids {
        positions.push(resolve_anchor_position(anchor_q, anchor_id)?);
    }
    Some(positions)
}

fn party_anchor_position(
    player_q: &mut Query<(&PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut count = 0.0;
    for (_, transform, _) in player_q.iter_mut() {
        sum += transform.translation;
        count += 1.0;
    }
    (count > 0.0).then_some(sum / count)
}

fn move_players_to_chapter_location(
    commands: &mut Commands,
    player_q: &mut Query<(Entity, &PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    anchor_q: &Query<(&WorldAnchor, &Transform), Without<Player>>,
    chapter_id: ChapterId,
) -> bool {
    let Some(location) = chapter_map_location(chapter_id) else {
        return false;
    };
    let Some(anchor) = resolve_anchor_position(anchor_q, location.anchor_id) else {
        return false;
    };

    let mut moved = false;
    for (entity, index, mut transform, mut movement) in player_q.iter_mut() {
        transform.translation = location.spawn_position(anchor, index.0);
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.coyote_timer = 0.0;
        movement.jump_buffer_timer = 0.0;
        movement.wall_jump_lock_timer = 0.0;
        movement.is_grounded = false;
        commands.entity(entity).remove::<BoatPassenger>();
        moved = true;
    }
    moved
}

fn move_players_to_airship_deck(
    player_q: &mut Query<(&PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    deck_center: Vec3,
) {
    const OFFSETS: [Vec3; 4] = [
        Vec3::new(-4.0, 6.0, -10.0),
        Vec3::new(4.0, 6.0, -10.0),
        Vec3::new(-4.0, 6.0, -4.0),
        Vec3::new(4.0, 6.0, -4.0),
    ];
    for (index, mut transform, mut movement) in player_q.iter_mut() {
        let slot = usize::from(index.0.min(3));
        transform.translation = deck_center + OFFSETS[slot];
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.is_grounded = false;
    }
}

fn airship_palette(faction: Faction) -> (Color, Color, Color) {
    match faction {
        Faction::DragonRoyalty => (
            Color::srgb(0.25, 0.03, 0.03),
            Color::srgb(0.95, 0.55, 0.12),
            Color::srgb(1.0, 0.22, 0.08),
        ),
        Faction::DragonExile => (
            Color::srgb(0.08, 0.10, 0.14),
            Color::srgb(0.55, 0.80, 1.0),
            Color::srgb(0.20, 0.65, 1.0),
        ),
        _ => (
            Color::srgb(0.16, 0.12, 0.28),
            Color::srgb(0.80, 0.65, 1.0),
            Color::srgb(0.60, 0.30, 1.0),
        ),
    }
}

fn airship_mat(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
    glow: f32,
) -> Handle<StandardMaterial> {
    let c = color.to_srgba();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(c.red * glow, c.green * glow, c.blue * glow, 1.0),
        metallic: 0.35,
        perceptual_roughness: 0.42,
        ..default()
    })
}

fn spawn_airship_escape_prop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    faction: Faction,
) {
    let (hull, trim, glow) = airship_palette(faction);
    let hull_mat = airship_mat(materials, hull, 0.7);
    let trim_mat = airship_mat(materials, trim, 1.2);
    let glow_mat = airship_mat(materials, glow, 4.0);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(10.0, 2.0, 24.0))),
            material: MeshMaterial3d(hull_mat.clone()),
            transform: Transform::from_translation(position),
            ..default()
        },
        AirshipLevelPiece,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(1.0))),
            material: MeshMaterial3d(trim_mat.clone()),
            transform: Transform {
                translation: position + Vec3::new(0.0, 3.2, 0.0),
                scale: Vec3::new(7.0, 1.8, 12.0),
                ..default()
            },
            ..default()
        },
        AirshipLevelPiece,
    ));
    for x in [-6.0, 6.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(1.0, 3.0))),
                material: MeshMaterial3d(glow_mat.clone()),
                transform: Transform::from_translation(position + Vec3::new(x, -0.2, -10.0))
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            AirshipLevelPiece,
        ));
    }
}

fn spawn_airship_arena(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    center: Vec3,
    faction: Faction,
) {
    let (hull, trim, glow) = airship_palette(faction);
    let deck_mat = airship_mat(materials, hull, 0.45);
    let trim_mat = airship_mat(materials, trim, 1.0);
    let glow_mat = airship_mat(materials, glow, 3.8);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(86.0, 4.0, 48.0))),
            material: MeshMaterial3d(deck_mat.clone()),
            transform: Transform::from_translation(center),
            ..default()
        },
        Collider::cuboid(43.0, 2.0, 24.0),
        WorldGeometry,
        WalkableSurface,
        AirshipLevelPiece,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(1.0))),
            material: MeshMaterial3d(trim_mat.clone()),
            transform: Transform {
                translation: center + Vec3::new(0.0, 9.0, 0.0),
                scale: Vec3::new(30.0, 5.5, 17.0),
                ..default()
            },
            ..default()
        },
        AirshipLevelPiece,
    ));

    for z in [-25.5, 25.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(88.0, 2.0, 1.3))),
                material: MeshMaterial3d(trim_mat.clone()),
                transform: Transform::from_translation(center + Vec3::new(0.0, 3.0, z)),
                ..default()
            },
            Collider::cuboid(44.0, 1.0, 0.65),
            WorldGeometry,
            AirshipLevelPiece,
        ));
    }
    for x in [-44.5, 44.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.3, 2.0, 48.0))),
                material: MeshMaterial3d(trim_mat.clone()),
                transform: Transform::from_translation(center + Vec3::new(x, 3.0, 0.0)),
                ..default()
            },
            Collider::cuboid(0.65, 1.0, 24.0),
            WorldGeometry,
            AirshipLevelPiece,
        ));
    }
    for x in [-32.0, 32.0] {
        for z in [-18.0, 18.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(1.8, 4.0))),
                    material: MeshMaterial3d(glow_mat.clone()),
                    transform: Transform::from_translation(center + Vec3::new(x, -2.6, z))
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    ..default()
                },
                AirshipLevelPiece,
            ));
        }
    }

    spawn_airship_hazards(commands, meshes, center, trim_mat, glow_mat);
}

fn spawn_airship_hazards(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    center: Vec3,
    trim_mat: Handle<StandardMaterial>,
    glow_mat: Handle<StandardMaterial>,
) {
    let turret_offsets = [
        Vec3::new(-34.0, 5.0, -16.0),
        Vec3::new(34.0, 5.0, -16.0),
        Vec3::new(-34.0, 5.0, 16.0),
        Vec3::new(34.0, 5.0, 16.0),
    ];
    for (i, offset) in turret_offsets.into_iter().enumerate() {
        let turret = commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(0.9, 1.35))),
                    material: MeshMaterial3d(trim_mat.clone()),
                    transform: Transform::from_translation(center + offset),
                    ..default()
                },
                Collider::cylinder(0.68, 0.9),
                RigidBody::Fixed,
                WorldGeometry,
                AirshipLevelPiece,
                LaserTurret {
                    range: 58.0,
                    cooldown: 2.25 + i as f32 * 0.12,
                    cooldown_timer: i as f32 * 0.35,
                    windup: 0.48,
                    windup_timer: 0.0,
                    locked_target: None,
                    damage: 9.0 + i as f32,
                    beam_material: glow_mat.clone(),
                },
            ))
            .id();

        commands.entity(turret).with_children(|parent| {
            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.32, 0.28, 2.3))),
                material: MeshMaterial3d(glow_mat.clone()),
                transform: Transform::from_xyz(0.0, 0.34, -1.05),
                ..default()
            });
            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.24))),
                material: MeshMaterial3d(glow_mat.clone()),
                transform: Transform::from_xyz(0.0, 0.66, -0.86),
                ..default()
            });
        });
    }

    let platform_runs = [
        (
            center + Vec3::new(-18.0, 5.8, 0.0),
            center + Vec3::new(18.0, 5.8, 0.0),
            3.2,
            0.0,
            Vec3::new(9.0, 1.0, 5.0),
        ),
        (
            center + Vec3::new(0.0, 7.0, -14.0),
            center + Vec3::new(0.0, 7.0, 14.0),
            2.7,
            1.4,
            Vec3::new(7.5, 1.0, 4.5),
        ),
    ];
    for (start, end, speed, phase, size) in platform_runs {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(trim_mat.clone()),
                transform: Transform::from_translation(start),
                ..default()
            },
            Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
            RigidBody::KinematicPositionBased,
            WorldGeometry,
            WalkableSurface,
            AirshipLevelPiece,
            MovingPlatform {
                start,
                end,
                speed,
                phase,
                size,
            },
        ));
    }
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
    let beacon_y = position.y + 1.0;
    let color = match &kind {
        DiscoverableKind::Blueprint(_) => Color::srgb(0.2, 0.7, 1.0),
        DiscoverableKind::WeaponMod(_) => Color::srgb(1.0, 0.5, 0.0),
        DiscoverableKind::ArmorMod(_) => Color::srgb(0.3, 1.0, 0.5),
        DiscoverableKind::CompanionRecruit(_) => Color::srgb(1.0, 0.85, 0.3),
        DiscoverableKind::RobotPetRescue { .. } => Color::srgb(0.35, 1.0, 0.95),
        DiscoverableKind::BeamSabreUnlock => Color::srgb(0.8, 0.1, 1.0),
        DiscoverableKind::ScientistRelic { .. } => Color::srgb(1.0, 0.95, 0.45),
        DiscoverableKind::RelicFragment { .. } => Color::srgb(0.45, 1.0, 0.95),
        DiscoverableKind::SecretCave { .. } => Color::srgb(0.35, 0.95, 0.65),
        DiscoverableKind::HiddenReward { .. } => Color::srgb(1.0, 0.70, 0.25),
        DiscoverableKind::SpyData { .. } => Color::srgb(0.25, 1.0, 0.78),
        DiscoverableKind::TechCache { .. } => Color::srgb(0.9, 0.95, 1.0),
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
            transform: Transform::from_translation(Vec3::new(position.x, beacon_y, position.z)),
            ..default()
        },
        Discoverable::new(kind, label, beacon_y),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_relic_fragment_puzzle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    progress: &ChapterProgress,
    scientist: &'static str,
    relic_id: &'static str,
    label: &'static str,
    total: u8,
    center: Vec3,
) {
    let pedestal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.24, 0.30),
        emissive: LinearRgba::new(0.1, 0.6, 0.7, 1.0),
        metallic: 0.35,
        perceptual_roughness: 0.30,
        ..default()
    });
    let obstacle_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.16, 0.12),
        emissive: LinearRgba::new(3.2, 0.4, 0.2, 1.0),
        metallic: 0.50,
        perceptual_roughness: 0.22,
        ..default()
    });
    let platform_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.38, 0.72),
        emissive: LinearRgba::new(0.2, 0.8, 2.5, 1.0),
        metallic: 0.45,
        perceptual_roughness: 0.28,
        ..default()
    });

    let total = total.max(1);
    for piece in 1..=total {
        if progress.has_relic_fragment(scientist, relic_id, piece) {
            continue;
        }
        let t = (piece - 1) as f32 / total as f32 * std::f32::consts::TAU;
        let radius = 12.0 + (piece % 2) as f32 * 5.0;
        let fragment_pos = center + Vec3::new(t.cos() * radius, 1.4, t.sin() * radius);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(3.2, 0.45, 3.2))),
                material: MeshMaterial3d(pedestal_mat.clone()),
                transform: Transform::from_translation(fragment_pos + Vec3::Y * -0.85),
                ..default()
            },
            Collider::cuboid(1.6, 0.22, 1.6),
            WorldGeometry,
            WalkableSurface,
            RelicFragmentPuzzlePiece {
                scientist,
                relic_id,
            },
        ));
        spawn_discoverable_beacon(
            commands,
            meshes,
            materials,
            DiscoverableKind::RelicFragment {
                scientist,
                relic_id,
                piece,
                total,
            },
            label,
            fragment_pos,
        );
    }

    for i in 0..5 {
        let lane = i as f32 - 2.0;
        let base = center + Vec3::new(lane * 6.5, 1.0, 0.0);
        let travel = if i % 2 == 0 {
            Vec3::new(0.0, 0.0, 13.0)
        } else {
            Vec3::new(9.0, 0.0, 0.0)
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.8, 1.1, 1.1))),
                material: MeshMaterial3d(obstacle_mat.clone()),
                transform: Transform::from_translation(base),
                ..default()
            },
            Collider::cuboid(3.4, 0.55, 0.55),
            RigidBody::KinematicPositionBased,
            RelicFragmentObstacle {
                base,
                travel,
                speed: 0.85 + i as f32 * 0.18,
                phase: i as f32 * 0.7,
                spin_speed: 0.9 + i as f32 * 0.15,
            },
            RelicFragmentPuzzlePiece {
                scientist,
                relic_id,
            },
        ));
    }

    for i in 0..3 {
        let base = center + Vec3::new(-13.0 + i as f32 * 13.0, 1.0, -8.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(5.2, 0.55, 5.2))),
                material: MeshMaterial3d(platform_mat.clone()),
                transform: Transform::from_translation(base),
                ..default()
            },
            Collider::cuboid(2.6, 0.28, 2.6),
            RigidBody::KinematicPositionBased,
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start: base + Vec3::new(0.0, -1.4, 0.0),
                end: base + Vec3::new(0.0, 2.6, 0.0),
                speed: 3.0 + i as f32 * 0.55,
                phase: i as f32 * 1.1,
                size: Vec3::new(5.2, 0.55, 5.2),
            },
            RelicFragmentPuzzlePiece {
                scientist,
                relic_id,
            },
        ));
    }
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
    mut killed_ev: MessageReader<EnemyKilledEvent>,
    mut current: ResMut<CurrentChapter>,
    mut boss_def_ev: MessageWriter<BossDefeatedEvent>,
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
                boss_def_ev.write(BossDefeatedEvent {
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
    mut completed_ev: MessageReader<ChapterCompletedEvent>,
    mut progress: ResMut<ChapterProgress>,
) {
    for ev in completed_ev.read() {
        progress.mark_completed(ChapterId(ev.chapter));
    }
}

/// Public helper — list completed chapter ids (for save/load).
#[allow(dead_code)]
pub fn completed_chapters(progress: &ChapterProgress) -> Vec<u8> {
    progress.completed.clone()
}
