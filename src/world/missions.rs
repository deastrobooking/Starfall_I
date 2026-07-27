use bevy::prelude::*;
use std::sync::OnceLock;

use crate::chapters::{all_chapters, ChapterId, SECRET_CAVE_LOCATIONS};
use crate::components::player::{Player, PlayerStats};
use crate::components::world::WorldAnchor;
use crate::engine::state::AppState;
use crate::events::UiMessageEvent;
use crate::resources::{ChapterProgress, DungeonRoomState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomMissionArea {
    Chapter,
    Dungeon,
    DragonBoss,
    Castle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomMissionObjective {
    CompleteChapter(ChapterId),
    ClearDungeon(&'static str),
    ReachAnchor(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomMissionReward {
    pub credits: u32,
    pub experience: u32,
    pub armor: u32,
}

#[derive(Debug, Clone)]
pub struct CustomMissionDef {
    pub id: String,
    pub title: String,
    pub briefing: String,
    pub chapter: ChapterId,
    pub area: CustomMissionArea,
    pub travel_anchor: Option<&'static str>,
    pub travel_label: Option<&'static str>,
    pub enter_dungeon: bool,
    pub objective: CustomMissionObjective,
    pub reward: CustomMissionReward,
}

impl CustomMissionDef {
    pub fn completion_key(&self) -> String {
        format!("custom_mission:{}", self.id)
    }

    pub fn objective_text(&self) -> String {
        match self.objective {
            CustomMissionObjective::CompleteChapter(chapter) => {
                format!("Complete Chapter {:02}", chapter.0)
            }
            CustomMissionObjective::ClearDungeon(_) => {
                "Clear the final encounter chamber".to_string()
            }
            CustomMissionObjective::ReachAnchor(_) => "Reach and scout the castle gate".to_string(),
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CustomMissionState {
    /// Deliberately session-local. Saving records first-clear completion keys,
    /// while loading returns players to mission selection instead of restoring
    /// a potentially stale world-space objective.
    pub active_id: Option<String>,
}

impl CustomMissionState {
    pub fn activate(&mut self, mission_id: impl Into<String>) {
        self.active_id = Some(mission_id.into());
    }

    pub fn clear(&mut self) {
        self.active_id = None;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpecialMissionTravelPoint {
    pub chapter: ChapterId,
    pub anchor_id: &'static str,
    pub label: &'static str,
    pub x: f32,
    pub z: f32,
    pub enter_dungeon: bool,
}

pub const SPECIAL_MISSION_TRAVEL_POINTS: [SpecialMissionTravelPoint; 8] = [
    SpecialMissionTravelPoint {
        chapter: ChapterId(6),
        anchor_id: "dragon_dungeon_ch06",
        label: "Collosar's Crown Gate",
        x: -8500.0,
        z: -7800.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(7),
        anchor_id: "dragon_dungeon_ch07",
        label: "Tarack's Ember Gate",
        x: -7600.0,
        z: 3800.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(8),
        anchor_id: "dragon_dungeon_ch08",
        label: "Shread's Fangroot Gate",
        x: -5600.0,
        z: 7800.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(9),
        anchor_id: "dragon_dungeon_ch09",
        label: "Pink Flame Garden Gate",
        x: 6600.0,
        z: 700.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(10),
        anchor_id: "dragon_dungeon_ch10",
        label: "Ragar's Granite Gate",
        x: 8500.0,
        z: 4800.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(11),
        anchor_id: "dragon_dungeon_ch11",
        label: "Blackskull Ice Gate",
        x: 3200.0,
        z: -8200.0,
        enter_dungeon: true,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(3),
        anchor_id: "aurora_castle_gate",
        label: "Aurora Castle",
        x: 490.0,
        z: -40.0,
        enter_dungeon: false,
    },
    SpecialMissionTravelPoint {
        chapter: ChapterId(6),
        anchor_id: "collosar_castle_gate",
        label: "Collosar's Dragon Castle",
        x: -8800.0,
        z: -8020.0,
        enter_dungeon: false,
    },
];

const DRAGON_MISSIONS: [(u8, &str, &str, &str); 6] = [
    (
        6,
        "collosar_crown",
        "Break Collosar's Crown",
        "dragon_dungeon_ch06",
    ),
    (
        7,
        "tarack_ember",
        "Quench Tarack's Ember",
        "dragon_dungeon_ch07",
    ),
    (
        8,
        "shread_fangroot",
        "Ground Shread's Scrapwing",
        "dragon_dungeon_ch08",
    ),
    (
        9,
        "pink_flame",
        "Calm the Pink Flame",
        "dragon_dungeon_ch09",
    ),
    (
        10,
        "ragar_granite",
        "Crack Ragar's Granite Gate",
        "dragon_dungeon_ch10",
    ),
    (
        11,
        "blackskull_ice",
        "Stop Blackskull's Icebreaker",
        "dragon_dungeon_ch11",
    ),
];

pub fn custom_missions() -> &'static [CustomMissionDef] {
    static MISSIONS: OnceLock<Vec<CustomMissionDef>> = OnceLock::new();
    MISSIONS.get_or_init(|| {
        let mut missions = Vec::new();
        for chapter in all_chapters() {
            missions.push(CustomMissionDef {
                id: format!("chapter_{:02}", chapter.id.0),
                title: format!("Chapter {:02}: {}", chapter.id.0, chapter.title),
                briefing: chapter.subtitle.to_string(),
                chapter: chapter.id,
                area: CustomMissionArea::Chapter,
                travel_anchor: None,
                travel_label: None,
                enter_dungeon: false,
                objective: CustomMissionObjective::CompleteChapter(chapter.id),
                reward: CustomMissionReward {
                    credits: 90 + chapter.id.0 as u32 * 8,
                    experience: 70 + chapter.id.0 as u32 * 9,
                    armor: 8 + chapter.id.0 as u32 / 2,
                },
            });
        }
        for cave in SECRET_CAVE_LOCATIONS {
            missions.push(CustomMissionDef {
                id: format!("dungeon_{:02}", cave.chapter.0),
                title: format!("Dungeon {:02}: {}", cave.chapter.0, cave.label),
                briefing: format!("Explore {} and clear its awakened chamber.", cave.label),
                chapter: cave.chapter,
                area: CustomMissionArea::Dungeon,
                travel_anchor: Some(cave.anchor_id),
                travel_label: Some(cave.label),
                enter_dungeon: true,
                objective: CustomMissionObjective::ClearDungeon(cave.anchor_id),
                reward: CustomMissionReward {
                    credits: 125 + cave.chapter.0 as u32 * 7,
                    experience: 105 + cave.chapter.0 as u32 * 8,
                    armor: 12,
                },
            });
        }
        for (chapter, id, title, anchor_id) in DRAGON_MISSIONS {
            let travel = SPECIAL_MISSION_TRAVEL_POINTS
                .iter()
                .find(|point| point.anchor_id == anchor_id)
                .expect("dragon mission travel point");
            missions.push(CustomMissionDef {
                id: format!("dragon_{id}"),
                title: title.to_string(),
                briefing: format!("Enter {} and defeat its final defense.", travel.label),
                chapter: ChapterId(chapter),
                area: CustomMissionArea::DragonBoss,
                travel_anchor: Some(anchor_id),
                travel_label: Some(travel.label),
                enter_dungeon: true,
                objective: CustomMissionObjective::ClearDungeon(anchor_id),
                reward: CustomMissionReward {
                    credits: 260 + chapter as u32 * 10,
                    experience: 220 + chapter as u32 * 12,
                    armor: 20,
                },
            });
        }
        for (id, title, chapter, anchor_id, label) in [
            (
                "castle_aurora",
                "Aurora Castle Welcome",
                ChapterId(3),
                "aurora_castle_gate",
                "Aurora Castle",
            ),
            (
                "castle_collosar",
                "Scout the Dragon King's Castle",
                ChapterId(6),
                "collosar_castle_gate",
                "Collosar's Dragon Castle",
            ),
        ] {
            missions.push(CustomMissionDef {
                id: id.to_string(),
                title: title.to_string(),
                briefing: format!("Travel to {label} and scout its outer gate."),
                chapter,
                area: CustomMissionArea::Castle,
                travel_anchor: Some(anchor_id),
                travel_label: Some(label),
                enter_dungeon: false,
                objective: CustomMissionObjective::ReachAnchor(anchor_id),
                reward: CustomMissionReward {
                    credits: 180,
                    experience: 150,
                    armor: 15,
                },
            });
        }
        missions
    })
}

pub fn chapter_mission(chapter: ChapterId) -> Option<&'static CustomMissionDef> {
    custom_missions()
        .iter()
        .find(|mission| mission.area == CustomMissionArea::Chapter && mission.chapter == chapter)
}

pub fn mission_for_travel_anchor(anchor_id: &str) -> Option<&'static CustomMissionDef> {
    custom_missions()
        .iter()
        .find(|mission| mission.travel_anchor == Some(anchor_id))
}

pub fn active_custom_mission(state: &CustomMissionState) -> Option<&'static CustomMissionDef> {
    let active_id = state.active_id.as_deref()?;
    custom_missions()
        .iter()
        .find(|mission| mission.id == active_id)
}

pub fn dungeon_destination(anchor_id: &str) -> Option<(ChapterId, &'static str)> {
    if let Some(cave) = SECRET_CAVE_LOCATIONS
        .iter()
        .find(|cave| cave.anchor_id == anchor_id)
    {
        let cave = crate::chapters::secret_cave_location(cave.chapter)?;
        return Some((cave.chapter, cave.label));
    }
    let mission = mission_for_travel_anchor(anchor_id)?;
    mission
        .enter_dungeon
        .then_some((mission.chapter, mission.travel_label?))
}

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CustomMissionState>().add_systems(
            Update,
            custom_mission_progress_system.run_if(in_state(AppState::Playing)),
        );
    }
}

fn custom_mission_progress_system(
    mut state: ResMut<CustomMissionState>,
    mut progress: ResMut<ChapterProgress>,
    room_state: Res<DungeonRoomState>,
    anchor_q: Query<(&WorldAnchor, &Transform)>,
    player_positions: Query<&Transform, With<Player>>,
    mut player_stats: Query<&mut PlayerStats, With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let Some(mission) = active_custom_mission(&state) else {
        return;
    };
    let completed = match mission.objective {
        CustomMissionObjective::CompleteChapter(chapter) => progress.is_completed(chapter),
        CustomMissionObjective::ClearDungeon(gate_id) => {
            room_state.cleared_rooms.contains(&(gate_id, 2))
        }
        CustomMissionObjective::ReachAnchor(anchor_id) => {
            let Some((_, anchor_transform)) =
                anchor_q.iter().find(|(anchor, _)| anchor.id == anchor_id)
            else {
                return;
            };
            player_positions
                .iter()
                .any(|player| player.translation.distance(anchor_transform.translation) <= 24.0)
        }
    };
    if !completed {
        return;
    }

    let completion_key = mission.completion_key();
    let first_clear = !progress.has_discoverable(&completion_key);
    progress.unlock(&completion_key);
    if first_clear {
        // Couch co-op policy: a first-clear reward is not divided. Every
        // currently active local player receives the full authored amount so
        // joining the shared mission never penalizes individual progression.
        for mut stats in &mut player_stats {
            stats.credits = stats.credits.saturating_add(mission.reward.credits);
            stats.experience = stats.experience.saturating_add(mission.reward.experience);
            stats.armor = (stats.armor + mission.reward.armor as f32).min(stats.max_armor);
        }
    }
    msg_ev.write(UiMessageEvent {
        text: if first_clear {
            format!(
                "MISSION COMPLETE — {}  +{} credits +{} XP +{} armor",
                mission.title,
                mission.reward.credits,
                mission.reward.experience,
                mission.reward.armor
            )
        } else {
            format!("MISSION COMPLETE — {} (replay)", mission.title)
        },
        duration: 5.5,
    });
    state.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_requested_area_and_uses_unique_ids() {
        let missions = custom_missions();
        assert_eq!(
            missions
                .iter()
                .filter(|mission| mission.area == CustomMissionArea::Chapter)
                .count(),
            14
        );
        assert_eq!(
            missions
                .iter()
                .filter(|mission| mission.area == CustomMissionArea::Dungeon)
                .count(),
            14
        );
        assert_eq!(
            missions
                .iter()
                .filter(|mission| mission.area == CustomMissionArea::DragonBoss)
                .count(),
            6
        );
        assert_eq!(
            missions
                .iter()
                .filter(|mission| mission.area == CustomMissionArea::Castle)
                .count(),
            2
        );
        let mut ids = missions
            .iter()
            .map(|mission| mission.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), missions.len());
    }

    #[test]
    fn every_special_travel_point_resolves_to_a_mission() {
        for point in SPECIAL_MISSION_TRAVEL_POINTS {
            let mission = mission_for_travel_anchor(point.anchor_id).unwrap();
            assert_eq!(mission.chapter, point.chapter);
            assert_eq!(mission.enter_dungeon, point.enter_dungeon);
        }
    }

    #[test]
    fn every_cave_and_dragon_dungeon_is_a_direct_destination() {
        for cave in SECRET_CAVE_LOCATIONS {
            assert_eq!(
                dungeon_destination(cave.anchor_id),
                Some((cave.chapter, cave.label))
            );
        }
        for point in SPECIAL_MISSION_TRAVEL_POINTS
            .into_iter()
            .filter(|point| point.enter_dungeon)
        {
            assert_eq!(
                dungeon_destination(point.anchor_id),
                Some((point.chapter, point.label))
            );
        }
    }

    #[test]
    fn first_clear_rewards_every_active_local_player_equally_and_only_once() {
        let mut app = App::new();
        app.init_resource::<ChapterProgress>();
        app.init_resource::<DungeonRoomState>();
        app.init_resource::<CustomMissionState>();
        app.add_message::<UiMessageEvent>();
        app.add_systems(Update, custom_mission_progress_system);
        app.world_mut()
            .resource_mut::<CustomMissionState>()
            .activate("castle_aurora");
        app.world_mut().spawn((
            WorldAnchor {
                id: "aurora_castle_gate",
            },
            Transform::default(),
        ));
        let players = [3.0, 5.0].map(|x| {
            app.world_mut()
                .spawn((
                    Player,
                    Transform::from_xyz(x, 0.0, 0.0),
                    PlayerStats {
                        armor: 0.0,
                        ..default()
                    },
                ))
                .id()
        });

        app.update();
        for player in players {
            let stats = app.world().get::<PlayerStats>(player).unwrap();
            assert_eq!(stats.credits, 180);
            assert_eq!(stats.experience, 150);
            assert_eq!(stats.armor, 15.0);
        }
        assert!(app
            .world()
            .resource::<ChapterProgress>()
            .has_discoverable("custom_mission:castle_aurora"));

        app.world_mut()
            .resource_mut::<CustomMissionState>()
            .activate("castle_aurora");
        app.update();
        for player in players {
            let stats = app.world().get::<PlayerStats>(player).unwrap();
            assert_eq!(stats.credits, 180);
            assert_eq!(stats.experience, 150);
            assert_eq!(stats.armor, 15.0);
        }
    }
}
