//! Combat/reward SFX bus (Sprint 3): the read-side that finally makes the
//! game audible. Bakes the procedural retro palette (`audio_synth.rs`) into
//! `Assets<AudioSource>` at startup, then maps the gameplay events — most of
//! which previously had no reader at all — to one-shot players.
//!
//! Design rules:
//! * Per-kind **cooldown** (50 ms) so 4-player firefights don't clip into mud.
//! * Deterministic **pitch jitter** (±5%, LCG — no `thread_rng`) so repeated
//!   shots don't machine-gun a single identical sample.
//! * Master volume follows `GameSettings.sfx_volume`.
//! * File-based sounds can replace any preset later by swapping the handle —
//!   the event wiring doesn't change.

use bevy::audio::{AudioPlayer, PlaybackSettings, Volume};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::audio::player::AudioLibraryReloadEvent;
use crate::audio::synth::{self, render_wav};
use crate::engine::state::AppState;
use crate::events::{
    ChestOpenedEvent, ComboHitEvent, EnemyDamagedEvent, EnemyKilledEvent, LootCollectedEvent,
    PlayerDamagedEvent, PlayerLevelUpEvent, PlayerParryEvent, WeaponFiredEvent,
    WeaponReloadedEvent,
};
use crate::resources::GameSettings;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SfxKind {
    Shoot,
    Slash,
    Hit,
    Parry,
    Kill,
    Hurt,
    Loot,
    Chest,
    LevelUp,
    Reload,
}

pub const DEFAULT_USER_SFX_DIR: &str = "assets/user_sfx";

/// Generic seam for the modular action system and future Forge-authored
/// actions. Assigned MP3 clips play here without changing the retro bus.
#[derive(Message, Debug, Clone)]
pub struct ModularActionSfxEvent {
    pub action_id: String,
}

