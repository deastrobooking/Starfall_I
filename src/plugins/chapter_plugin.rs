//! Chapter director - the heart of the Starfall I gameplay loop.
//!
//! Replaces the old wave timer/boss-spawn logic. On `OnEnter(Playing)` the
//! director starts the chapter selected via `CurrentChapter`. Each frame, the
//! `chapter_director_system` advances through the script.

use bevy::prelude::*;

use crate::chapters::{get_chapter, ChapterId, EncounterStep};
use crate::components::discoverable::{Discoverable, DiscoverableKind};
use crate::components::enemy::BossEnemy;
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::player::Player;
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
    mut wave: ResMut<WaveInfo>,
    mut radio_ev: EventWriter<RadioChatterEvent>,
    mut step_ev: EventWriter<EncounterStepAdvancedEvent>,
    mut completed_ev: EventWriter<ChapterCompletedEvent>,
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
    if current.awaiting_kills > 0 {
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

// ── Discoverable beacon spawn ─────────────────────────────────────────────────
fn spawn_discoverable_beacon(
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
    mut next_state: ResMut<NextState<AppState>>,
) {
    for ev in completed_ev.read() {
        progress.mark_completed(ChapterId(ev.chapter));
        // Return to chapter select after a short delay (handled by UI).
        let _ = next_state; // future hook
    }
}

/// Public helper — list completed chapter ids (for save/load).
pub fn completed_chapters(progress: &ChapterProgress) -> Vec<u8> {
    progress.completed.clone()
}
