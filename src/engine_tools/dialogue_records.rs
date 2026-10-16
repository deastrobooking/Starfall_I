//! Durable Dialogue Forge records and project-local voice recording.
//!
//! Dialogue graphs intentionally live in `Ui` content payloads for project
//! schema compatibility. The typed wrapper distinguishes them from ordinary UI
//! recipes and gives gameplay/cutscene playback one deterministic timeline.
//!
//! The graph/node/timeline schema and its pure validation now live in
//! `crate::world::dialogue_schema` so gameplay can read and validate
//! dialogue without depending on `engine_tools` — this module keeps only the
//! authoring side: records that take `&mut ForgeProject`, and voice capture.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Stream,
};

use super::persistence::{
    ContentCategory, ContentPayload, ForgeProject, GenericRecipeDraft, CURRENT_PROJECT_SCHEMA,
};
use crate::world::dialogue_schema::{
    safe_asset_path, validate_dialogue, DialogueGraph, MAX_VOICE_SECONDS,
};

const GRAPH_FIELD: &str = "dialogue_graph";
const RECORD_KIND_FIELD: &str = "record_kind";
const RECORD_KIND: &str = "starfall_dialogue_graph_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueRecordError {
    MissingRecord(String),
    WrongRecordKind(String),
    InvalidGraph(Vec<String>),
    Serialization(String),
}

pub fn create_dialogue(
    project: &mut ForgeProject,
    display_name: &str,
) -> Result<String, DialogueRecordError> {
    let id = project
        .create_content(ContentCategory::Ui, display_name)
        .map_err(|error| DialogueRecordError::Serialization(format!("{error:?}")))?;
    save_dialogue(project, &id, &DialogueGraph::default())?;
    if let Some(record) = project
        .records
        .iter_mut()
        .find(|record| record.content_id == id)
    {
        record.tags = vec!["dialogue".into(), "timeline".into(), "draft".into()];
    }
    Ok(id)
}

pub fn save_dialogue(
    project: &mut ForgeProject,
    content_id: &str,
    graph: &DialogueGraph,
) -> Result<(), DialogueRecordError> {
    let errors = validate_dialogue(graph);
    if !errors.is_empty() {
        return Err(DialogueRecordError::InvalidGraph(errors));
    }
    let Some(ContentPayload::Ui(recipe)) = project.payloads.get_mut(content_id) else {
        return Err(DialogueRecordError::MissingRecord(content_id.into()));
    };
    recipe.schema_version = CURRENT_PROJECT_SCHEMA;
    recipe
        .fields
        .insert(RECORD_KIND_FIELD.into(), serde_json::json!(RECORD_KIND));
    recipe.fields.insert(
        GRAPH_FIELD.into(),
        serde_json::to_value(graph)
            .map_err(|error| DialogueRecordError::Serialization(error.to_string()))?,
    );
    Ok(())
}

pub fn load_dialogue(
    project: &ForgeProject,
    content_id: &str,
) -> Result<DialogueGraph, DialogueRecordError> {
    let Some(ContentPayload::Ui(GenericRecipeDraft { fields, .. })) =
        project.payloads.get(content_id)
    else {
        return Err(DialogueRecordError::MissingRecord(content_id.into()));
    };
    if fields
        .get(RECORD_KIND_FIELD)
        .and_then(serde_json::Value::as_str)
        != Some(RECORD_KIND)
    {
        return Err(DialogueRecordError::WrongRecordKind(content_id.into()));
    }
    let graph = serde_json::from_value(
        fields
            .get(GRAPH_FIELD)
            .cloned()
            .ok_or_else(|| DialogueRecordError::WrongRecordKind(content_id.into()))?,
    )
    .map_err(|error| DialogueRecordError::Serialization(error.to_string()))?;
    let errors = validate_dialogue(&graph);
    if errors.is_empty() {
        Ok(graph)
    } else {
        Err(DialogueRecordError::InvalidGraph(errors))
    }
}

pub fn validate_dialogue_records(project: &ForgeProject) -> Vec<String> {
    project
        .records
        .iter()
        .filter(|record| is_dialogue_record(project, record))
        .flat_map(|record| match load_dialogue(project, &record.content_id) {
            Ok(_) => Vec::new(),
            Err(error) => vec![format!("{}: {error:?}", record.content_id)],
        })
        .collect()
}

