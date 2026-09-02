//! Runtime director for Designer-published dialogue graphs.
//!
//! The Designer authors typed [`DialogueGraph`] records in the Dialogue Forge
//! and publishes them into [`PublishedDialogueCatalog`]; this module plays a
//! published graph deterministically at runtime. The director never stores ECS
//! entities in its persistent state — the [`DialogueCursor`] and flag set are
//! stable-ID data, so a conversation can suspend, save, and resume without
//! coupling to a world instance.
//!
//! Presentation (UI panel, voice playback) and cue execution (animation,
//! camera, audio, gameplay signals) subscribe to the [`DialogueStateChange`]
//! stream instead of polling internals, keeping the director testable headless.

use bevy::prelude::*;
use std::collections::BTreeSet;

use crate::engine_tools::dialogue_records::{
    DialogueCursor, DialogueGraph, DialogueNode, TimelineAction, TimelineCue,
};
use crate::engine_tools::PublishedDialogueCatalog;

/// Identifies which cue a [`DialogueStateChange::CueFired`] refers to. Cues
/// are addressed by the stable node id plus their position in that node's
/// timeline — never by runtime ordering state.
#[derive(Debug, Clone, PartialEq)]
pub struct FiredCue<'a> {
    pub node_id: &'a str,
    pub cue_index: usize,
    pub cue: &'a TimelineCue,
}

/// Emitted whenever the director's observable state moves. Every variant
/// carries the owning player so a 1–4 player session can mix one active
/// conversation with free-roaming partners without cross-player leakage.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum DialogueStateChange {
    /// A conversation began on the graph's entry node.
    Started {
        content_id: String,
        player_index: u8,
    },
    /// The cursor moved to a new node (advance, choice, or signal jump).
    NodeEntered {
        content_id: String,
        node_id: String,
        player_index: u8,
    },
    /// A timeline cue crossed the node clock. Delivered exactly once per cue
    /// per node visit, in authored time order.
    CueFired {
        content_id: String,
        node_id: String,
        cue_index: usize,
        action: TimelineAction,
        player_index: u8,
    },
    /// The conversation is now blocked on input: either a choice is required
    /// or a `WaitForInput` cue fired.
    AwaitingInput {
        content_id: String,
        player_index: u8,
    },
    /// The graph reached an exit node, the owner cancelled, or a close was
    /// requested by gameplay. `completed` distinguishes reaching the end of
    /// the graph from an early cancel.
    Closed {
        content_id: String,
        player_index: u8,
        completed: bool,
    },
}

/// One active conversation's playback state. All fields are stable-ID data.
#[derive(Debug, Clone)]
struct ActiveConversation {
    content_id: String,
    cursor: DialogueCursor,
    player_index: u8,
    /// Seconds elapsed on the current node; drives ordered cue firing and the
    /// authored minimum hold before advancing.
    node_clock: f32,
    /// Cues already fired on this node visit (authored order indices).
    fired_cues: BTreeSet<usize>,
    /// Flags accumulated by choices taken during this conversation. These
    /// layer over the persistent flag store so a replayed conversation
    /// re-evaluates conditions deterministically.
    session_flags: BTreeSet<String>,
    /// A `WaitForInput` cue (or an empty choice-less node) is holding the
    /// cursor until the owner advances.
    waiting_for_input: bool,
}

impl ActiveConversation {
    fn enter_node(&mut self, cursor: DialogueCursor) {
        self.cursor = cursor;
        self.node_clock = 0.0;
        self.fired_cues.clear();
        self.waiting_for_input = false;
    }
}

/// Persistent per-campaign dialogue flags set by `set_flag` choices and
/// consulted by `required_flag` conditions. Kept separate from the director so
/// a future save-schema slice can serialize this resource verbatim.
#[derive(Resource, Debug, Default, Clone)]
pub struct DialogueFlags {
    flags: BTreeSet<String>,
}

