//! Heavy Water's mode adapters. Requests validate and capture the party first,
//! then cross a dedicated teardown state before the next scene starts.

use bevy::prelude::*;

use crate::engine::state::AppState;
use crate::events::UiMessageEvent;
use crate::graph::StableId;
use crate::play_modes::{CameraStyle, ModeRegistry, PlayMode, PlayStyle};
use crate::plugins::save_plugin::{capture_mode_session, ModeSessionCarryover, SaveParams};
use crate::resources::{
    DungeonCrawlState, LocalPlayerConfig, PlayExperience, PlaySessionTransition,
};

pub const OPEN_WORLD_MODE: &str = "heavy_water.open_world";
pub const PLATFORMER_MODE: &str = "heavy_water.platformer";

#[derive(Resource)]
pub struct HeavyWaterModes(pub ModeRegistry<PlayExperience>);

impl Default for HeavyWaterModes {
    fn default() -> Self {
        let mut registry = ModeRegistry::default();
        for (id, label, style, camera, adapter) in [
            (
                OPEN_WORLD_MODE,
                "3D OPEN WORLD",
                PlayStyle::OpenWorld,
                CameraStyle::PerPlayer3d,
                PlayExperience::Campaign,
            ),
            (
                PLATFORMER_MODE,
                "SHARED-SCREEN PLATFORMER",
                PlayStyle::Platformer,
                CameraStyle::SharedPlatformer3d,
                PlayExperience::SharedPlatformer,
            ),
        ] {
            registry
                .register(PlayMode {
                    id: StableId::new(id).expect("built-in mode ID"),
                    label: label.into(),
                    style,
                    camera,
                    min_players: 1,
                    max_players: 4,
                    adapter,
                })
                .expect("valid unique built-in mode");
        }
        Self(registry)
    }
}

impl PlayExperience {
    pub fn mode_id(self) -> &'static str {
        match self {
            Self::Campaign => OPEN_WORLD_MODE,
            Self::SharedPlatformer => PLATFORMER_MODE,
        }
    }
}

/// Send from a pause button, portal, or native gameplay system. Unknown modes,
/// incompatible parties, and competing screen transitions leave play intact.
#[derive(Message, Debug, Clone)]
pub struct SwitchPlayMode(pub String);

#[derive(Resource, Default)]
struct PendingMode(Option<PlayExperience>);

pub struct HeavyWaterModesPlugin;

impl Plugin for HeavyWaterModesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWaterModes>()
            .init_resource::<PendingMode>()
            .add_message::<SwitchPlayMode>()
            // After gameplay and menu actions so there is only one accepted
            // destination, and capture sees the completed frame's progress.
            .add_systems(Last, (accept_mode_switch, finish_mode_switch).chain());
    }
}

fn accept_mode_switch(
    mut requests: MessageReader<SwitchPlayMode>,
    modes: Res<HeavyWaterModes>,
    party: Res<LocalPlayerConfig>,
    experience: Res<PlayExperience>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut pending: ResMut<PendingMode>,
    mut carryover: ResMut<ModeSessionCarryover>,
    mut transition: ResMut<PlaySessionTransition>,
    snapshot: SaveParams,
    mut messages: MessageWriter<UiMessageEvent>,
) {
    for request in requests.read() {
        let error = if pending.0.is_some()
            || !matches!(next.as_ref(), NextState::Unchanged)
            || !matches!(state.get(), AppState::Playing | AppState::Paused)
        {
            Some("Finish the current screen transition before changing mode".into())
        } else {
            match modes.0.resolve(&request.0, party.active) {
                Err(error) => Some(error.to_string()),
                Ok(mode) if mode.adapter == *experience => None,
                Ok(mode) => match capture_mode_session(&snapshot) {
                    Err(error) => Some(error),
                    Ok(data) => {
                        carryover.0 = Some(data);
                        pending.0 = Some(mode.adapter);
                        transition.pausing = false;
                        transition.resuming_from_pause = false;
                        next.set(AppState::SwitchingMode);
                        None
                    }
                },
            }
        };
        if let Some(error) = error {
            messages.write(UiMessageEvent {
                text: error,
                duration: 3.0,
            });
        }
    }
}

fn finish_mode_switch(
    state: Res<State<AppState>>,
    mut pending: ResMut<PendingMode>,
    mut experience: ResMut<PlayExperience>,
    mut dungeon: ResMut<DungeonCrawlState>,
    mut next: ResMut<NextState<AppState>>,
) {
    if *state.get() != AppState::SwitchingMode {
        return;
    }
    // OnEnter(SwitchingMode) has finished all cleanup, including deferred
    // despawns. No outgoing scene can observe the new mode/camera policy.
    if let Some(destination) = pending.0.take() {
        dungeon.clear();
        *experience = destination;
        next.set(AppState::Playing);
    }
}
