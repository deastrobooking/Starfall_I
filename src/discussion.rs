use bevy::prelude::*;

use crate::chapters::MapSettlementKind;

#[derive(Debug, Clone, Copy)]
pub struct DiscussionLine {
    pub speaker: &'static str,
    pub text: &'static str,
    pub voice_mp3: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct DiscussionScript {
    pub id: &'static str,
    pub title: &'static str,
    pub npc_name: &'static str,
    pub role: &'static str,
    pub lines: &'static [DiscussionLine],
}

#[derive(Resource, Debug, Clone)]
pub struct DiscussionState {
    pub active: bool,
    pub script_id: Option<&'static str>,
    pub line_index: usize,
    pub player_index: u8,
    pub input_cooldown: f32,
    pub voice_token: u64,
}

impl Default for DiscussionState {
    fn default() -> Self {
        Self {
            active: false,
            script_id: None,
            line_index: 0,
            player_index: 0,
            input_cooldown: 0.0,
            voice_token: 0,
        }
    }
}

impl DiscussionState {
    pub fn start(&mut self, script_id: &'static str, player_index: u8) -> bool {
        if discussion_script(script_id).is_none() {
            return false;
        }

        self.active = true;
        self.script_id = Some(script_id);
        self.line_index = 0;
        self.player_index = player_index;
        self.input_cooldown = 0.18;
        self.voice_token = self.voice_token.wrapping_add(1);
        true
    }

    pub fn close(&mut self) {
        self.active = false;
        self.script_id = None;
        self.line_index = 0;
        self.input_cooldown = 0.0;
    }

    pub fn script(&self) -> Option<&'static DiscussionScript> {
        self.script_id.and_then(discussion_script)
    }

    pub fn line(&self) -> Option<&'static DiscussionLine> {
        self.script()
            .and_then(|script| script.lines.get(self.line_index))
    }

    pub fn advance(&mut self) -> bool {
        let Some(script) = self.script() else {
            self.close();
            return false;
        };

        if self.line_index + 1 < script.lines.len() {
            self.line_index += 1;
            self.input_cooldown = 0.16;
            self.voice_token = self.voice_token.wrapping_add(1);
            true
        } else {
            self.close();
            false
        }
    }
}

pub fn settlement_discussion_id(anchor_id: &str) -> &'static str {
    match anchor_id {
        "settlement_cloudrail_city" => "cloudrail_city_captain",
        "settlement_switchwork_borough" => "switchwork_borough_keeper",
        "settlement_riftglass_village" => "riftglass_village_healer",
        "settlement_lantern_hamlet" => "lantern_hamlet_keeper",
        "settlement_star_orchard" => "star_orchard_botanist",
        "settlement_frost_harbor" => "frost_harbor_navigator",
        "settlement_granite_market" => "granite_market_smith",
        "settlement_starfell_outpost" => "starfell_outpost_scout",
        _ => "free_people_wayfarer",
    }
}

pub fn settlement_guardian_role(kind: MapSettlementKind) -> &'static str {
    match kind {
        MapSettlementKind::City => "Free Peoples sky captain",
        MapSettlementKind::Village => "village guardian",
        MapSettlementKind::Harbor => "harbor navigator",
        MapSettlementKind::Outpost => "frontier scout",
    }
}

pub fn discussion_script(id: &str) -> Option<&'static DiscussionScript> {
    ALL_DISCUSSION_SCRIPTS.iter().find(|script| script.id == id)
}

const CLOUDRAIL_CITY_CAPTAIN_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Captain Mira",
        text: "Welcome to Cloudrail City. The towers are peaceful, but our skyships keep watch while the Scallarians hunt the passes.",
        voice_mp3: Some("voice/settlements/cloudrail_city_01.mp3"),
    },
    DiscussionLine {
        speaker: "Captain Mira",
        text: "If a dragon shadow crosses the ridge, the city shields rise first. Children, healers, and travelers get protected before anyone fires.",
        voice_mp3: Some("voice/settlements/cloudrail_city_02.mp3"),
    },
    DiscussionLine {
        speaker: "Captain Mira",
        text: "Use the fast-travel map when you need shelter. The Free Peoples will keep a lane open for the Starfall family.",
        voice_mp3: Some("voice/settlements/cloudrail_city_03.mp3"),
    },
];

