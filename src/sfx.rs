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
use std::collections::HashMap;

use crate::audio_synth::{self, render_wav};
use crate::events::{
    ChestOpenedEvent, ComboHitEvent, EnemyDamagedEvent, EnemyKilledEvent, LootCollectedEvent,
    PlayerDamagedEvent, PlayerLevelUpEvent, PlayerParryEvent, WeaponFiredEvent,
    WeaponReloadedEvent,
};
use crate::resources::GameSettings;
use crate::state::AppState;

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
    let mut bake = |kind: SfxKind, params: audio_synth::SfxParams, level: f32| {
        let source = AudioSource {
            bytes: render_wav(&params).into(),
        };
        handles.insert(kind, sources.add(source));
        mix.insert(kind, level);
    };
    bake(SfxKind::Shoot, audio_synth::preset_shoot(), 0.34);
    bake(SfxKind::Slash, audio_synth::preset_slash(), 0.5);
    bake(SfxKind::Hit, audio_synth::preset_hit(), 0.5);
    bake(SfxKind::Parry, audio_synth::preset_parry(), 0.65);
    bake(SfxKind::Kill, audio_synth::preset_kill(), 0.6);
    bake(SfxKind::Hurt, audio_synth::preset_hurt(), 0.6);
    bake(SfxKind::Loot, audio_synth::preset_loot(), 0.45);
    bake(SfxKind::Chest, audio_synth::preset_chest(), 0.55);
    bake(SfxKind::LevelUp, audio_synth::preset_level_up(), 0.6);
    bake(SfxKind::Reload, audio_synth::preset_reload(), 0.35);

    commands.insert_resource(SfxLibrary {
        handles,
        cooldowns: HashMap::new(),
        mix,
        jitter_state: 0x5F3759DF,
    });
}

fn tick_sfx_cooldowns(time: Res<Time>, library: Option<ResMut<SfxLibrary>>) {
    let Some(mut library) = library else { return };
    let dt = time.delta_secs();
    library.cooldowns.retain(|_, remaining| {
        *remaining -= dt;
        *remaining > 0.0
    });
}

fn play(
    commands: &mut Commands,
    library: &mut SfxLibrary,
    settings: &GameSettings,
    kind: SfxKind,
) {
    if settings.sfx_volume <= 0.01 {
        return;
    }
    if library.cooldowns.contains_key(&kind) {
        return;
    }
    let Some(handle) = library.handles.get(&kind).cloned() else {
        return;
    };
    library.cooldowns.insert(kind, SFX_COOLDOWN);
    let level = library.mix.get(&kind).copied().unwrap_or(0.5) * settings.sfx_volume;
    let speed = library.jitter();
    commands.spawn((
        AudioPlayer::new(handle),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(level))
            .with_speed(speed),
    ));
}

/// One reader over every hooked event. Cheap: readers that saw nothing cost a
/// cursor check.
#[allow(clippy::too_many_arguments)]
fn combat_sfx_system(
    mut commands: Commands,
    settings: Res<GameSettings>,
    library: Option<ResMut<SfxLibrary>>,
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
    let lib = library.as_mut();

    if fired.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Shoot);
    }
    if combo.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Slash);
    }
    if enemy_damaged.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Hit);
    }
    if parry.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Parry);
    }
    if killed.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Kill);
    }
    if player_damaged.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Hurt);
    }
    if loot.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Loot);
    }
    if chest.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Chest);
    }
    if level_up.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::LevelUp);
    }
    if reload.read().next().is_some() {
        play(&mut commands, lib, &settings, SfxKind::Reload);
    }
}

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, bake_sfx_library).add_systems(
            Update,
            (tick_sfx_cooldowns, combat_sfx_system)
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}