impl DialogueFlags {
    pub fn is_set(&self, flag: &str) -> bool {
        self.flags.contains(flag)
    }

    pub fn set(&mut self, flag: impl Into<String>) {
        self.flags.insert(flag.into());
    }

    pub fn clear(&mut self, flag: &str) {
        self.flags.remove(flag);
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.flags.iter().map(String::as_str)
    }
}

/// Runtime director: at most one conversation owns the dialogue channel at a
/// time (the same policy as the static `DiscussionState`). The conversation's
/// owning player is the only input authority for advancing and choosing.
#[derive(Resource, Debug, Default)]
pub struct DialogueDirector {
    active: Option<ActiveConversation>,
}

impl DialogueDirector {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn active_content_id(&self) -> Option<&str> {
        self.active.as_ref().map(|c| c.content_id.as_str())
    }

    pub fn owner(&self) -> Option<u8> {
        self.active.as_ref().map(|c| c.player_index)
    }

    /// Begin the published graph for `content_id`, owned by `player_index`.
    /// Returns `false` without side effects when the graph is unknown, has a
    /// broken entry node, or another conversation already owns the channel.
    pub fn start(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        content_id: &str,
        player_index: u8,
        changes: &mut Vec<DialogueStateChange>,
    ) -> bool {
        if self.active.is_some() {
            return false;
        }
        let Some(graph) = catalog.get(content_id) else {
            return false;
        };
        let Ok(cursor) = DialogueCursor::start(graph) else {
            return false;
        };
        let node_id = cursor.node_id.clone();
        self.active = Some(ActiveConversation {
            content_id: content_id.to_string(),
            cursor,
            player_index,
            node_clock: 0.0,
            fired_cues: BTreeSet::new(),
            session_flags: BTreeSet::new(),
            waiting_for_input: false,
        });
        changes.push(DialogueStateChange::Started {
            content_id: content_id.to_string(),
            player_index,
        });
        changes.push(DialogueStateChange::NodeEntered {
            content_id: content_id.to_string(),
            node_id,
            player_index,
        });
        self.refresh_gate(catalog, changes);
        true
    }

