//! Durable Dialogue Forge records and project-local voice recording.
//!
//! Dialogue graphs intentionally live in `Ui` content payloads for project
//! schema compatibility. The typed wrapper distinguishes them from ordinary UI
//! recipes and gives gameplay/cutscene playback one deterministic timeline.

use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Stream,
};
use serde::{Deserialize, Serialize};

use super::persistence::{
    ContentCategory, ContentPayload, ForgeProject, GenericRecipeDraft, CURRENT_PROJECT_SCHEMA,
};

const GRAPH_FIELD: &str = "dialogue_graph";
const RECORD_KIND_FIELD: &str = "record_kind";
const RECORD_KIND: &str = "starfall_dialogue_graph_v1";
pub const MAX_DIALOGUE_NODES: usize = 512;
pub const MAX_TIMELINE_CUES: usize = 2048;
pub const MAX_VOICE_SECONDS: f32 = 300.0;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DialoguePlaybackMode {
    #[default]
    Gameplay,
    Cutscene,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DialogueGraph {
    pub graph_version: u32,
    pub entry_node: String,
    pub mode: DialoguePlaybackMode,
    #[serde(default)]
    pub skippable: bool,
    #[serde(default)]
    pub pause_gameplay: bool,
    #[serde(default)]
    pub nodes: Vec<DialogueNode>,
}

impl Default for DialogueGraph {
    fn default() -> Self {
        Self {
            graph_version: 1,
            entry_node: "opening".into(),
            mode: DialoguePlaybackMode::Gameplay,
            skippable: true,
            pause_gameplay: false,
            nodes: vec![DialogueNode::speech(
                "opening",
                "Narrator",
                "A new adventure begins.",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DialogueNode {
    pub node_id: String,
    pub speaker: String,
    pub text: String,
    #[serde(default)]
    pub portrait_content_id: Option<String>,
    #[serde(default)]
    pub voice: Option<VoiceClip>,
    #[serde(default)]
    pub minimum_hold_seconds: f32,
    #[serde(default)]
    pub choices: Vec<DialogueChoice>,
    #[serde(default)]
    pub timeline: Vec<TimelineCue>,
    #[serde(default)]
    pub next_node: Option<String>,
}

impl DialogueNode {
    pub fn speech(
        id: impl Into<String>,
        speaker: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            node_id: id.into(),
            speaker: speaker.into(),
            text: text.into(),
            portrait_content_id: None,
            voice: None,
            minimum_hold_seconds: 0.25,
            choices: Vec::new(),
            timeline: Vec::new(),
            next_node: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DialogueChoice {
    pub label: String,
    pub target_node: String,
    #[serde(default)]
    pub required_flag: Option<String>,
    #[serde(default)]
    pub set_flag: Option<String>,
}

/// Stable-ID playback cursor shared by runtime conversations and cutscenes.
/// It never stores ECS entities, so save/reload and editor playtest restarts can
/// resume without coupling authored data to a particular world instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DialogueCursor {
    pub node_id: String,
}

impl DialogueCursor {
    pub fn start(graph: &DialogueGraph) -> Result<Self, String> {
        graph
            .nodes
            .iter()
            .any(|node| node.node_id == graph.entry_node)
            .then(|| Self {
                node_id: graph.entry_node.clone(),
            })
            .ok_or_else(|| "Dialogue entry node is missing".into())
    }

    pub fn node<'a>(&self, graph: &'a DialogueGraph) -> Option<&'a DialogueNode> {
        graph.nodes.iter().find(|node| node.node_id == self.node_id)
    }

    pub fn advance(&mut self, graph: &DialogueGraph) -> Result<bool, String> {
        let Some(next) = self.node(graph).and_then(|node| node.next_node.as_deref()) else {
            return Ok(false);
        };
        self.move_to(graph, next)?;
        Ok(true)
    }

    pub fn choose(&mut self, graph: &DialogueGraph, choice_index: usize) -> Result<(), String> {
        let target = self
            .node(graph)
            .and_then(|node| node.choices.get(choice_index))
            .map(|choice| choice.target_node.clone())
            .ok_or_else(|| format!("Choice {choice_index} is unavailable"))?;
        self.move_to(graph, &target)
    }

    fn move_to(&mut self, graph: &DialogueGraph, node_id: &str) -> Result<(), String> {
        if graph.nodes.iter().any(|node| node.node_id == node_id) {
            self.node_id = node_id.into();
            Ok(())
        } else {
            Err(format!("Dialogue target '{node_id}' is missing"))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceClip {
    /// Asset-relative path, normally `voice/dialogue/<graph>/<take>.wav`.
    pub asset_path: String,
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default)]
    pub take: u16,
    #[serde(default)]
    pub transcript_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineCue {
    pub at_seconds: f32,
    #[serde(flatten)]
    pub action: TimelineAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TimelineAction {
    PlayAnimation {
        actor: String,
        clip: String,
        #[serde(default = "one")]
        speed: f32,
        #[serde(default)]
        looping: bool,
        #[serde(default)]
        blend_seconds: f32,
    },
    CameraShot {
        camera_rig: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default = "default_blend")]
        blend_seconds: f32,
    },
    PlayAudio {
        asset_path: String,
        #[serde(default = "one")]
        volume: f32,
        #[serde(default)]
        spatial_actor: Option<String>,
    },
    GameplaySignal {
        signal: String,
        #[serde(default)]
        payload: serde_json::Value,
    },
    SetActorVisible {
        actor: String,
        visible: bool,
    },
    WaitForInput,
}

const fn one() -> f32 {
    1.0
}
const fn default_blend() -> f32 {
    0.25
}

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

pub fn validate_dialogue(graph: &DialogueGraph) -> Vec<String> {
    let mut errors = Vec::new();
    if graph.nodes.is_empty() || graph.nodes.len() > MAX_DIALOGUE_NODES {
        errors.push(format!(
            "dialogue must contain 1..={MAX_DIALOGUE_NODES} nodes"
        ));
    }
    let ids = graph
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if ids.len() != graph.nodes.len() {
        errors.push("node ids must be unique".into());
    }
    if !ids.contains(graph.entry_node.as_str()) {
        errors.push("entry node does not exist".into());
    }
    for node in &graph.nodes {
        if node.node_id.trim().is_empty() || node.text.trim().is_empty() {
            errors.push(format!("node '{}' needs an id and text", node.node_id));
        }
        if node.minimum_hold_seconds < 0.0 || !node.minimum_hold_seconds.is_finite() {
            errors.push(format!("node '{}' has an invalid hold time", node.node_id));
        }
        if node.timeline.len() > MAX_TIMELINE_CUES {
            errors.push(format!(
                "node '{}' has too many timeline cues",
                node.node_id
            ));
        }
        if node
            .next_node
            .as_deref()
            .is_some_and(|target| !ids.contains(target))
        {
            errors.push(format!("node '{}' targets missing next node", node.node_id));
        }
        for choice in &node.choices {
            if choice.label.trim().is_empty() || !ids.contains(choice.target_node.as_str()) {
                errors.push(format!("node '{}' has an invalid choice", node.node_id));
            }
        }
        for cue in &node.timeline {
            if cue.at_seconds < 0.0 || !cue.at_seconds.is_finite() {
                errors.push(format!("node '{}' has an invalid cue time", node.node_id));
            }
        }
        if let Some(voice) = &node.voice {
            if voice.duration_seconds <= 0.0
                || voice.duration_seconds > MAX_VOICE_SECONDS
                || !safe_asset_path(&voice.asset_path)
            {
                errors.push(format!(
                    "node '{}' has invalid voice metadata",
                    node.node_id
                ));
            }
        }
    }
    errors
}

pub fn validate_dialogue_records(project: &ForgeProject) -> Vec<String> {
    project
        .records
        .iter()
        .filter(|record| {
            record.category == ContentCategory::Ui
                && matches!(
                    project.payloads.get(&record.content_id),
                    Some(ContentPayload::Ui(recipe))
                        if recipe.fields.get(RECORD_KIND_FIELD).and_then(serde_json::Value::as_str)
                            == Some(RECORD_KIND)
                )
        })
        .flat_map(|record| match load_dialogue(project, &record.content_id) {
            Ok(_) => Vec::new(),
            Err(error) => vec![format!("{}: {error:?}", record.content_id)],
        })
        .collect()
}

fn safe_asset_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        && matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("wav" | "mp3" | "ogg")
        )
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
    ) -> Result<VoiceClip, String> {
        if !safe_asset_path(asset_path) {
            return Err("Voice output must be a safe WAV asset path".into());
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
        write_pcm16_wav(&output, self.sample_rate, &samples)?;
        Ok(VoiceClip {
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

fn write_pcm16_wav(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), String> {
    let data_len =
        u32::try_from(samples.len().saturating_mul(2)).map_err(|_| "Recording is too large")?;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let pcm = ((*sample).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    super::persistence::atomic_write(path, &bytes).map_err(|error| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_round_trips_with_gameplay_and_cutscene_cues() {
        let mut project = ForgeProject::default();
        let id = create_dialogue(&mut project, "Dragon Castle Intro").unwrap();
        let mut graph = load_dialogue(&project, &id).unwrap();
        graph.mode = DialoguePlaybackMode::Cutscene;
        graph.pause_gameplay = true;
        graph.nodes[0].timeline.push(TimelineCue {
            at_seconds: 0.1,
            action: TimelineAction::CameraShot {
                camera_rig: "castle_wide".into(),
                target: Some("dragon_boss".into()),
                blend_seconds: 0.4,
            },
        });
        graph.nodes[0].timeline.push(TimelineCue {
            at_seconds: 0.3,
            action: TimelineAction::PlayAnimation {
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
    fn validation_rejects_dangling_choices_and_unsafe_voice_paths() {
        let mut graph = DialogueGraph::default();
        graph.nodes[0].choices.push(DialogueChoice {
            label: "Go".into(),
            target_node: "missing".into(),
            required_flag: None,
            set_flag: None,
        });
        graph.nodes[0].voice = Some(VoiceClip {
            asset_path: "../outside.wav".into(),
            duration_seconds: 1.0,
            sample_rate: 48_000,
            channels: 1,
            take: 1,
            transcript_locked: false,
        });
        assert!(validate_dialogue(&graph).len() >= 2);
    }

    #[test]
    fn wav_encoder_writes_a_valid_mono_pcm_header() {
        let path = std::env::temp_dir().join(format!("starfall_voice_{}.wav", std::process::id()));
        write_pcm16_wav(&path, 48_000, &[0.0, 0.5, -0.5]).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(bytes.len(), 50);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn stable_cursor_traverses_linear_and_choice_nodes() {
        let mut graph = DialogueGraph::default();
        graph.nodes[0].next_node = Some("choice".into());
        graph
            .nodes
            .push(DialogueNode::speech("choice", "Nova", "Which route?"));
        graph
            .nodes
            .push(DialogueNode::speech("sky", "Nova", "To the clouds!"));
        graph.nodes[1].choices.push(DialogueChoice {
            label: "Sky road".into(),
            target_node: "sky".into(),
            required_flag: None,
            set_flag: Some("picked_sky".into()),
        });
        let mut cursor = DialogueCursor::start(&graph).unwrap();
        assert!(cursor.advance(&graph).unwrap());
        cursor.choose(&graph, 0).unwrap();
        assert_eq!(cursor.node_id, "sky");
        assert!(!cursor.advance(&graph).unwrap());
    }
}