/// A Ui-category record only counts as dialogue when its payload carries the
/// typed graph marker; ordinary Ui recipes stay with the UI pipeline.
fn is_dialogue_record(project: &ForgeProject, record: &super::persistence::ContentRecord) -> bool {
    record.category == ContentCategory::Ui
        && matches!(
            project.payloads.get(&record.content_id),
            Some(ContentPayload::Ui(recipe))
                if recipe.fields.get(RECORD_KIND_FIELD).and_then(serde_json::Value::as_str)
                    == Some(RECORD_KIND)
        )
}

/// Every dialogue record in the project as `(content_id, display_name)`,
/// sorted by content id for deterministic publish output.
pub fn dialogue_entries(project: &ForgeProject) -> Vec<(String, String)> {
    let mut entries = project
        .records
        .iter()
        .filter(|record| is_dialogue_record(project, record))
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// Live microphone capture. It owns the native stream until `finish`; no input
/// device is opened before `start`, keeping editor startup permission-free.
pub struct VoiceRecorder {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    input_channels: u16,
}

impl VoiceRecorder {
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No microphone input device is available")?;
        let supported = device
            .default_input_config()
            .map_err(|error| format!("Could not read microphone format: {error}"))?;
        let sample_rate = supported.sample_rate();
        let input_channels = supported.channels();
        let config = supported.config();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&samples);
        let error_callback = |error| eprintln!("Starfall voice recording stream error: {error}");
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| push_mono(data.iter().copied(), input_channels, &sink),
                error_callback,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    push_mono(
                        data.iter().map(|sample| *sample as f32 / i16::MAX as f32),
                        input_channels,
                        &sink,
                    )
                },
                error_callback,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    push_mono(
                        data.iter()
                            .map(|sample| *sample as f32 / u16::MAX as f32 * 2.0 - 1.0),
                        input_channels,
                        &sink,
                    )
                },
                error_callback,
                None,
            ),
            format => return Err(format!("Unsupported microphone sample format: {format}")),
        }
        .map_err(|error| format!("Could not open microphone: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("Could not start microphone: {error}"))?;
        Ok(Self {
            stream,
            samples,
            sample_rate,
            input_channels,
        })
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.samples
            .lock()
            .map(|samples| samples.len() as f32 / self.sample_rate as f32)
            .unwrap_or(0.0)
    }

    pub fn finish(
        self,
        project_root: &Path,
        asset_path: &str,
        take: u16,
    ) -> Result<crate::world::dialogue_schema::VoiceClip, String> {
        if !safe_asset_path(asset_path)
            || Path::new(asset_path)
                .extension()
                .and_then(|value| value.to_str())
                != Some("mp3")
        {
            return Err("Recorded voice output must be a safe MP3 asset path".into());
        }
        drop(self.stream);
        let samples = Arc::try_unwrap(self.samples)
            .map_err(|_| "Microphone callback is still active")?
            .into_inner()
            .map_err(|_| "Microphone sample buffer was poisoned")?;
        let duration_seconds = samples.len() as f32 / self.sample_rate as f32;
        if samples.is_empty() || duration_seconds > MAX_VOICE_SECONDS {
            return Err("Recording must be between 0 and 300 seconds".into());
        }
        let output = project_root.join("assets").join(asset_path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        encode_pcm16_mp3(&output, self.sample_rate, &samples)?;
        Ok(crate::world::dialogue_schema::VoiceClip {
            asset_path: asset_path.into(),
            duration_seconds,
            sample_rate: self.sample_rate,
            channels: 1,
            take,
            transcript_locked: false,
        })
    }

    pub fn input_channels(&self) -> u16 {
        self.input_channels
    }
}

fn push_mono(samples: impl Iterator<Item = f32>, channels: u16, sink: &Arc<Mutex<Vec<f32>>>) {
    let channel_count = usize::from(channels.max(1));
    let input = samples.collect::<Vec<_>>();
    if let Ok(mut output) = sink.lock() {
        output.extend(
            input
                .chunks(channel_count)
                .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32),
        );
    }
}

