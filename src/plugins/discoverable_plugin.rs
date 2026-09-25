//! Discoverable pickup plugin — collects beacons placed by the chapter director,
//! applies their effect (blueprint, mod, companion recruit, beam-sabre unlock).

use bevy::prelude::*;

use crate::combat::damage::Health;
use crate::combat::upgrades::UpgradeLedger;
use crate::components::armor::ArmorSet;
use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleNode,
    PuzzleRelicEncounter, RelicFragmentObstacle, RelicFragmentPuzzlePiece,
};
use crate::components::mods::{ArmorMod, PlayerLoadout, WeaponMod};
use crate::components::player::{
    JetpackState, Player, PlayerBaseStats, PlayerIndex, PlayerMovement, PlayerProgression,
    PlayerStats,
};
use crate::components::weapon::{
    BeamSabre, BeamSabreLocked, MeleeCombo, SpecialWeaponInventory, WeaponInventory,
};
use crate::engine::state::AppState;
use crate::events::*;
use crate::plugins::player_plugin::apply_ancient_flight_core;
use crate::resources::{ChapterProgress, CurrentChapter};
use crate::world::robot_pets::{RobotPetBlueprint, RobotPetCollection};

pub struct DiscoverablePlugin;

impl Plugin for DiscoverablePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::world::puzzles::HeavyWaterPuzzlePlugin)
            .init_resource::<PlayerLoadout>()
            .add_systems(OnEnter(AppState::MainMenu), cleanup_discoverables_for_menu)
            .add_systems(
                OnEnter(AppState::SwitchingMode),
                cleanup_discoverables_for_menu,
            )
            .add_systems(
                Update,
                (
                    beacon_bob_system,
                    fragment_obstacle_system,
                    discoverable_pickup_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn cleanup_discoverables_for_menu(
    mut commands: Commands,
    discoverable_q: Query<Entity, With<Discoverable>>,
    node_q: Query<Entity, With<PuzzleNode>>,
    encounter_q: Query<Entity, With<PuzzleRelicEncounter>>,
    obstacle_q: Query<Entity, With<RelicFragmentObstacle>>,
    fragment_q: Query<Entity, With<RelicFragmentPuzzlePiece>>,
) {
    for entity in discoverable_q
        .iter()
        .chain(node_q.iter())
        .chain(encounter_q.iter())
        .chain(obstacle_q.iter())
        .chain(fragment_q.iter())
    {
        commands.entity(entity).despawn();
    }
}

fn beacon_bob_system(time: Res<Time>, mut q: Query<(&mut Transform, &mut Discoverable)>) {
    let dt = time.delta_secs();
    for (mut t, mut d) in q.iter_mut() {
        d.bob_phase += dt * 2.5;
        let bob = d.bob_phase.sin() * 0.25;
        t.translation.y = t.translation.y * 0.99 + (d.base_y + bob) * 0.01;
        t.rotation = Quat::from_rotation_y(d.bob_phase);
    }
}

fn fragment_obstacle_system(
    time: Res<Time>,
    mut q: Query<(&RelicFragmentObstacle, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (obstacle, mut transform) in q.iter_mut() {
        let wave = (elapsed * obstacle.speed + obstacle.phase).sin();
        transform.translation = obstacle.base + obstacle.travel * wave;
        if obstacle.spin_speed.abs() > f32::EPSILON {
            transform.rotation =
                Quat::from_rotation_y(elapsed * obstacle.spin_speed + obstacle.phase);
        }
    }
}

fn grant_reward_stats(
    stats: &mut PlayerStats,
    base_stats: &mut PlayerBaseStats,
    health: &mut Health,
    armor_set: &ArmorSet,
    progression: &PlayerProgression,
    special_weapons: &mut SpecialWeaponInventory,
    credits: u32,
    experience: u32,
    armor: u32,
    special_ability: Option<&str>,
) -> Option<&'static str> {
    stats.credits = stats.credits.saturating_add(credits);
    stats.experience = stats.experience.saturating_add(experience);
    if armor > 0 {
        base_stats.max_armor += armor as f32 * 0.5;
        let caps = crate::plugins::armor_plugin::current_derived_caps(
            *base_stats,
            stats,
            armor_set,
            progression,
        );
        crate::plugins::armor_plugin::apply_derived_caps(stats, health, caps);
        stats.armor = (stats.armor + armor as f32).min(stats.max_armor);
        health.heal((armor as f32 * 0.25).max(2.0));
    }

    match special_ability {
        Some("homing_star_overdrive") => {
            special_weapons.slot7.level = special_weapons.slot7.level.max(2);
            special_weapons.slot7.max_ammo = special_weapons.slot7.max_ammo.max(12);
            special_weapons.slot7.ammo = special_weapons.slot7.max_ammo;
            Some("Homing Star Overdrive")
        }
        Some("tri_star_splitter") => {
            special_weapons.slot8.level = special_weapons.slot8.level.max(2);
            special_weapons.slot8.max_ammo = special_weapons.slot8.max_ammo.max(18);
            special_weapons.slot8.ammo = special_weapons.slot8.max_ammo;
            Some("Tri-Star Splitter")
        }
        Some("moon_bubble_overcharge") => {
            special_weapons.slot9.level = special_weapons.slot9.level.max(2);
            special_weapons.slot9.max_ammo = special_weapons.slot9.max_ammo.max(7);
            special_weapons.slot9.ammo = special_weapons.slot9.max_ammo;
            Some("Moon Bubble Overcharge")
        }
        Some("sprite_turret_pack") => {
            special_weapons.slot0.level = special_weapons.slot0.level.max(2);
            special_weapons.slot0.max_ammo = special_weapons.slot0.max_ammo.max(4);
            special_weapons.slot0.ammo = special_weapons.slot0.max_ammo;
            Some("Sprite Turret Pack")
        }
        Some("ancient_flight_core") => Some("Ancient Flight Core"),
        Some("solar_sabre_glyph") => Some("Solar Sabre Glyph"),
        Some("nova_missile_matrix") => Some("Nova Missile Matrix"),
        Some("aegis_armor_frame") => Some("Aegis Armor Frame"),
        Some(_) | None => None,
    }
}

fn grant_scientist_temple_ability(
    ability_id: Option<&str>,
    movement: &mut PlayerMovement,
    jetpack: &mut JetpackState,
    weapons: &mut WeaponInventory,
    specials: &mut SpecialWeaponInventory,
    melee: &mut MeleeCombo,
) {
    match ability_id {
        Some("ancient_flight_core") => {
            apply_ancient_flight_core(movement, jetpack);
        }
        Some("solar_sabre_glyph") => {
            melee.damage_multiplier = melee.damage_multiplier.max(1.18);
            weapons.slots[4].damage = weapons.slots[4].damage.max(34.0);
            weapons.slots[4].max_ammo = weapons.slots[4].max_ammo.max(260);
            weapons.slots[4].ammo = weapons.slots[4].max_ammo;
            specials.slot8.level = specials.slot8.level.max(2);
            specials.slot8.max_ammo = specials.slot8.max_ammo.max(20);
            specials.slot8.ammo = specials.slot8.max_ammo;
        }
        Some("nova_missile_matrix") => {
            weapons.slots[3].damage = weapons.slots[3].damage.max(116.0);
            weapons.slots[3].explosion_radius = weapons.slots[3].explosion_radius.max(8.8);
            weapons.slots[3].max_ammo = weapons.slots[3].max_ammo.max(14);
            weapons.slots[3].ammo = weapons.slots[3].max_ammo;
            specials.slot7.level = specials.slot7.level.max(2);
            specials.slot7.max_ammo = specials.slot7.max_ammo.max(12);
            specials.slot7.ammo = specials.slot7.max_ammo;
        }
        Some("aegis_armor_frame") => {
            movement.ground_snap_distance = movement.ground_snap_distance.max(0.34);
            movement.autostep_height = movement.autostep_height.max(0.52);
            movement.max_wall_jump_charges = movement.max_wall_jump_charges.max(3);
            movement.wall_jump_charges = movement
                .wall_jump_charges
                .max(movement.max_wall_jump_charges);
        }
        Some(_) | None => {}
    }
}

fn friend_rescue_reward(name: &str) -> (u32, u32, u32) {
    match name {
        "Vincenzo" | "Antonio" | "Angelo" | "Joseph" => (120, 60, 8),
        "Gabriella" | "Nova" | "Aurora" | "Fortuna" => (90, 45, 6),
        "Pink Flame" => (160, 80, 10),
        _ => (70, 35, 5),
    }
}

#[allow(clippy::too_many_arguments)]
fn discoverable_pickup_system(
    mut commands: Commands,
    player_q: Query<(Entity, &PlayerIndex, &Transform), With<Player>>,
    disc_q: Query<(Entity, &Transform, &Discoverable)>,
    encounter_q: Query<(Entity, &PuzzleRelicEncounter)>,
    fragment_piece_q: Query<(Entity, &RelicFragmentPuzzlePiece)>,
    mut beam_q: Query<&mut BeamSabre>,
    mut reward_player_q: Query<
        (
            &mut PlayerStats,
            &mut PlayerBaseStats,
            &mut Health,
            &mut SpecialWeaponInventory,
            &mut PlayerMovement,
            &mut JetpackState,
            &mut WeaponInventory,
            &mut MeleeCombo,
            &mut PlayerProgression,
            &ArmorSet,
        ),
        With<Player>,
    >,
    mut progress: ResMut<ChapterProgress>,
    mut current: ResMut<CurrentChapter>,
    mut loadout: ResMut<PlayerLoadout>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut radio_ev: MessageWriter<RadioChatterEvent>,
    mut disc_ev: MessageWriter<DiscoverableCollectedEvent>,
    mut companion_ev: MessageWriter<CompanionRecruitedEvent>,
) {
    for (e, t, d) in disc_q.iter() {
        let Some((player_entity, player_index, _)) =
            player_q.iter().find(|(_, _, player_transform)| {
                player_transform.translation.distance(t.translation) <= 2.5
            })
        else {
            continue;
        };
        match &d.kind {
            DiscoverableKind::Blueprint(id) => {
                loadout.add_blueprint(*id);
                progress.unlock(id);
                msg_ev.write(UiMessageEvent {
                    text: format!("Blueprint acquired: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::WeaponMod(id) => {
                let m = match *id {
                    "homing_star" => WeaponMod::homing_star(),
                    "piercing_rounds" => WeaponMod::piercing_rounds(),
                    _ => WeaponMod::piercing_rounds(),
                };
                loadout.equip_weapon_mod(crate::components::weapon::WeaponType::Rifle, m);
                progress.unlock(id);
                msg_ev.write(UiMessageEvent {
                    text: format!("Weapon mod: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::ArmorMod(id) => {
                let m = match *id {
                    "reactive_plating" => ArmorMod::reactive_plating(),
                    "coolant_weave" => ArmorMod::coolant_weave(),
                    _ => ArmorMod::reactive_plating(),
                };
                loadout.add_armor_mod(m);
                progress.unlock(id);
                msg_ev.write(UiMessageEvent {
                    text: format!("Armor mod: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::CompanionRecruit(name) => {
                let was_new = !progress
                    .companions_recruited
                    .iter()
                    .any(|recruited| recruited.as_str() == *name);
                progress.recruit(name);
                let (credits, experience, armor) = friend_rescue_reward(name);
                if was_new {
                    if let Ok((
                        mut stats,
                        mut base_stats,
                        mut health,
                        mut special_weapons,
                        ..,
                        progression,
                        armor_set,
                    )) = reward_player_q.get_mut(player_entity)
                    {
                        grant_reward_stats(
                            &mut stats,
                            &mut base_stats,
                            &mut health,
                            armor_set,
                            &progression,
                            &mut special_weapons,
                            credits,
                            experience,
                            armor,
                            None,
                        );
                    }
                }
                companion_ev.write(CompanionRecruitedEvent {
                    name: (*name).into(),
                    player_index: player_index.0,
                });
                msg_ev.write(UiMessageEvent {
                    text: if was_new {
                        format!(
                            "Friend rescued: {} (+{} credits, +{} XP, +{} armor)",
                            name, credits, experience, armor
                        )
                    } else {
                        format!("Friend already rescued: {}", name)
                    },
                    duration: 3.5,
                });
                radio_ev.write(RadioChatterEvent {
                    speaker: (*name).into(),
                    text: if was_new {
                        format!(
                            "{} stands with you. I found supplies in the enemy zone.",
                            name
                        )
                    } else {
                        format!("{} is already with the team.", name)
                    },
                    faction: crate::components::faction::Faction::HeroBrother,
                    duration: 3.0,
                });
            }
            DiscoverableKind::RobotPetRescue { pet_id, name, role } => {
                let was_new =
                    robot_pets.rescue_pet(RobotPetBlueprint::rescued(*pet_id, *name, *role));
                progress.unlock(pet_id);
                msg_ev.write(UiMessageEvent {
                    text: if was_new {
                        format!("Robot pet rescued: {} ({:?})", name, role)
                    } else {
                        format!("Robot pet already rescued: {}", name)
                    },
                    duration: 3.5,
                });
                radio_ev.write(RadioChatterEvent {
                    speaker: "Giacoma".into(),
                    text: if was_new {
                        format!(
                            "{} is synced to the garage. Its chassis can help with vehicle and mech assemblies.",
                            name
                        )
                    } else {
                        format!("{} is already in the robot pet roster.", name)
                    },
                    faction: crate::components::faction::Faction::WizardScientist,
                    duration: 4.0,
                });
            }
            DiscoverableKind::BeamSabreUnlock => {
                if let Ok(mut beam) = beam_q.single_mut() {
                    beam.unlocked = true;
                }
                commands.entity(player_entity).remove::<BeamSabreLocked>();
                progress.unlock("star_sabre");
                msg_ev.write(UiMessageEvent {
                    text: "Star Sabre online - press T".into(),
                    duration: 4.0,
                });
            }
            DiscoverableKind::ScientistRelic {
                scientist,
                relic_id,
            } => {
                progress.recover_relic(scientist, relic_id);
                progress.unlock(relic_id);
                current.awaiting_puzzle = false;
                msg_ev.write(UiMessageEvent {
                    text: format!("Recovered relic: {}", d.label),
                    duration: 4.0,
                });
                radio_ev.write(RadioChatterEvent {
                    speaker: (*scientist).into(),
                    text: format!(
                        "The {} is back in our hands. One more stolen treasure reclaimed.",
                        d.label
                    ),
                    faction: crate::components::faction::Faction::WizardScientist,
                    duration: 4.0,
                });
                for (encounter_entity, encounter) in encounter_q.iter() {
                    if encounter.scientist == *scientist && encounter.relic_id == *relic_id {
                        commands.entity(encounter_entity).despawn();
                    }
                }
            }
            DiscoverableKind::RelicFragment {
                scientist,
                relic_id,
                piece,
                total,
            } => {
                let was_new = progress.recover_relic_fragment(scientist, relic_id, *piece);
                let recovered = progress.relic_fragment_count(scientist, relic_id);
                if recovered >= *total as usize {
                    progress.recover_relic(scientist, relic_id);
                    progress.unlock(relic_id);
                    current.awaiting_puzzle = false;
                    msg_ev.write(UiMessageEvent {
                        text: format!("Assembled relic: {} ({}/{})", d.label, total, total),
                        duration: 4.5,
                    });
                    radio_ev.write(RadioChatterEvent {
                        speaker: (*scientist).into(),
                        text: format!(
                            "All five fragments of {} are back together. Bring it home.",
                            d.label
                        ),
                        faction: crate::components::faction::Faction::WizardScientist,
                        duration: 4.0,
                    });
                    for (piece_entity, puzzle_piece) in fragment_piece_q.iter() {
                        if puzzle_piece.scientist == *scientist
                            && puzzle_piece.relic_id == *relic_id
                        {
                            commands.entity(piece_entity).despawn();
                        }
                    }
                } else if was_new {
                    msg_ev.write(UiMessageEvent {
                        text: format!("Relic fragment {}/{}: {}", recovered, total, d.label),
                        duration: 3.0,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: format!("Relic fragment already recovered: {}", d.label),
                        duration: 2.5,
                    });
                }
            }
            DiscoverableKind::SecretCave { chapter, cave_id } => {
                let was_new = !progress.has_discoverable(cave_id);
                progress.unlock(cave_id);
                if was_new {
                    msg_ev.write(UiMessageEvent {
                        text: format!("Secret cave discovered: {}", d.label),
                        duration: 4.0,
                    });
                    radio_ev.write(RadioChatterEvent {
                        speaker: "Giacoma".into(),
                        text: format!(
                            "Chapter {} cave charted. Marking {} on the family map.",
                            chapter, d.label
                        ),
                        faction: crate::components::faction::Faction::WizardScientist,
                        duration: 4.0,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: format!("Secret cave already charted: {}", d.label),
                        duration: 2.5,
                    });
                }
            }
            DiscoverableKind::HiddenReward {
                reward_id,
                credits,
                experience,
                armor,
                power_up,
                special_ability,
            } => {
                let was_new = !progress.has_discoverable(reward_id);
                progress.unlock(reward_id);
                let power_up = *power_up;
                let special_ability = *special_ability;
                let mut special_label = None;
                if was_new {
                    if let Some(power_up_id) = power_up {
                        progress.unlock(power_up_id);
                        loadout.add_blueprint(power_up_id);
                    }
                    if let Ok((
                        mut stats,
                        mut base_stats,
                        mut health,
                        mut special_weapons,
                        mut movement,
                        mut jetpack,
                        mut weapons,
                        mut melee,
                        progression,
                        armor_set,
                    )) = reward_player_q.get_mut(player_entity)
                    {
                        special_label = grant_reward_stats(
                            &mut stats,
                            &mut base_stats,
                            &mut health,
                            armor_set,
                            &progression,
                            &mut special_weapons,
                            *credits,
                            *experience,
                            *armor,
                            special_ability,
                        );
                        grant_scientist_temple_ability(
                            special_ability,
                            &mut movement,
                            &mut jetpack,
                            &mut weapons,
                            &mut special_weapons,
                            &mut melee,
                        );
                    }
                    if let Some(ability_id) = special_ability {
                        progress.unlock(ability_id);
                        if crate::combat::upgrades::is_sabre_relic(ability_id) {
                            // World relics are campaign discoveries. Mirror the
                            // unlock into every active player's owned ledger so
                            // local co-op never races for a one-use pickup.
                            for (.., mut progression, _) in reward_player_q.iter_mut() {
                                progression.upgrades.unlock_relic(ability_id);
                            }
                        }
                    }
                }

                let mut gains = Vec::new();
                if *credits > 0 {
                    gains.push(format!("+{} credits", credits));
                }
                if *experience > 0 {
                    gains.push(format!("+{} XP", experience));
                }
                if *armor > 0 {
                    gains.push(format!("+{} armor", armor));
                }
                if let Some(power_up_id) = power_up {
                    gains.push(format!("power-up {}", power_up_id.replace('_', " ")));
                }
                if let Some(label) = special_label {
                    gains.push(format!("ability {}", label));
                }
                msg_ev.write(UiMessageEvent {
                    text: if was_new {
                        format!("Hidden cache: {} ({})", d.label, gains.join(", "))
                    } else {
                        format!("Hidden cache already opened: {}", d.label)
                    },
                    duration: 4.0,
                });
                if was_new {
                    radio_ev.write(RadioChatterEvent {
                        speaker: "Giacoma".into(),
                        text: format!("Secret room cleared. Logged reward cache {}.", d.label),
                        faction: crate::components::faction::Faction::WizardScientist,
                        duration: 3.5,
                    });
                }
            }
            DiscoverableKind::SpyData {
                data_id,
                credits,
                experience,
                armor,
            } => {
                let was_new = !progress.has_discoverable(data_id);
                progress.unlock(data_id);
                if was_new {
                    if let Ok((
                        mut stats,
                        mut base_stats,
                        mut health,
                        mut special_weapons,
                        ..,
                        progression,
                        armor_set,
                    )) = reward_player_q.get_mut(player_entity)
                    {
                        grant_reward_stats(
                            &mut stats,
                            &mut base_stats,
                            &mut health,
                            armor_set,
                            &progression,
                            &mut special_weapons,
                            *credits,
                            *experience,
                            *armor,
                            None,
                        );
                    }
                }

                msg_ev.write(UiMessageEvent {
                    text: if was_new {
                        format!(
                            "Spy data recovered: {} (+{} credits, +{} XP, +{} armor)",
                            d.label, credits, experience, armor
                        )
                    } else {
                        format!("Spy data already recovered: {}", d.label)
                    },
                    duration: 4.0,
                });
                radio_ev.write(RadioChatterEvent {
                    speaker: "Giacoma".into(),
                    text: if was_new {
                        format!(
                            "{} decoded. The Free Peoples can trace this Scallarian signal now.",
                            d.label
                        )
                    } else {
                        format!("{} was already sent to the city watch.", d.label)
                    },
                    faction: crate::components::faction::Faction::WizardScientist,
                    duration: 4.0,
                });
            }
            DiscoverableKind::TechCache {
                cache_id,
                parts,
                upgrade_hint,
                rejuvenation_charge,
            } => {
                let was_new = !progress.has_discoverable(cache_id);
                progress.unlock(cache_id);

                let mut gains = Vec::new();
                if was_new {
                    for (kind, quantity) in parts {
                        robot_pets.add_part(*kind, *quantity);
                        if *quantity > 0 {
                            gains.push(format!("+{} {}", quantity, kind.label()));
                        }
                    }
                    if *rejuvenation_charge > 0 {
                        upgrades.rejuvenation_charge += *rejuvenation_charge as f32;
                        gains.push(format!("+{} rejuvenation reserve", rejuvenation_charge));
                    }
                }
                if let Some(upgrade_id) = *upgrade_hint {
                    gains.push(format!("{} route", upgrade_id.def().name));
                }

                msg_ev.write(UiMessageEvent {
                    text: if was_new {
                        format!("Tech cache: {} ({})", d.label, gains.join(", "))
                    } else {
                        format!("Tech cache already opened: {}", d.label)
                    },
                    duration: 4.0,
                });
                radio_ev.write(RadioChatterEvent {
                    speaker: "Gabrio".into(),
                    text: if was_new {
                        format!(
                            "{} is logged for the upgrade bench. Spend the parts when the team is ready.",
                            d.label
                        )
                    } else {
                        format!("{} was already stripped for parts.", d.label)
                    },
                    faction: crate::components::faction::Faction::WizardScientist,
                    duration: 4.0,
                });
            }
            DiscoverableKind::LoreFragment(text) => {
                msg_ev.write(UiMessageEvent {
                    text: format!("LORE: {}", text),
                    duration: 5.0,
                });
            }
        }
        disc_ev.write(DiscoverableCollectedEvent {
            kind_label: d.label.into(),
            raw_id: format!("{:?}", d.kind),
        });
        commands.entity(e).despawn();
    }
}
