//! Discoverable pickup plugin — collects beacons placed by the chapter director,
//! applies their effect (blueprint, mod, companion recruit, beam-sabre unlock).

use bevy::prelude::*;

use crate::components::discoverable::{Discoverable, DiscoverableKind};
use crate::components::mods::{ArmorMod, PlayerLoadout, WeaponMod};
use crate::components::player::Player;
use crate::components::weapon::{BeamSabre, BeamSabreLocked};
use crate::events::*;
use crate::resources::ChapterProgress;
use crate::state::AppState;

pub struct DiscoverablePlugin;

impl Plugin for DiscoverablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerLoadout>().add_systems(
            Update,
            (beacon_bob_system, discoverable_pickup_system).run_if(in_state(AppState::Playing)),
        );
    }
}

fn beacon_bob_system(time: Res<Time>, mut q: Query<(&mut Transform, &mut Discoverable)>) {
    let dt = time.delta_secs();
    for (mut t, mut d) in q.iter_mut() {
        d.bob_phase += dt * 2.5;
        let bob = d.bob_phase.sin() * 0.25;
        t.translation.y = t.translation.y * 0.99 + (1.6 + bob) * 0.01;
        t.rotation = Quat::from_rotation_y(d.bob_phase);
    }
}

#[allow(clippy::too_many_arguments)]
fn discoverable_pickup_system(
    mut commands: Commands,
    player_q: Query<(Entity, &Transform), With<Player>>,
    disc_q: Query<(Entity, &Transform, &Discoverable)>,
    mut beam_q: Query<&mut BeamSabre>,
    mut progress: ResMut<ChapterProgress>,
    mut loadout: ResMut<PlayerLoadout>,
    mut msg_ev: EventWriter<UiMessageEvent>,
    mut radio_ev: EventWriter<RadioChatterEvent>,
    mut disc_ev: EventWriter<DiscoverableCollectedEvent>,
    mut companion_ev: EventWriter<CompanionRecruitedEvent>,
) {
    let Ok((player_entity, pt)) = player_q.get_single() else {
        return;
    };
    for (e, t, d) in disc_q.iter() {
        if pt.translation.distance(t.translation) > 2.5 {
            continue;
        }
        match &d.kind {
            DiscoverableKind::Blueprint(id) => {
                loadout.add_blueprint(*id);
                progress.unlock(id);
                msg_ev.send(UiMessageEvent {
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
                msg_ev.send(UiMessageEvent {
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
                msg_ev.send(UiMessageEvent {
                    text: format!("Armor mod: {}", d.label),
                    duration: 3.0,
                });
            }
            DiscoverableKind::CompanionRecruit(name) => {
                progress.recruit(name);
                companion_ev.send(CompanionRecruitedEvent {
                    name: (*name).into(),
                });
                radio_ev.send(RadioChatterEvent {
                    speaker: (*name).into(),
                    text: format!("{} stands with you.", name),
                    faction: crate::components::faction::Faction::HeroBrother,
                    duration: 3.0,
                });
            }
            DiscoverableKind::BeamSabreUnlock => {
                if let Ok(mut beam) = beam_q.get_single_mut() {
                    beam.unlocked = true;
                }
                commands.entity(player_entity).remove::<BeamSabreLocked>();
                progress.unlock("star_sabre");
                msg_ev.send(UiMessageEvent {
                    text: "Star Sabre online - press T".into(),
                    duration: 4.0,
                });
            }
            DiscoverableKind::LoreFragment(text) => {
                msg_ev.send(UiMessageEvent {
                    text: format!("LORE: {}", text),
                    duration: 5.0,
                });
            }
        }
        disc_ev.send(DiscoverableCollectedEvent {
            kind_label: d.label.into(),
            raw_id: format!("{:?}", d.kind),
        });
        commands.entity(e).despawn_recursive();
    }
}
