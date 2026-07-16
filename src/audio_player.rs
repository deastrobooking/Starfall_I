//! Player-facing background music deck. Music is deliberately independent of
//! the arcade/action SFX bus: both can play concurrently and obey separate
//! `GameSettings` volume controls.

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume,
};
use bevy::prelude::*;
use std::path::{Path, PathBuf};

use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::resources::{GameSettings, UiGameplayCapture};
use crate::sfx::{action_sfx_directory, ActionSfxRegistry};

pub const DEFAULT_USER_MUSIC_DIR: &str = "assets/user_music";

#[derive(Message, Debug, Clone, Copy)]
pub struct AudioLibraryReloadEvent;

#[derive(Debug, Clone)]
pub struct MusicTrack {
    pub name: String,
    #[allow(dead_code)]
    pub source_path: PathBuf,
    pub handle: Handle<AudioSource>,
}

#[derive(Resource, Debug, Default)]
pub struct MusicDeck {
    pub tracks: Vec<MusicTrack>,
    pub current_index: usize,
    pub playback_entity: Option<Entity>,
    pub paused: bool,
    pub shuffle: bool,
    pub overlay_visible: bool,
    pub generation: u32,
}

impl MusicDeck {
    fn current(&self) -> Option<&MusicTrack> {
        self.tracks.get(self.current_index)
    }

    fn advance(&mut self, direction: i32) {
        if self.tracks.is_empty() {
            self.current_index = 0;
            return;
        }
        if self.shuffle && direction > 0 {
            // Deterministic shuffle avoids save/replay nondeterminism while
            // still preventing a short playlist from repeating in order.
            let stride = (self.generation as usize * 2 + 3).max(3);
            self.current_index = (self.current_index + stride) % self.tracks.len();
            if self.tracks.len() > 1 && self.current_index == 0 {
                self.current_index = 1;
            }
        } else if direction >= 0 {
            self.current_index = (self.current_index + 1) % self.tracks.len();
        } else {
            self.current_index = self
                .current_index
                .checked_sub(1)
                .unwrap_or(self.tracks.len() - 1);
        }
        self.generation = self.generation.wrapping_add(1);
    }
}

#[derive(Component)]
struct MusicPlayback;

#[derive(Component)]
struct MusicPlayerOverlay;

#[derive(Component)]
struct MusicPlayerText;

fn configured_music_dir() -> PathBuf {
    std::env::var_os("STARFALL_MUSIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_USER_MUSIC_DIR))
}

fn is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
}

pub(crate) fn looks_like_mp3(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3")
        || bytes
            .windows(2)
            .take(4096)
            .any(|frame| frame[0] == 0xFF && frame[1] & 0xE0 == 0xE0 && frame[1] & 0x06 != 0)
}

fn scan_music_tracks(directory: &Path, sources: &mut Assets<AudioSource>) -> Vec<MusicTrack> {
    let mut paths = std::fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_mp3(path))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    paths
        .into_iter()
        .filter_map(|source_path| {
            let bytes = std::fs::read(&source_path).ok()?;
            if !looks_like_mp3(&bytes) {
                return None;
            }
            let name = source_path
                .file_stem()
                .map(|name| name.to_string_lossy().replace(['_', '-'], " "))
                .unwrap_or_else(|| "Unknown Track".to_string());
            Some(MusicTrack {
                name,
                source_path,
                handle: sources.add(AudioSource {
                    bytes: bytes.into(),
                }),
            })
        })
        .collect()
}

fn setup_music_player(
    mut commands: Commands,
    mut sources: ResMut<Assets<AudioSource>>,
    mut deck: ResMut<MusicDeck>,
) {
    let directory = configured_music_dir();
    let _ = std::fs::create_dir_all(&directory);
    deck.tracks = scan_music_tracks(&directory, &mut sources);
    deck.current_index = deck.current_index.min(deck.tracks.len().saturating_sub(1));

    commands
        .spawn((
            MusicPlayerOverlay,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(18.0),
                bottom: Val::Px(18.0),
                width: Val::Px(520.0),
                padding: UiRect::all(Val::Px(14.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.025, 0.07, 0.94)),
            BorderColor::all(Color::srgb(0.2, 0.85, 1.0)),
        ))
        .with_children(|root| {
            root.spawn((
                MusicPlayerText,
                Text::new("STARFALL MUSIC DECK"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.95, 1.0)),
            ));
        });
}

fn stop_music_entity(commands: &mut Commands, deck: &mut MusicDeck) {
    if let Some(entity) = deck.playback_entity.take() {
        commands.entity(entity).despawn();
    }
}

fn music_player_input_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    player_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    mut capture: ResMut<UiGameplayCapture>,
    mut deck: ResMut<MusicDeck>,
    mut reload_ev: MessageWriter<AudioLibraryReloadEvent>,
) {
    if keyboard.just_pressed(KeyCode::F6) {
        if deck.overlay_visible {
            deck.overlay_visible = false;
            if capture.owner == Some(0) {
                capture.owner = None;
            }
        } else if capture.owner.is_none() {
            deck.overlay_visible = true;
            capture.owner = Some(0);
        }
    }
    if !deck.overlay_visible {
        return;
    }

    let controller = player_q
        .iter()
        .find(|(index, _)| index.0 == 0)
        .map(|(_, input)| input);
    let previous =
        keyboard.just_pressed(KeyCode::ArrowLeft) || controller.is_some_and(|input| input.ui_left);
    let next = keyboard.just_pressed(KeyCode::ArrowRight)
        || controller.is_some_and(|input| input.ui_right);
    let pause =
        keyboard.just_pressed(KeyCode::Space) || controller.is_some_and(|input| input.ui_confirm);

    if previous {
        stop_music_entity(&mut commands, &mut deck);
        deck.advance(-1);
        deck.paused = false;
    } else if next {
        stop_music_entity(&mut commands, &mut deck);
        deck.advance(1);
        deck.paused = false;
    }
    if pause {
        deck.paused = !deck.paused;
    }
    if keyboard.just_pressed(KeyCode::KeyS) {
        deck.shuffle = !deck.shuffle;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        stop_music_entity(&mut commands, &mut deck);
        reload_ev.write(AudioLibraryReloadEvent);
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        deck.overlay_visible = false;
        if capture.owner == Some(0) {
            capture.owner = None;
        }
    }
}