    /// The node currently on screen, if any.
    pub fn current_node<'a>(
        &self,
        catalog: &'a PublishedDialogueCatalog,
    ) -> Option<&'a DialogueNode> {
        let active = self.active.as_ref()?;
        catalog
            .get(&active.content_id)
            .and_then(|graph| active.cursor.node(graph))
    }

    /// The graph currently playing, if any.
    pub fn current_graph<'a>(
        &self,
        catalog: &'a PublishedDialogueCatalog,
    ) -> Option<&'a DialogueGraph> {
        catalog.get(&self.active.as_ref()?.content_id)
    }

    /// Choices the owner may pick right now, filtered through
    /// `required_flag` against persistent + session flags.
    pub fn available_choices<'a>(
        &self,
        catalog: &'a PublishedDialogueCatalog,
        flags: &DialogueFlags,
    ) -> Vec<(usize, &'a str)> {
        let Some(active) = self.active.as_ref() else {
            return Vec::new();
        };
        let Some(node) = self.current_node(catalog) else {
            return Vec::new();
        };
        node.choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| {
                choice.required_flag.as_deref().is_none_or(|required| {
                    flags.is_set(required) || active.session_flags.contains(required)
                })
            })
            .map(|(index, choice)| (index, choice.label.as_str()))
            .collect()
    }

    /// Whether the director is blocked waiting on owner input (choice or
    /// explicit wait cue). Presentation uses this to show the input prompt.
    pub fn is_awaiting_input(&self) -> bool {
        self.active
            .as_ref()
            .map(|active| active.waiting_for_input)
            .unwrap_or(false)
    }

    /// Advance the node clock, fire newly-reached cues in authored order, and
    /// auto-advance linear nodes whose hold time has elapsed.
    pub fn tick(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        delta_seconds: f32,
        changes: &mut Vec<DialogueStateChange>,
    ) {
        if self.active.is_none() {
            return;
        }
        // Borrow-split: snapshot the identifiers we need, then mutate.
        let (content_id, player_index) = {
            let active = self.active.as_ref().expect("checked above");
            (active.content_id.clone(), active.player_index)
        };
        let Some(graph) = catalog.get(&content_id) else {
            // The catalog was rebuilt out from under an active conversation
            // (e.g. a live Designer republish). Close rather than reference
            // stale data.
            self.close(changes, false);
            return;
        };

        {
            let active = self.active.as_mut().expect("checked above");
            active.node_clock += delta_seconds.max(0.0);
        }

        // Fire cues in authored order. Only non-input cues fire from the
        // clock; a WaitForInput cue halts the scan (later cues stay pending)
        // and pins the clock at its authored time until the owner advances, so
        // releasing the wait resumes the timeline without skipping content.
        enum Halt {
            No,
            Wait(usize),
        }
        let (events, halt) = {
            let active = self.active.as_ref().expect("checked above");
            let Some(node) = active.cursor.node(graph) else {
                self.close(changes, false);
                return;
            };
            let mut events = Vec::new();
            let mut halt = Halt::No;
            if active.waiting_for_input {
                // Held at a wait: keep the clock parked at the cue's time and
                // let no later cue fire until the owner advances.
                if let Some(wait_time) = node
                    .timeline
                    .iter()
                    .find(|cue| matches!(cue.action, TimelineAction::WaitForInput))
                    .map(|cue| cue.at_seconds)
                {
                    let active = self.active.as_mut().expect("checked above");
                    active.node_clock = active.node_clock.min(wait_time);
                }
            } else {
                for (index, cue) in node.timeline.iter().enumerate() {
                    if active.fired_cues.contains(&index) {
                        continue;
                    }
                    if cue.at_seconds > active.node_clock {
                        break;
                    }
                    if matches!(cue.action, TimelineAction::WaitForInput) {
                        halt = Halt::Wait(index);
                        break;
                    }
                    events.push((index, cue.action.clone()));
                }
            }
            (events, halt)
        };

        for (index, action) in events {
            let active = self.active.as_mut().expect("checked above");
            active.fired_cues.insert(index);
            changes.push(DialogueStateChange::CueFired {
                content_id: content_id.clone(),
                node_id: active.cursor.node_id.clone(),
                cue_index: index,
                action,
                player_index,
            });
        }

        match halt {
            Halt::No => {}
            Halt::Wait(index) => {
                let active = self.active.as_mut().expect("checked above");
                active.fired_cues.insert(index);
                active.waiting_for_input = true;
                let node_id = active.cursor.node_id.clone();
                changes.push(DialogueStateChange::CueFired {
                    content_id: content_id.clone(),
                    node_id,
                    cue_index: index,
                    action: TimelineAction::WaitForInput,
                    player_index,
                });
                changes.push(DialogueStateChange::AwaitingInput {
                    content_id: content_id.clone(),
                    player_index,
                });
            }
        }

        self.refresh_gate(catalog, changes);
        self.maybe_auto_advance(catalog, changes);
    }

    /// The owner pressed advance. Clears a `WaitForInput` hold, or moves a
    /// choice-less node forward once its minimum hold has elapsed.
    pub fn advance(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        player_index: u8,
        changes: &mut Vec<DialogueStateChange>,
    ) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.player_index != player_index {
            return false;
        }
        if active
            .cursor
            .node(
                catalog
                    .get(&active.content_id)
                    .expect("active graph must resolve while active"),
            )
            .is_some_and(|node| !node.choices.is_empty())
        {
            // Choice nodes advance only through `choose`.
            return false;
        }
        if active.waiting_for_input {
            active.waiting_for_input = false;
            // An explicit WaitForInput cue pauses the current node's timeline;
            // owner input releases that gate but must not also skip the node.
            // A later advance may perform the ordinary cursor transition.
            return true;
        }
        let graph = catalog
            .get(&active.content_id)
            .expect("active graph must resolve while active");
        if active.node_clock + f32::EPSILON
            < active
                .cursor
                .node(graph)
                .map(|node| node.minimum_hold_seconds)
                .unwrap_or(0.0)
        {
            return false;
        }
        match active.cursor.advance(graph) {
            Ok(true) => {
                let node_id = active.cursor.node_id.clone();
                active.enter_node(DialogueCursor {
                    node_id: node_id.clone(),
                });
                changes.push(DialogueStateChange::NodeEntered {
                    content_id: active.content_id.clone(),
                    node_id,
                    player_index,
                });
                self.refresh_gate(catalog, changes);
                true
            }
            Ok(false) => {
                self.close(changes, true);
                true
            }
            Err(_) => {
                self.close(changes, false);
                true
            }
        }
    }

    /// The owner picked a choice. Applies `set_flag`, moves the cursor, and
    /// ignores stale or filtered-out indices.
    pub fn choose(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        flags: &mut DialogueFlags,
        player_index: u8,
        choice_index: usize,
        changes: &mut Vec<DialogueStateChange>,
    ) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.player_index != player_index {
            return false;
        }
        let graph = catalog
            .get(&active.content_id)
            .expect("active graph must resolve while active");
        let Some(node) = active.cursor.node(graph) else {
            self.close(changes, false);
            return false;
        };
        let Some(choice) = node.choices.get(choice_index) else {
            return false;
        };
        if choice.required_flag.as_deref().is_some_and(|required| {
            !flags.is_set(required) && !active.session_flags.contains(required)
        }) {
            return false;
        }
        if let Some(set_flag) = &choice.set_flag {
            flags.set(set_flag.clone());
            active.session_flags.insert(set_flag.clone());
        }
        if active.cursor.choose(graph, choice_index).is_err() {
            self.close(changes, false);
            return false;
        }
        active.waiting_for_input = false;
        let node_id = active.cursor.node_id.clone();
        active.enter_node(DialogueCursor {
            node_id: node_id.clone(),
        });
        changes.push(DialogueStateChange::NodeEntered {
            content_id: active.content_id.clone(),
            node_id,
            player_index,
        });
        self.refresh_gate(catalog, changes);
        true
    }

    /// End the conversation. `completed` is true when the graph finished
    /// naturally, false for cancels and error paths.
    pub fn close(&mut self, changes: &mut Vec<DialogueStateChange>, completed: bool) {
        if let Some(active) = self.active.take() {
            changes.push(DialogueStateChange::Closed {
                content_id: active.content_id,
                player_index: active.player_index,
                completed,
            });
        }
    }

    /// Mark the conversation as input-gated when the current node offers
    /// choices (choices always wait; they never auto-advance).
    fn refresh_gate(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        changes: &mut Vec<DialogueStateChange>,
    ) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let Some(graph) = catalog.get(&active.content_id) else {
            return;
        };
        let has_choices = active
            .cursor
            .node(graph)
            .is_some_and(|node| !node.choices.is_empty());
        if has_choices && !active.waiting_for_input {
            changes.push(DialogueStateChange::AwaitingInput {
                content_id: active.content_id.clone(),
                player_index: active.player_index,
            });
        }
    }

    /// Linear nodes with every cue fired and the hold elapsed advance without
    /// input, keeping cutscene-mode pacing authored rather than UI-driven.
    fn maybe_auto_advance(
        &mut self,
        catalog: &PublishedDialogueCatalog,
        changes: &mut Vec<DialogueStateChange>,
    ) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if active.waiting_for_input {
            return;
        }
        let Some(graph) = catalog.get(&active.content_id) else {
            return;
        };
        let Some(node) = active.cursor.node(graph) else {
            return;
        };
        if !node.choices.is_empty() {
            return;
        }
        let all_cues_fired = node
            .timeline
            .iter()
            .enumerate()
            .all(|(index, _)| active.fired_cues.contains(&index));
        if !all_cues_fired || active.node_clock < node.minimum_hold_seconds {
            return;
        }
        // Auto-advance only applies to cutscene mode; gameplay conversations
        // always let the player set the pace.
        if graph.mode != crate::engine_tools::dialogue_records::DialoguePlaybackMode::Cutscene {
            return;
        }
        let player_index = active.player_index;
        self.advance(catalog, player_index, changes);
    }
}