const SWITCHWORK_BOROUGH_KEEPER_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Director Sol",
        text: "Switchwork Borough powers the mountain rail and the guardian hangars. We build quietly so the children can sleep loudly.",
        voice_mp3: Some("voice/settlements/switchwork_borough_01.mp3"),
    },
    DiscussionLine {
        speaker: "Director Sol",
        text: "Bring robot parts here later and we can tune pet cores, turret mounts, and vehicle frames without breaking campaign saves.",
        voice_mp3: Some("voice/settlements/switchwork_borough_02.mp3"),
    },
    DiscussionLine {
        speaker: "Director Sol",
        text: "Every good machine has a promise inside it: protect the team, then help the team become braver.",
        voice_mp3: Some("voice/settlements/switchwork_borough_03.mp3"),
    },
];

const RIFTGLASS_VILLAGE_HEALER_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Aunt Renna",
        text: "Riftglass is small, but we watch every trail. Rest here before you chase the dragon lairs.",
        voice_mp3: Some("voice/settlements/riftglass_village_01.mp3"),
    },
    DiscussionLine {
        speaker: "Aunt Renna",
        text: "Regeneration should cost something in the final rules. A hero learns to guard friends, not just refill a bar.",
        voice_mp3: Some("voice/settlements/riftglass_village_02.mp3"),
    },
];

const LANTERN_HAMLET_KEEPER_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Keeper Tashi",
        text: "The lanterns mark safe roads through the snow. Follow their glow when the map gets dark.",
        voice_mp3: Some("voice/settlements/lantern_hamlet_01.mp3"),
    },
    DiscussionLine {
        speaker: "Keeper Tashi",
        text: "Old temples hide full mechanics upgrades. Flight, armor, and weapons should feel earned, not handed out.",
        voice_mp3: Some("voice/settlements/lantern_hamlet_02.mp3"),
    },
];

const STAR_ORCHARD_BOTANIST_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Nari Bloom",
        text: "These trees grow under alien starlight now, but they are still Earth trees. That matters.",
        voice_mp3: Some("voice/settlements/star_orchard_01.mp3"),
    },
    DiscussionLine {
        speaker: "Nari Bloom",
        text: "Robot pets love the orchard paths. Rescue them, build them gently, and they will amplify every sibling's special gift.",
        voice_mp3: Some("voice/settlements/star_orchard_02.mp3"),
    },
];

const FROST_HARBOR_NAVIGATOR_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Pilot Ione",
        text: "Frost Harbor keeps boats, submarines, and rescue craft ready. Water routes are quiet until Blackskull hears them.",
        voice_mp3: Some("voice/settlements/frost_harbor_01.mp3"),
    },
    DiscussionLine {
        speaker: "Pilot Ione",
        text: "One day this dock should launch bikes, tanks, jets, subs, and megaships from parts the team wins in battle.",
        voice_mp3: Some("voice/settlements/frost_harbor_02.mp3"),
    },
];

const GRANITE_MARKET_SMITH_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Smith Halden",
        text: "Granite Market trades in clean armor, beam saber grips, and honest repairs.",
        voice_mp3: Some("voice/settlements/granite_market_01.mp3"),
    },
    DiscussionLine {
        speaker: "Smith Halden",
        text: "Bring scrap from generic bad guys. The store can turn parts into turrets, weapon upgrades, and stronger robot frames.",
        voice_mp3: Some("voice/settlements/granite_market_02.mp3"),
    },
];

const STARFELL_OUTPOST_SCOUT_LINES: &[DiscussionLine] = &[
    DiscussionLine {
        speaker: "Scout Vela",
        text: "Starfell Outpost is the warning bell. When a boss finds one player, the whole party should rally on one screen.",
        voice_mp3: Some("voice/settlements/starfell_outpost_01.mp3"),
    },
    DiscussionLine {
        speaker: "Scout Vela",
        text: "Watch the sky. Drones, ships, and dragon patrols are invitations for everyone to come together.",
        voice_mp3: Some("voice/settlements/starfell_outpost_02.mp3"),
    },
];

