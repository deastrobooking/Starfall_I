use bevy::prelude::*;

use crate::commands::CommandRegistry;
use crate::components::enemy::{Enemy, EnemyStateMachine};
use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::damage::{apply_damage, DamageInfo, DamageType, Damageable, Health};
use crate::events::{EnemyDamagedEvent, EnemyKilledEvent, UiMessageEvent};
use crate::hacking::{complete_small_drone_hack, Hackable, HackedUnit, HackingRegistry};
use crate::state::AppState;

pub struct HackingPlugin;

impl Plugin for HackingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HackingRegistry>().add_systems(
            Update,
            (
                hack_interaction_system,
                hack_progress_system,
                hacked_unit_control_system,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn hack_interaction_system(
    player_q: Query<(&Transform, &PlayerInput, &PlayerIndex), With<Player>>,
    mut hackable_q: Query<(&Transform, &mut Hackable, Option<&Health>)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (target_xf, mut hackable, health) in hackable_q.iter_mut() {
        if hackable.progress.is_some() {
            continue;
        }
        if health.map(|h| !h.is_alive()).unwrap_or(false) {
            continue;
        }
        let Some((_player_xf, _input, index, distance)) = player_q
            .iter()
            .filter(|(player_xf, input, _)| {
                input.interact
                    && player_xf.translation.distance(target_xf.translation)
                        <= hackable.required_range
            })
            .map(|(player_xf, input, index)| {
                (
                    player_xf,
                    input,
                    index,
                    player_xf.translation.distance(target_xf.translation),
                )
            })
            .min_by(|a, b| a.3.total_cmp(&b.3))
        else {
            continue;
        };

        let remaining = hackable.hack_seconds();
        hackable.progress = Some(crate::hacking::HackProgress {
            player_index: *index,
            remaining,
        });
        msg_ev.write(UiMessageEvent {
            text: format!(
                "P{} started hacking {} ({distance:.0}m). Hold position.",
                index.0 + 1,
                hackable.target_class.label()
            ),
            duration: 2.4,
        });
    }
}

fn hack_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    player_q: Query<(&Transform, &PlayerIndex), With<Player>>,
    mut hackable_q: Query<(
        Entity,
        &Transform,
        &mut Hackable,
        Option<&mut EnemyStateMachine>,
        Option<&Health>,
    )>,
    mut registry: ResMut<HackingRegistry>,
    mut command_registry: ResMut<CommandRegistry>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (entity, target_xf, mut hackable, state_machine, health) in hackable_q.iter_mut() {
        let Some(progress) = hackable.progress else {
            continue;
        };
        if health.map(|h| !h.is_alive()).unwrap_or(false) {
            hackable.progress = None;
            continue;
        }
        let Some((player_xf, _)) = player_q
            .iter()
            .find(|(_, index)| **index == progress.player_index)
        else {
            hackable.progress = None;
            continue;
        };
        if player_xf.translation.distance(target_xf.translation) > hackable.required_range + 3.0 {
            let player = progress.player_index.0 + 1;
            hackable.progress = None;
            msg_ev.write(UiMessageEvent {
                text: format!("P{player} lost hack link."),
                duration: 1.6,
            });
            continue;
        }

        let remaining = (progress.remaining - dt).max(0.0);
        if remaining > 0.0 {
            if let Some(active) = hackable.progress.as_mut() {
                active.remaining = remaining;
            }
            continue;
        }

        let owner = progress.player_index;
        let target_label = hackable.target_class.label();
        let hacked = complete_small_drone_hack(
            &mut registry,
            &mut command_registry,
            owner,
            &hackable,
        );
        if let Some(mut sm) = state_machine {
            sm.force(crate::components::enemy::EnemyAIState::Stunned);
        }
        commands
            .entity(entity)
            .insert(hacked)
            .remove::<Hackable>()
            .remove::<crate::raids::RaidThreatMarker>();
        msg_ev.write(UiMessageEvent {
            text: format!(
                "P{} hacked {}: blueprint saved and drone linked.",
                owner.0 + 1,
                target_label
            ),
            duration: 4.0,
        });
    }
}

fn hacked_unit_control_system(
    mut commands: Commands,
    time: Res<Time>,
    player_q: Query<(&Transform, &PlayerIndex, &PlayerInput), With<Player>>,
    mut hacked_q: Query<(Entity, &mut Transform, &mut HackedUnit), Without<Player>>,
    mut target_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        (Without<Player>, Without<HackedUnit>),
    >,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut hacked) in hacked_q.iter_mut() {
        hacked.timer = (hacked.timer - dt).max(0.0);
        hacked.ability_cooldown = (hacked.ability_cooldown - dt).max(0.0);
        if let Some((player_xf, _, input)) = player_q
            .iter()
            .find(|(_, index, _)| **index == hacked.owner)
        {
            let phase = (time.elapsed_secs() * 1.8 + f32::from(hacked.owner.0)).sin();
            let desired = player_xf.translation
                + Vec3::new(4.0 + f32::from(hacked.owner.0), 4.2 + phase * 0.5, -5.0);
            transform.translation = transform.translation.lerp(desired, 1.0 - (-dt * 4.0).exp());
            if transform.translation.distance_squared(player_xf.translation) > 0.1 {
                transform.look_at(player_xf.translation + Vec3::Y * 1.0, Vec3::Y);
            }
            if hacked.ability_cooldown <= 0.0
                && (input.fire_just || input.melee_light || input.melee_heavy)
            {
                if hacked_drone_pulse(
                    entity,
                    &mut transform,
                    &mut target_q,
                    &mut damaged_ev,
                    &mut killed_ev,
                ) {
                    hacked.ability_cooldown = 0.65;
                }
            }
        }

        if hacked.timer > 0.0 {
            continue;
        }
        commands.entity(entity).try_despawn();
        msg_ev.write(UiMessageEvent {
            text: format!("Hacked {} powered down safely.", hacked.source.label()),
            duration: 2.0,
        });
    }
}