/// Advances the active conversation's node clock, fires due cues, and applies
/// cutscene auto-advance. Registered in `Update` by the world plugin; cue and
/// presentation subscribers read the emitted [`DialogueStateChange`] messages.
pub fn dialogue_director_tick_system(
    time: Res<Time>,
    catalog: Res<PublishedDialogueCatalog>,
    mut director: ResMut<DialogueDirector>,
    mut changes: MessageWriter<DialogueStateChange>,
) {
    let mut batch = Vec::new();
    director.tick(&catalog, time.delta_secs(), &mut batch);
    for change in batch {
        changes.write(change);
    }
}

/// The owning player cancelled. Convenience wrapper so UI code does not
/// repeat the ownership check.
pub fn cancel(
    director: &mut DialogueDirector,
    player_index: u8,
    changes: &mut Vec<DialogueStateChange>,
) -> bool {
    if director.owner() != Some(player_index) {
        return false;
    }
    director.close(changes, false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_tools::dialogue_records::{
        DialogueChoice, DialogueNode, DialoguePlaybackMode, TimelineAction, TimelineCue,
    };

    fn catalog_with(graph: DialogueGraph) -> (PublishedDialogueCatalog, String) {
        let mut catalog = PublishedDialogueCatalog::default();
        let content_id = "starfall.dialogue.test".to_string();
        let seeded = catalog.seed_from_published([(content_id.clone(), graph)]);
        assert_eq!(seeded, 1, "test catalog must accept the graph");
        (catalog, content_id)
    }

    fn linear_graph() -> DialogueGraph {
        let mut graph = DialogueGraph::default();
        graph.nodes[0].next_node = Some("second".into());
        graph
            .nodes
            .push(DialogueNode::speech("second", "Narrator", "The end."));
        graph
    }

    fn branching_graph() -> DialogueGraph {
        let mut graph = DialogueGraph::default();
        graph.nodes[0].choices = vec![
            DialogueChoice {
                label: "Fight".into(),
                target_node: "fight".into(),
                required_flag: None,
                set_flag: Some("chose_fight".into()),
            },
            DialogueChoice {
                label: "Secret path".into(),
                target_node: "secret".into(),
                required_flag: Some("knows_secret".into()),
                set_flag: None,
            },
        ];
        graph
            .nodes
            .push(DialogueNode::speech("fight", "Narrator", "Steel it is."));
        graph
            .nodes
            .push(DialogueNode::speech("secret", "Narrator", "A hidden door."));
        graph
    }

    #[test]
    fn start_enters_the_entry_node_and_blocks_a_second_conversation() {
        let (catalog, id) = catalog_with(linear_graph());
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();

        assert!(director.start(&catalog, &id, 1, &mut changes));
        assert!(director.is_active());
        assert_eq!(director.owner(), Some(1));
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "opening");
        assert!(matches!(
            changes.as_slice(),
            [
                DialogueStateChange::Started { .. },
                DialogueStateChange::NodeEntered { .. }
            ]
        ));

        // One channel: a second start is refused without disturbing the first.
        assert!(!director.start(&catalog, &id, 2, &mut changes));
        assert_eq!(director.owner(), Some(1));

        // Unknown or broken graphs never activate.
        let mut other = DialogueDirector::default();
        assert!(!other.start(&catalog, "starfall.dialogue.missing", 0, &mut changes));
        assert!(!other.is_active());
    }

    #[test]
    fn advance_walks_linear_nodes_and_completes() {
        let (catalog, id) = catalog_with(linear_graph());
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));
        changes.clear();

        // Minimum hold gates advancing.
        assert!(!director.advance(&catalog, 0, &mut changes));
        director.tick(&catalog, 0.5, &mut changes);
        assert!(director.advance(&catalog, 0, &mut changes));
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "second");

        director.tick(&catalog, 0.5, &mut changes);
        assert!(director.advance(&catalog, 0, &mut changes));
        assert!(!director.is_active());
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::Closed {
                completed: true,
                ..
            }
        )));
    }

    #[test]
    fn only_the_owner_may_advance_choose_or_cancel() {
        let (catalog, id) = catalog_with(branching_graph());
        let mut director = DialogueDirector::default();
        let mut flags = DialogueFlags::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 2, &mut changes));

        assert!(!director.choose(&catalog, &mut flags, 1, 0, &mut changes));
        assert!(!cancel(&mut director, 1, &mut changes));
        assert!(director.is_active());

        assert!(cancel(&mut director, 2, &mut changes));
        assert!(!director.is_active());
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::Closed {
                completed: false,
                player_index: 2,
                ..
            }
        )));
    }

    #[test]
    fn choices_respect_required_flags_and_apply_set_flags() {
        let (catalog, id) = catalog_with(branching_graph());
        let mut director = DialogueDirector::default();
        let mut flags = DialogueFlags::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));
        changes.clear();

        // Without the flag, only the first choice is listed.
        let choices = director.available_choices(&catalog, &flags);
        assert_eq!(choices, vec![(0, "Fight")]);

        // The gated choice refuses direct selection.
        assert!(!director.choose(&catalog, &mut flags, 0, 1, &mut changes));

        // Taking "Fight" sets its flag and moves the cursor.
        assert!(director.choose(&catalog, &mut flags, 0, 0, &mut changes));
        assert!(flags.is_set("chose_fight"));
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "fight");
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::NodeEntered { node_id, .. } if node_id == "fight"
        )));
    }

    #[test]
    fn flag_gated_choice_unlocks_once_the_flag_exists() {
        let (catalog, id) = catalog_with(branching_graph());
        let mut director = DialogueDirector::default();
        let mut flags = DialogueFlags::default();
        flags.set("knows_secret");
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));

        let choices = director.available_choices(&catalog, &flags);
        assert_eq!(choices, vec![(0, "Fight"), (1, "Secret path")]);
        assert!(director.choose(&catalog, &mut flags, 0, 1, &mut changes));
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "secret");
    }

    #[test]
    fn cues_fire_once_in_authored_order_as_the_clock_passes_them() {
        let mut graph = linear_graph();
        graph.nodes[0].timeline = vec![
            TimelineCue {
                at_seconds: 0.1,
                action: TimelineAction::PlayAudio {
                    asset_path: "voice/a.mp3".into(),
                    volume: 1.0,
                    spatial_actor: None,
                },
            },
            TimelineCue {
                at_seconds: 0.3,
                action: TimelineAction::SetActorVisible {
                    actor: "guard".into(),
                    visible: false,
                },
            },
        ];
        let (catalog, id) = catalog_with(graph);
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));
        changes.clear();

        director.tick(&catalog, 0.05, &mut changes);
        assert!(changes.is_empty());

        director.tick(&catalog, 0.1, &mut changes);
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::CueFired {
                cue_index: 0,
                action: TimelineAction::PlayAudio { .. },
                ..
            }
        )));

        // Re-ticking does not re-fire the earlier cue.
        changes.clear();
        director.tick(&catalog, 0.2, &mut changes);
        let fired: Vec<usize> = changes
            .iter()
            .filter_map(|change| match change {
                DialogueStateChange::CueFired { cue_index, .. } => Some(*cue_index),
                _ => None,
            })
            .collect();
        assert_eq!(fired, vec![1]);
    }

    #[test]
    fn wait_for_input_halts_the_clock_until_the_owner_advances() {
        let mut graph = linear_graph();
        graph.nodes[0].timeline = vec![
            TimelineCue {
                at_seconds: 0.2,
                action: TimelineAction::WaitForInput,
            },
            TimelineCue {
                at_seconds: 0.4,
                action: TimelineAction::PlayAudio {
                    asset_path: "voice/after.mp3".into(),
                    volume: 1.0,
                    spatial_actor: None,
                },
            },
        ];
        let (catalog, id) = catalog_with(graph);
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));
        changes.clear();

        director.tick(&catalog, 1.0, &mut changes);
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::CueFired {
                action: TimelineAction::WaitForInput,
                ..
            }
        )));
        assert!(changes
            .iter()
            .any(|change| matches!(change, DialogueStateChange::AwaitingInput { .. })));
        assert!(director.is_awaiting_input());
        // The later cue is still held behind the wait, even though wall time
        // has passed it: content must never be skipped over an author gate.
        assert!(!changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::CueFired {
                action: TimelineAction::PlayAudio { .. },
                ..
            }
        )));

        // While held, the clock stays pinned at the wait cue's authored time.
        changes.clear();
        director.tick(&catalog, 5.0, &mut changes);
        assert!(!changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::CueFired {
                action: TimelineAction::PlayAudio { .. },
                ..
            }
        )));

        // Advancing releases the wait; the clock resumes from the pin and the
        // pending cue fires once real time carries the node past 0.4s.
        changes.clear();
        assert!(director.advance(&catalog, 0, &mut changes));
        assert!(!director.is_awaiting_input());
        director.tick(&catalog, 0.25, &mut changes);
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::CueFired {
                action: TimelineAction::PlayAudio { .. },
                ..
            }
        )));
    }

    #[test]
    fn cutscene_mode_auto_advances_linear_nodes_after_hold_and_cues() {
        let mut graph = linear_graph();
        graph.mode = DialoguePlaybackMode::Cutscene;
        graph.nodes[0].minimum_hold_seconds = 0.5;
        let (catalog, id) = catalog_with(graph);
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));
        changes.clear();

        director.tick(&catalog, 0.2, &mut changes);
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "opening");

        director.tick(&catalog, 0.4, &mut changes);
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "second");
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::NodeEntered { node_id, .. } if node_id == "second"
        )));
    }

    #[test]
    fn gameplay_mode_never_auto_advances() {
        let mut graph = linear_graph();
        graph.nodes[0].minimum_hold_seconds = 0.1;
        let (catalog, id) = catalog_with(graph);
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));

        for _ in 0..10 {
            director.tick(&catalog, 0.5, &mut changes);
        }
        assert_eq!(director.current_node(&catalog).unwrap().node_id, "opening");
    }

    #[test]
    fn a_graph_that_vanishes_from_the_catalog_closes_safely() {
        let (catalog, id) = catalog_with(linear_graph());
        let mut director = DialogueDirector::default();
        let mut changes = Vec::new();
        assert!(director.start(&catalog, &id, 0, &mut changes));

        // Simulate a live republish replacing the catalog.
        let empty = PublishedDialogueCatalog::default();
        changes.clear();
        director.tick(&empty, 0.1, &mut changes);
        assert!(!director.is_active());
        assert!(changes.iter().any(|change| matches!(
            change,
            DialogueStateChange::Closed {
                completed: false,
                ..
            }
        )));
    }

    #[test]
    fn flags_resource_stores_and_clears_named_flags() {
        let mut flags = DialogueFlags::default();
        assert!(!flags.is_set("met_captain"));
        flags.set("met_captain");
        assert!(flags.is_set("met_captain"));
        flags.clear("met_captain");
        assert!(!flags.is_set("met_captain"));
    }
}