impl ModularActionSfxEvent {
    pub fn new(action_id: impl Into<String>) -> Self {
        Self {
            action_id: action_id.into(),
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct ActionSfxRegistry {
    handles: HashMap<String, Handle<AudioSource>>,
    source_paths: HashMap<String, PathBuf>,
    cooldowns: HashMap<String, f32>,
}

impl ActionSfxRegistry {
    pub fn assigned_count(&self) -> usize {
        self.handles.len()
    }

    #[allow(dead_code)]
    pub fn is_assigned(&self, action_id: &str) -> bool {
        self.handles.contains_key(action_id)
    }
}

#[derive(Deserialize, Default)]
struct ActionSfxManifest {
    #[serde(default)]
    actions: HashMap<String, String>,
}

/// Baked handles + per-kind cooldowns + the jitter LCG state.
#[derive(Resource)]
pub struct SfxLibrary {
    handles: HashMap<SfxKind, Handle<AudioSource>>,
    cooldowns: HashMap<SfxKind, f32>,
    /// Per-kind base volume (pre master-volume) for a rough mix.
    mix: HashMap<SfxKind, f32>,
    jitter_state: u32,
}

const SFX_COOLDOWN: f32 = 0.05;

impl SfxLibrary {
    fn jitter(&mut self) -> f32 {
        self.jitter_state = self
            .jitter_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let unit = ((self.jitter_state >> 16) & 0xFF) as f32 / 255.0;
        0.95 + unit * 0.10
    }
}

fn bake_sfx_library(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut handles = HashMap::new();
    let mut mix = HashMap::new();
    let mut bake = |kind: SfxKind, params: synth::SfxParams, level: f32| {
        let source = AudioSource {
            bytes: render_wav(&params).into(),
        };
        handles.insert(kind, sources.add(source));
        mix.insert(kind, level);
    };
    bake(SfxKind::Shoot, synth::preset_shoot(), 0.34);
    bake(SfxKind::Slash, synth::preset_slash(), 0.5);
    bake(SfxKind::Hit, synth::preset_hit(), 0.5);
    bake(SfxKind::Parry, synth::preset_parry(), 0.65);
    bake(SfxKind::Kill, synth::preset_kill(), 0.6);
    bake(SfxKind::Hurt, synth::preset_hurt(), 0.6);
    bake(SfxKind::Loot, synth::preset_loot(), 0.45);
    bake(SfxKind::Chest, synth::preset_chest(), 0.55);
    bake(SfxKind::LevelUp, synth::preset_level_up(), 0.6);
    bake(SfxKind::Reload, synth::preset_reload(), 0.35);

    commands.insert_resource(SfxLibrary {
        handles,
        cooldowns: HashMap::new(),
        mix,
        jitter_state: 0x5F3759DF,
    });
    commands.insert_resource(load_action_sfx_registry(&mut sources));
}

pub fn action_sfx_directory() -> PathBuf {
    std::env::var_os("STARFALL_SFX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_USER_SFX_DIR))
}

fn valid_action_id(action_id: &str) -> bool {
    !action_id.is_empty()
        && action_id.len() <= 96
        && action_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn safe_mp3_path(directory: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || !relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
    {
        return None;
    }
    Some(directory.join(relative))
}

fn load_action_sfx_registry(sources: &mut Assets<AudioSource>) -> ActionSfxRegistry {
    let directory = action_sfx_directory();
    let _ = std::fs::create_dir_all(&directory);
    let manifest = std::fs::read(directory.join("actions.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ActionSfxManifest>(&bytes).ok())
        .unwrap_or_default();
    let mut registry = ActionSfxRegistry::default();
    let mut assignments = manifest.actions.into_iter().collect::<Vec<_>>();
    assignments.sort_by(|a, b| a.0.cmp(&b.0));
    for (action_id, relative_path) in assignments {
        if !valid_action_id(&action_id) {
            continue;
        }
        let Some(source_path) = safe_mp3_path(&directory, &relative_path) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&source_path) else {
            continue;
        };
        if !crate::audio::player::valid_music_bytes(&bytes) {
            warn!(
                "Skipping action SFX {} for '{action_id}' — not a playable MP3",
                source_path.display()
            );
            continue;
        }
        registry.handles.insert(
            action_id.clone(),
            sources.add(AudioSource {
                bytes: bytes.into(),
            }),
        );
        registry.source_paths.insert(action_id, source_path);
    }
    registry
}

fn tick_sfx_cooldowns(time: Res<Time>, library: Option<ResMut<SfxLibrary>>) {
    let Some(mut library) = library else { return };
    let dt = time.delta_secs();
    library.cooldowns.retain(|_, remaining| {
        *remaining -= dt;
        *remaining > 0.0
    });
}

fn tick_action_sfx_cooldowns(time: Res<Time>, registry: Option<ResMut<ActionSfxRegistry>>) {
    let Some(mut registry) = registry else {
        return;
    };
    let dt = time.delta_secs();
    registry.cooldowns.retain(|_, remaining| {
        *remaining -= dt;
        *remaining > 0.0
    });
}

fn play(
    commands: &mut Commands,
    library: &mut SfxLibrary,
    action_registry: &mut ActionSfxRegistry,
    settings: &GameSettings,
    action_id: &str,
    kind: SfxKind,
) {
    if settings.sfx_volume <= 0.01 || settings.master_volume <= 0.01 {
        return;
    }
    if library.cooldowns.contains_key(&kind) {
        return;
    }
    let custom = action_registry.handles.get(action_id).cloned();
    let is_custom = custom.is_some();
    let Some(handle) = custom.or_else(|| library.handles.get(&kind).cloned()) else {
        return;
    };
    library.cooldowns.insert(kind, SFX_COOLDOWN);
    let level = library.mix.get(&kind).copied().unwrap_or(0.5)
        * settings.master_volume
        * settings.sfx_volume;
    let speed = if is_custom { 1.0 } else { library.jitter() };
    commands.spawn((
        AudioPlayer::new(handle),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(level))
            .with_speed(speed),
    ));
}

fn play_modular_action(
    commands: &mut Commands,
    registry: &mut ActionSfxRegistry,
    settings: &GameSettings,
    action_id: &str,
) {
    if settings.sfx_volume <= 0.01 || registry.cooldowns.contains_key(action_id) {
        return;
    }
    let Some(handle) = registry.handles.get(action_id).cloned() else {
        return;
    };
    registry
        .cooldowns
        .insert(action_id.to_string(), SFX_COOLDOWN);
    commands.spawn((
        AudioPlayer::new(handle),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(
            (settings.master_volume * settings.sfx_volume * 0.55).clamp(0.0, 1.0),
        )),
    ));
}

/// One reader over every hooked event. Cheap: readers that saw nothing cost a
/// cursor check.
#[allow(clippy::too_many_arguments)]
fn combat_sfx_system(
    mut commands: Commands,
    settings: Res<GameSettings>,
    library: Option<ResMut<SfxLibrary>>,
    action_registry: Option<ResMut<ActionSfxRegistry>>,
    mut fired: MessageReader<WeaponFiredEvent>,
    mut combo: MessageReader<ComboHitEvent>,
    mut enemy_damaged: MessageReader<EnemyDamagedEvent>,
    mut parry: MessageReader<PlayerParryEvent>,
    mut killed: MessageReader<EnemyKilledEvent>,
    mut player_damaged: MessageReader<PlayerDamagedEvent>,
    mut loot: MessageReader<LootCollectedEvent>,
    mut chest: MessageReader<ChestOpenedEvent>,
    mut level_up: MessageReader<PlayerLevelUpEvent>,
    mut reload: MessageReader<WeaponReloadedEvent>,
) {
    let Some(mut library) = library else { return };
    let Some(mut action_registry) = action_registry else {
        return;
    };
    let lib = library.as_mut();
    let actions = action_registry.as_mut();

    if fired.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "weapon.fire",
            SfxKind::Shoot,
        );
    }
    if combo.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "melee.slash",
            SfxKind::Slash,
        );
    }
    if enemy_damaged.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "combat.hit",
            SfxKind::Hit,
        );
    }
    if parry.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "combat.parry",
            SfxKind::Parry,
        );
    }
    if killed.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "combat.kill",
            SfxKind::Kill,
        );
    }
    if player_damaged.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "player.hurt",
            SfxKind::Hurt,
        );
    }
    if loot.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "reward.loot",
            SfxKind::Loot,
        );
    }
    if chest.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "reward.chest",
            SfxKind::Chest,
        );
    }
    if level_up.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "player.level_up",
            SfxKind::LevelUp,
        );
    }
    if reload.read().next().is_some() {
        play(
            &mut commands,
            lib,
            actions,
            &settings,
            "weapon.reload",
            SfxKind::Reload,
        );
    }
}