const FREE_PEOPLE_WAYFARER_LINES: &[DiscussionLine] = &[DiscussionLine {
    speaker: "Wayfarer",
    text: "Peaceful roads survive because people choose to guard them every day.",
    voice_mp3: Some("voice/settlements/free_people_wayfarer_01.mp3"),
}];

const ALL_DISCUSSION_SCRIPTS: &[DiscussionScript] = &[
    DiscussionScript {
        id: "cloudrail_city_captain",
        title: "Peaceful Mega City",
        npc_name: "Captain Mira",
        role: "Free Peoples sky captain",
        lines: CLOUDRAIL_CITY_CAPTAIN_LINES,
    },
    DiscussionScript {
        id: "switchwork_borough_keeper",
        title: "Guardian Hangars",
        npc_name: "Director Sol",
        role: "city engineer",
        lines: SWITCHWORK_BOROUGH_KEEPER_LINES,
    },
    DiscussionScript {
        id: "riftglass_village_healer",
        title: "Safe Village",
        npc_name: "Aunt Renna",
        role: "village healer",
        lines: RIFTGLASS_VILLAGE_HEALER_LINES,
    },
    DiscussionScript {
        id: "lantern_hamlet_keeper",
        title: "Lantern Road",
        npc_name: "Keeper Tashi",
        role: "snow-road keeper",
        lines: LANTERN_HAMLET_KEEPER_LINES,
    },
    DiscussionScript {
        id: "star_orchard_botanist",
        title: "Earth Orchard",
        npc_name: "Nari Bloom",
        role: "star botanist",
        lines: STAR_ORCHARD_BOTANIST_LINES,
    },
    DiscussionScript {
        id: "frost_harbor_navigator",
        title: "Vehicle Harbor",
        npc_name: "Pilot Ione",
        role: "harbor navigator",
        lines: FROST_HARBOR_NAVIGATOR_LINES,
    },
    DiscussionScript {
        id: "granite_market_smith",
        title: "Upgrade Market",
        npc_name: "Smith Halden",
        role: "armor smith",
        lines: GRANITE_MARKET_SMITH_LINES,
    },
    DiscussionScript {
        id: "starfell_outpost_scout",
        title: "Boss Rally Watch",
        npc_name: "Scout Vela",
        role: "frontier scout",
        lines: STARFELL_OUTPOST_SCOUT_LINES,
    },
    DiscussionScript {
        id: "free_people_wayfarer",
        title: "Free Peoples Road",
        npc_name: "Wayfarer",
        role: "traveler",
        lines: FREE_PEOPLE_WAYFARER_LINES,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chapters::map_settlements;

    #[test]
    fn every_settlement_discussion_id_resolves() {
        for settlement in map_settlements() {
            let script_id = settlement_discussion_id(settlement.anchor_id);
            assert!(
                discussion_script(script_id).is_some(),
                "missing discussion script for {}",
                settlement.name
            );
        }
    }

    #[test]
    fn voiced_lines_use_mp3_asset_paths() {
        for script in ALL_DISCUSSION_SCRIPTS {
            assert!(!script.lines.is_empty(), "{} has no lines", script.id);
            for line in script.lines {
                if let Some(path) = line.voice_mp3 {
                    assert!(path.ends_with(".mp3"), "{path} should be an mp3");
                    assert!(
                        path.starts_with("voice/"),
                        "{path} should live under assets/voice"
                    );
                }
            }
        }
    }

    #[test]
    fn discussion_state_advances_and_closes() {
        let mut state = DiscussionState::default();
        assert!(state.start("riftglass_village_healer", 2));
        assert!(state.active);
        assert_eq!(state.player_index, 2);
        assert_eq!(state.line().unwrap().speaker, "Aunt Renna");
        assert!(state.advance());
        assert!(!state.advance());
        assert!(!state.active);
    }
}
