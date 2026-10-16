//! Dialogue graph schema and playback-cursor logic — the pure-data half of
//! what used to live in `engine_tools::dialogue_records`.
//!
//! Split out so gameplay (`world::dialogue_director`, `world::published_content`)
//! can read and validate dialogue graphs without depending on `engine_tools`,
//! which still owns the authoring side (`create_dialogue`/`save_dialogue`/
//! `load_dialogue`, all of which take `&mut ForgeProject`, and voice
//! recording, which needs `cpal`/a microphone).

use serde::{Deserialize, Serialize};

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
    /// Asset-relative path, normally `voice/dialogue/<graph>/<take>.mp3`.
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

/// Structural validation only — no `ForgeProject` dependency, so gameplay can
/// run this on a published/baked graph the same way authoring runs it on a
/// draft.
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

/// Shared by validation here and `engine_tools::dialogue_records::VoiceRecorder`'s
/// authoring-time write path — one path-safety rule either side relies on.
pub fn safe_asset_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    !path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        && matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("wav" | "mp3" | "ogg")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

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
