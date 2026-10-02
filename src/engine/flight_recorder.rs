//! Bounded diagnostic history for Starfall player input and movement.
//!
//! Records fixed commands and observed player transforms, not an authoritative
//! world snapshot. Avian motion flush, combat and AI still run outside this
//! boundary; this is deliberately not a deterministic replay file.

use bevy::prelude::*;
use serde_json::json;
use std::io::{self, Write};

use crate::components::player::PlayerIndex;
use crate::engine::{
    game_loop::{FixedTickCount, SimConfig, FIXED_HZ},
    game_rng::GameRng,
    input_buffer::{FixedInput, PlayerInputBuffers},
    state::AppState,
};
use crate::runtime::history::TickHistory;

pub const HISTORY_TICKS: usize = 2048;
pub const HISTORY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct PlayerTick {
    pub command: FixedInput,
    pub position: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct RecordedTick {
    /// Indexed by PlayerIndex, never ECS iteration order.
    pub players: [Option<PlayerTick>; 4],
    pub fixed_motor: bool,
}

#[derive(Resource, Debug)]
pub struct FlightRecorder {
    history: TickHistory<RecordedTick>,
}

impl Default for FlightRecorder {
    fn default() -> Self {
        Self {
            history: TickHistory::new(HISTORY_TICKS, HISTORY_BYTES),
        }
    }
}

impl FlightRecorder {
    pub fn len(&self) -> usize {
        self.history.len()
    }
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
    pub fn history(&self) -> &TickHistory<RecordedTick> {
        &self.history
    }

    /// Newline-delimited JSON: one versioned header, then chronological ticks.
    /// Seed identifies the session; it is not a saved RNG state at clip start.
    pub fn write_json_lines(&self, mut writer: impl Write, seed: u64) -> io::Result<()> {
        serde_json::to_writer(
            &mut writer,
            &json!({
                "format": "starfall.input-diagnostics", "schema_version": 1,
                "engine_version": env!("CARGO_PKG_VERSION"), "fixed_hz": FIXED_HZ,
                "seed": seed, "ticks": self.len(), "playable_replay": false,
            }),
        )?;
        writeln!(writer)?;
        for entry in self.history.iter() {
            let players: Vec<_> = entry.value.players.iter().enumerate().filter_map(|(index, player)| {
                let player = player.as_ref()?;
                let held = player.command.held;
                let edges = player.command.edges;
                Some(json!({
                    "player": index, "position": player.position,
                    "held": {"move_axis": held.move_axis.to_array(), "fire": held.fire,
                        "aim": held.aim, "sprint": held.sprint, "jetpack": held.jetpack,
                        "grapple": held.grapple},
                    "edges": {"jump": edges.jump, "dodge": edges.dodge, "parry": edges.parry,
                        "grapple": edges.grapple, "fire": edges.fire, "melee_light": edges.melee_light,
                        "melee_heavy": edges.melee_heavy, "interact": edges.interact, "reload": edges.reload,
                        "weapon_next": edges.weapon_next, "weapon_prev": edges.weapon_prev,
                        "special_next": edges.special_next, "special_prev": edges.special_prev,
                        "sabre_toggle": edges.sabre_toggle, "sabre_attack": edges.sabre_attack,
                        "enter_vehicle": edges.enter_vehicle, "look_delta": edges.look_delta.to_array(),
                        "weapon_slot": edges.weapon_slot, "special_slot": edges.special_slot,
                        "traversal": edges.traversal.map(|mode| mode.label())},
                }))
            }).collect();
            serde_json::to_writer(
                &mut writer,
                &json!({
                    "tick": entry.tick, "fixed_motor": entry.value.fixed_motor, "players": players,
                }),
            )?;
            writeln!(writer)?;
        }
        Ok(())
    }
}

fn record_tick(
    players: Query<(&PlayerIndex, &Transform)>,
    buffers: Res<PlayerInputBuffers>,
    tick: Res<FixedTickCount>,
    config: Res<SimConfig>,
    mut recorder: ResMut<FlightRecorder>,
) {
    let mut frame = RecordedTick {
        players: [None; 4],
        fixed_motor: config.fixed_motor,
    };
    for (index, transform) in &players {
        if let (Some(slot), Some(command)) = (
            frame.players.get_mut(index.0 as usize),
            buffers.fixed(index.0),
        ) {
            *slot = Some(PlayerTick {
                command: *command,
                position: transform.translation.to_array(),
            });
        }
    }
    // Fixed-size payload: no unaccounted nested Vec or String allocations.
    let _ = recorder
        .history
        .push(tick.0, frame, std::mem::size_of::<RecordedTick>());
}

fn reset_history(mut recorder: ResMut<FlightRecorder>) {
    recorder.history.clear();
}

fn export_history(
    keyboard: Res<ButtonInput<KeyCode>>,
    recorder: Res<FlightRecorder>,
    rng: Res<GameRng>,
) {
    if !cfg!(feature = "designer") || !keyboard.just_pressed(KeyCode::F12) || recorder.is_empty() {
        return;
    }
    match export_clip(&recorder, rng.seed) {
        Ok(path) => info!("Input diagnostics exported to {}", path.display()),
        Err(error) => warn!("Could not export input diagnostics: {error}"),
    }
}

fn export_clip(recorder: &FlightRecorder, seed: u64) -> io::Result<std::path::PathBuf> {
    let root = crate::engine::platform_paths::data_root().join("input-diagnostics");
    std::fs::create_dir_all(&root)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stem = format!("input_{stamp:030}_{}", std::process::id());
    let temporary = root.join(format!("{stem}.tmp"));
    let path = root.join(format!("{stem}.jsonl"));
    let result = (|| {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut writer = io::BufWriter::new(file);
        recorder.write_json_lines(&mut writer, seed)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        std::fs::rename(&temporary, &path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    let mut clips: Vec<_> = std::fs::read_dir(&root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("input_") && n.ends_with(".jsonl"))
        })
        .collect();
    clips.sort();
    let remove = clips.len().saturating_sub(5);
    for old in clips.into_iter().take(remove) {
        std::fs::remove_file(old)?;
    }
    Ok(path)
}

pub struct FlightRecorderPlugin;
impl Plugin for FlightRecorderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlightRecorder>()
            .add_systems(OnEnter(AppState::ChapterSelect), reset_history)
            .add_systems(OnEnter(AppState::MainMenu), reset_history)
            .add_systems(OnEnter(AppState::PlayerSelect), reset_history)
            .add_systems(OnEnter(AppState::SwitchingMode), reset_history)
            .add_systems(
                FixedPostUpdate,
                record_tick.run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, export_history);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::player::PlayerInput;
    use crate::engine::input_buffer::PlayerBuffer;

    #[test]
    fn fixed_commands_precede_motor_and_recording_follows_it() {
        use crate::engine::input_buffer::InputBufferPlugin;
        use bevy::state::app::StatesPlugin;
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Playing)
            .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / FIXED_HZ),
            ))
            .insert_resource(GameRng::from_seed(42))
            .insert_resource(SimConfig { fixed_motor: true })
            .init_resource::<FixedTickCount>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((InputBufferPlugin, FlightRecorderPlugin))
            .add_systems(FixedFirst, |mut tick: ResMut<FixedTickCount>| {
                tick.0 += 1;
            })
            .add_systems(
                FixedUpdate,
                (|buffers: Res<PlayerInputBuffers>,
                  mut players: Query<(&PlayerIndex, &mut Transform)>| {
                    for (index, mut transform) in &mut players {
                        let command = buffers
                            .fixed(index.0)
                            .expect("commands exist before the motor");
                        transform.translation.x += command.held.move_axis.x;
                    }
                })
                .run_if(in_state(AppState::Playing)),
            );
        app.world_mut().spawn((
            PlayerIndex(3),
            Transform::default(),
            PlayerInput {
                move_axis: Vec2::X,
                ..default()
            },
        ));
        app.update();
        app.update();
        app.update();
        let recorder = app.world().resource::<FlightRecorder>();
        assert!(!recorder.is_empty());
        for entry in recorder.history().iter() {
            assert_eq!(
                entry.value.players[3].unwrap().position[0],
                entry.tick as f32
            );
            assert!(entry.value.players[0].is_none());
        }
        let captured = recorder.len();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Paused);
        app.update();
        assert_eq!(app.world().resource::<FlightRecorder>().len(), captured);
        assert!(app
            .world()
            .resource::<PlayerInputBuffers>()
            .fixed(3)
            .is_none());
    }

    #[test]
    fn export_preserves_slot_order_edges_and_schema() {
        let mut buffer = PlayerBuffer::default();
        buffer.latch(&PlayerInput {
            jump: true,
            ..default()
        });
        let mut recorder = FlightRecorder::default();
        for tick in 10..12 {
            let mut players = [None; 4];
            players[3] = Some(PlayerTick {
                command: buffer.consume(),
                position: [1., 2., 3.],
            });
            recorder
                .history
                .push(
                    tick,
                    RecordedTick {
                        players,
                        fixed_motor: true,
                    },
                    std::mem::size_of::<RecordedTick>(),
                )
                .unwrap();
        }
        let mut bytes = Vec::new();
        recorder.write_json_lines(&mut bytes, 42).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["schema_version"], 1);
        assert_eq!(rows[0]["playable_replay"], false);
        assert_eq!(rows[1]["tick"], 10);
        assert_eq!(rows[1]["players"][0]["player"], 3);
        assert_eq!(rows[1]["players"][0]["edges"]["jump"], true);
        assert_eq!(rows[2]["players"][0]["edges"]["jump"], false);
    }
}