fn hacked_drone_pulse(
    source: Entity,
    source_transform: &mut Transform,
    target_q: &mut Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        (Without<Player>, Without<HackedUnit>),
    >,
    damaged_ev: &mut MessageWriter<EnemyDamagedEvent>,
    killed_ev: &mut MessageWriter<EnemyKilledEvent>,
) -> bool {
    let origin = source_transform.translation;
    let mut closest: Option<(Entity, f32)> = None;
    for (entity, target_xf, health, _, _) in target_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let distance = origin.distance(target_xf.translation);
        if distance > 36.0 {
            continue;
        }
        if closest.map(|(_, best)| distance < best).unwrap_or(true) {
            closest = Some((entity, distance));
        }
    }

    let Some((target, _)) = closest else {
        return false;
    };
    let Ok((_, target_xf, mut health, mut damageable, enemy)) = target_q.get_mut(target) else {
        return false;
    };
    let hit_point = target_xf.translation + Vec3::Y * 0.8;
    let mut info = DamageInfo::new(18.0, DamageType::Laser).with_knockback(2.0);
    info.attacker = Some(source);
    info.hit_point = Some(hit_point);
    info.hit_direction = Some((hit_point - origin).normalize_or_zero());
    let result = apply_damage(&mut health, &mut damageable, &info);
    source_transform.look_at(hit_point, Vec3::Y);
    damaged_ev.write(EnemyDamagedEvent {
        entity: target,
        damage: result.damage_amount,
        position: hit_point,
    });
    if result.was_killed {
        killed_ev.write(EnemyKilledEvent {
            enemy_type: enemy.enemy_type.as_str().to_string(),
            credits: enemy.config.credits,
            experience: enemy.config.experience_value,
            position: target_xf.translation,
        });
    }
    true
}