fn reload_music_library_system(
    mut commands: Commands,
    mut reload_ev: MessageReader<AudioLibraryReloadEvent>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut deck: ResMut<MusicDeck>,
) {
    if reload_ev.read().next().is_none() {
        return;
    }
    stop_music_entity(&mut commands, &mut deck);
    deck.tracks = scan_music_tracks(&configured_music_dir(), &mut sources);
    deck.current_index = 0;
    deck.paused = false;
    deck.generation = deck.generation.wrapping_add(1);
}

fn music_playback_system(
    mut commands: Commands,
    settings: Res<GameSettings>,
    mut deck: ResMut<MusicDeck>,
    mut sink_q: Query<&mut AudioSink, With<MusicPlayback>>,
) {
    if let Some(entity) = deck.playback_entity {
        if let Ok(mut sink) = sink_q.get_mut(entity) {
            sink.set_volume(Volume::Linear(
                (settings.master_volume * settings.music_volume).clamp(0.0, 1.0),
            ));
            if deck.paused {
                sink.pause();
            } else {
                sink.play();
            }
            return;
        }
        // Audio output adds the sink asynchronously. Keep a newly spawned
        // player alive for one frame, then treat a missing entity as finished.
        if commands.get_entity(entity).is_ok() {
            return;
        }
        deck.playback_entity = None;
        deck.advance(1);
    }

    if deck.paused || settings.music_volume <= 0.001 {
        return;
    }
    let Some(handle) = deck.current().map(|track| track.handle.clone()) else {
        return;
    };
    let entity = commands
        .spawn((
            Name::new("Background Music Track"),
            MusicPlayback,
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(
                (settings.master_volume * settings.music_volume).clamp(0.0, 1.0),
            )),
        ))
        .id();
    deck.playback_entity = Some(entity);
}

fn update_music_overlay_system(
    deck: Res<MusicDeck>,
    action_sfx: Option<Res<ActionSfxRegistry>>,
    mut root_q: Query<&mut Visibility, With<MusicPlayerOverlay>>,
    mut text_q: Query<&mut Text, With<MusicPlayerText>>,
) {
    if !deck.is_changed() {
        return;
    }
    for mut visibility in root_q.iter_mut() {
        *visibility = if deck.overlay_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut text) = text_q.single_mut() {
        let directory = configured_music_dir();
        let track = deck
            .current()
            .map(|track| track.name.as_str())
            .unwrap_or("No MP3 tracks found");
        let status = if deck.paused { "PAUSED" } else { "PLAYING" };
        *text = Text::new(format!(
            "STARFALL MUSIC DECK  [{status}]\nTrack {}/{}: {}\nShuffle: {}\nCustom action SFX: {} assigned\n\n←/→ or D-pad: Previous/Next    Space/A: Pause\nS: Shuffle    R: Rescan music + action SFX    Esc/F6: Close\nMusic: {}\nAction SFX: {}",
            if deck.tracks.is_empty() { 0 } else { deck.current_index + 1 },
            deck.tracks.len(),
            track,
            if deck.shuffle { "ON" } else { "OFF" },
            action_sfx.map(|registry| registry.assigned_count()).unwrap_or(0),
            directory.display(),
            action_sfx_directory().display()
        ));
    }
}

pub struct MusicPlayerPlugin;

impl Plugin for MusicPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicDeck>()
            .add_systems(Startup, setup_music_player)
            .add_systems(
                Update,
                (
                    music_player_input_system,
                    reload_music_library_system,
                    music_playback_system,
                    update_music_overlay_system,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_filter_is_case_insensitive_and_rejects_other_files() {
        assert!(is_mp3(Path::new("track.mp3")));
        assert!(is_mp3(Path::new("TRACK.MP3")));
        assert!(!is_mp3(Path::new("notes.txt")));
        assert!(!is_mp3(Path::new("fake.mp3.exe")));
    }

    #[test]
    fn playlist_navigation_wraps_in_both_directions() {
        let mut deck = MusicDeck {
            tracks: (0..3)
                .map(|index| MusicTrack {
                    name: format!("Track {index}"),
                    source_path: PathBuf::from(format!("{index}.mp3")),
                    handle: Handle::default(),
                })
                .collect(),
            ..default()
        };
        deck.advance(-1);
        assert_eq!(deck.current_index, 2);
        deck.advance(1);
        assert_eq!(deck.current_index, 0);
    }

    #[test]
    fn mp3_header_check_rejects_renamed_text_files() {
        assert!(looks_like_mp3(b"ID3\x04\x00\x00"));
        assert!(looks_like_mp3(&[0x00, 0xFF, 0xFB, 0x90, 0x64]));
        assert!(!looks_like_mp3(b"this is not audio"));
    }
}