fn pcm16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
    for sample in samples {
        let pcm = ((*sample).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

/// Encode a compact 96-kbps mono MP3 without retaining an intermediate WAV.
/// Forge accepts a bundled/system LAME or FFmpeg executable and reports a
/// clear error if neither production encoder is installed.
fn encode_pcm16_mp3(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), String> {
    let parent = path.parent().ok_or("MP3 output has no parent directory")?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("MP3 output file name is invalid")?;
    let temporary = parent.join(format!(".{file_name}.recording.tmp.mp3"));
    let pcm = pcm16_bytes(samples);
    let rate = sample_rate.to_string();
    let rate_khz = format!("{:.3}", sample_rate as f32 / 1000.0);
    let mut attempts = 0;

    for executable in ["/opt/homebrew/bin/lame", "/usr/local/bin/lame", "lame"] {
        attempts += 1;
        let args = [
            "--silent",
            "-r",
            "-s",
            rate_khz.as_str(),
            "--bitwidth",
            "16",
            "--signed",
            "--little-endian",
            "-m",
            "m",
            "-b",
            "96",
            "-",
        ];
        if run_encoder(executable, &args, &temporary, &pcm).is_ok() {
            return promote_encoded_mp3(&temporary, path);
        }
    }

    for executable in [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "ffmpeg",
    ] {
        attempts += 1;
        let args = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "s16le",
            "-ar",
            rate.as_str(),
            "-ac",
            "1",
            "-i",
            "pipe:0",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "96k",
        ];
        if run_encoder(executable, &args, &temporary, &pcm).is_ok() {
            return promote_encoded_mp3(&temporary, path);
        }
    }
    let _ = fs::remove_file(&temporary);
    Err(format!(
        "MP3 encoding requires LAME or FFmpeg ({attempts} encoder attempts failed)"
    ))
}

fn run_encoder(executable: &str, args: &[&str], output: &Path, pcm: &[u8]) -> Result<(), String> {
    let _ = fs::remove_file(output);
    let mut child = Command::new(executable)
        .args(args)
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{executable}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("{executable}: encoder stdin unavailable"))?
        .write_all(pcm)
        .map_err(|error| format!("{executable}: {error}"))?;
    let output_status = child
        .wait_with_output()
        .map_err(|error| format!("{executable}: {error}"))?;
    if output_status.status.success() && output.exists() {
        Ok(())
    } else {
        Err(format!(
            "{executable}: {}",
            String::from_utf8_lossy(&output_status.stderr).trim()
        ))
    }
}

fn promote_encoded_mp3(temporary: &Path, path: &Path) -> Result<(), String> {
    let bytes = fs::read(temporary).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(temporary);
    if bytes.len() < 4 {
        return Err("MP3 encoder produced an empty take".into());
    }
    super::persistence::atomic_write(path, &bytes).map_err(|error| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::dialogue_schema::DialoguePlaybackMode;

    #[test]
    fn graph_round_trips_with_gameplay_and_cutscene_cues() {
        let mut project = ForgeProject::default();
        let id = create_dialogue(&mut project, "Dragon Castle Intro").unwrap();
        let mut graph = load_dialogue(&project, &id).unwrap();
        graph.mode = DialoguePlaybackMode::Cutscene;
        graph.pause_gameplay = true;
        graph.nodes[0]
            .timeline
            .push(crate::world::dialogue_schema::TimelineCue {
                at_seconds: 0.1,
                action: crate::world::dialogue_schema::TimelineAction::CameraShot {
                    camera_rig: "castle_wide".into(),
                    target: Some("dragon_boss".into()),
                    blend_seconds: 0.4,
                },
            });
        graph.nodes[0]
            .timeline
            .push(crate::world::dialogue_schema::TimelineCue {
                at_seconds: 0.3,
                action: crate::world::dialogue_schema::TimelineAction::PlayAnimation {
                    actor: "dragon_boss".into(),
                    clip: "roar".into(),
                    speed: 1.0,
                    looping: false,
                    blend_seconds: 0.15,
                },
            });
        save_dialogue(&mut project, &id, &graph).unwrap();
        assert_eq!(load_dialogue(&project, &id).unwrap(), graph);
    }

    #[test]
    fn mp3_encoder_writes_a_compact_playable_take() {
        let path = std::env::temp_dir().join(format!("starfall_voice_{}.mp3", std::process::id()));
        let samples = (0..48_000)
            .map(|index| ((index as f32 / 48_000.0) * std::f32::consts::TAU * 220.0).sin() * 0.2)
            .collect::<Vec<_>>();
        encode_pcm16_mp3(&path, 48_000, &samples).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"ID3") || bytes.windows(2).any(|frame| frame == [0xff, 0xfb]));
        assert!(
            bytes.len() < 20_000,
            "one second voice take should remain compact"
        );
        let _ = fs::remove_file(path);
    }
}