fn modular_action_sfx_system(
    mut commands: Commands,
    settings: Res<GameSettings>,
    library: Option<ResMut<SfxLibrary>>,
    registry: Option<ResMut<ActionSfxRegistry>>,
    mut actions: MessageReader<ModularActionSfxEvent>,
) {
    let Some(mut library) = library else {
        return;
    };
    let Some(mut registry) = registry else {
        return;
    };
    for action in actions.read() {
        if valid_action_id(&action.action_id) {
            if registry.is_assigned(&action.action_id) {
                play_modular_action(&mut commands, &mut registry, &settings, &action.action_id);
            } else {
                play(
                    &mut commands,
                    &mut library,
                    &mut registry,
                    &settings,
                    &action.action_id,
                    modular_fallback_kind(&action.action_id),
                );
            }
        }
    }
}

fn modular_fallback_kind(action_id: &str) -> SfxKind {
    if action_id.starts_with("sabre.") || action_id.contains("slash") {
        SfxKind::Slash
    } else if action_id.starts_with("water.")
        || action_id.starts_with("waterfall.")
        || action_id.contains("land")
        || action_id.contains("impact")
    {
        SfxKind::Hit
    } else if action_id.contains("loot") || action_id.contains("reward") {
        SfxKind::Loot
    } else {
        SfxKind::Shoot
    }
}

fn reload_action_sfx_system(
    mut reload_ev: MessageReader<AudioLibraryReloadEvent>,
    mut sources: ResMut<Assets<AudioSource>>,
    registry: Option<ResMut<ActionSfxRegistry>>,
) {
    if reload_ev.read().next().is_none() {
        return;
    }
    let replacement = load_action_sfx_registry(&mut sources);
    if let Some(mut registry) = registry {
        *registry = replacement;
    }
}

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ModularActionSfxEvent>()
            .add_message::<AudioLibraryReloadEvent>()
            .add_systems(Startup, bake_sfx_library)
            .add_systems(Update, reload_action_sfx_system)
            .add_systems(
                Update,
                (
                    tick_sfx_cooldowns,
                    tick_action_sfx_cooldowns,
                    combat_sfx_system,
                    modular_action_sfx_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_are_bounded_and_path_safe() {
        assert!(valid_action_id("weapon.fire"));
        assert!(valid_action_id("hoverboard_grind-01"));
        assert!(!valid_action_id("../weapon.fire"));
        assert!(!valid_action_id("weapon/fire"));
        assert!(!valid_action_id(""));
    }

    #[test]
    fn custom_sfx_paths_reject_escape_and_non_mp3_files() {
        let root = Path::new("/tmp/starfall_sfx");
        assert_eq!(
            safe_mp3_path(root, "laser.mp3"),
            Some(root.join("laser.mp3"))
        );
        assert!(safe_mp3_path(root, "../laser.mp3").is_none());
        assert!(safe_mp3_path(root, "/outside/laser.mp3").is_none());
        assert!(safe_mp3_path(root, "laser.wav").is_none());
    }

    #[test]
    fn empty_registry_keeps_arcade_fallback_available() {
        let registry = ActionSfxRegistry::default();
        assert!(!registry.is_assigned("weapon.fire"));
        assert_eq!(registry.assigned_count(), 0);
        assert_eq!(synth::render_wav(&synth::preset_shoot())[0..4], *b"RIFF");
    }

    #[test]
    fn modular_actions_select_readable_procedural_fallbacks() {
        assert_eq!(modular_fallback_kind("sabre.cyclone"), SfxKind::Slash);
        assert_eq!(
            modular_fallback_kind("hoverboard.overdrive"),
            SfxKind::Shoot
        );
        assert_eq!(modular_fallback_kind("waterfall.splash"), SfxKind::Hit);
        assert_eq!(modular_fallback_kind("player.land_hard"), SfxKind::Hit);
        assert_eq!(modular_fallback_kind("world.reward"), SfxKind::Loot);
    }
}
