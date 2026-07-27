use bevy::prelude::*;

use crate::components::armor::*;
use crate::components::player::{
    DerivedPlayerCaps, Player, PlayerBaseStats, PlayerInput, PlayerProgression, PlayerStats,
};
use crate::events::PlayerDamagedEvent;
use crate::engine::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct ArmorPlugin;

impl Plugin for ArmorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                sync_armor_upgrade_state,
                sync_derived_player_caps,
                armor_recharge_system,
                element_switch_system,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn armor_recharge_amount(current: f32, maximum: f32, rate: f32, dt: f32) -> f32 {
    (current + rate.max(0.0) * dt.max(0.0)).min(maximum.max(0.0))
}

fn armor_recharge_system(
    time: Res<Time>,
    mut damaged_events: MessageReader<PlayerDamagedEvent>,
    mut player_q: Query<
        (
            &crate::components::player::PlayerIndex,
            &mut PlayerStats,
            &mut ArmorRechargeState,
        ),
        With<Player>,
    >,
) {
    let mut damaged = [false; 4];
    for event in damaged_events.read() {
        if let Some(index) = event.player_index.filter(|index| *index < 4) {
            damaged[index as usize] = true;
        }
    }
    let dt = time.delta_secs();
    for (index, mut stats, mut recharge) in player_q.iter_mut() {
        if damaged.get(index.0 as usize).copied().unwrap_or(false) {
            recharge.delay_remaining = recharge.delay_after_hit;
            continue;
        }
        recharge.delay_remaining = (recharge.delay_remaining - dt).max(0.0);
        if recharge.delay_remaining <= 0.0 && stats.armor < stats.max_armor {
            stats.armor = armor_recharge_amount(
                stats.armor,
                stats.max_armor,
                recharge.recharge_per_second,
                dt,
            );
        }
    }
}

fn sync_armor_upgrade_state(
    mut player_q: Query<(&mut ArmorSet, &PlayerProgression), With<Player>>,
) {
    for (mut armor, progression) in player_q.iter_mut() {
        let upgrades = &progression.upgrades;
        let state = ArmorUpgradeState {
            shield_defense_bonus: upgrades.armor_shield_defense_bonus(),
            hardened_reduction: upgrades.armor_hardened_reduction(),
            retaliation_damage: upgrades.armor_retaliation_damage(),
            retaliation_radius: upgrades.armor_retaliation_radius(),
            speed_mult: upgrades.armor_speed_mult(),
            strength_mult: upgrades.armor_strength_mult(),
            jump_mult: upgrades.armor_jump_mult(),
        };
        if armor.upgrade_state != state {
            armor.upgrade_state = state;
        }
    }
}

pub(crate) fn current_derived_caps(
    base: PlayerBaseStats,
    stats: &PlayerStats,
    armor: &ArmorSet,
    progression: &PlayerProgression,
) -> DerivedPlayerCaps {
    base.derived_caps(
        stats.level,
        armor.total_health_bonus(),
        progression.perks.hp_bonus(),
        progression.upgrades.armor_health_bonus(),
        progression.upgrades.armor_shield_defense_bonus() * 0.8,
    )
}

/// Reconcile cached effective caps while preserving the fill ratio. This is the
/// sole ordinary-frame writer for maximum health and armor durability.
pub(crate) fn apply_derived_caps(
    stats: &mut PlayerStats,
    health: &mut crate::combat::damage::Health,
    caps: DerivedPlayerCaps,
) {
    if (stats.max_health - caps.max_health).abs() > 0.1 {
        let ratio = if health.max > 0.0 {
            health.current / health.max
        } else {
            1.0
        };
        stats.max_health = caps.max_health;
        health.max = caps.max_health;
        health.current = (caps.max_health * ratio).clamp(0.0, caps.max_health);
    }

    if (stats.max_armor - caps.max_armor).abs() > 0.1 {
        let ratio = if stats.max_armor > 0.0 {
            stats.armor / stats.max_armor
        } else {
            1.0
        };
        stats.max_armor = caps.max_armor;
        stats.armor = (caps.max_armor * ratio).clamp(0.0, caps.max_armor);
    }
}

/// Keep cached effective caps synchronized with stable authored bases.
fn sync_derived_player_caps(
    mut player_q: Query<
        (
            &ArmorSet,
            &PlayerBaseStats,
            &mut PlayerStats,
            &mut crate::combat::damage::Health,
            &PlayerProgression,
        ),
        With<Player>,
    >,
) {
    for (armor, base, mut stats, mut health, progression) in player_q.iter_mut() {
        let stamina_bonus = armor.total_stamina_bonus();
        let caps = current_derived_caps(*base, &stats, armor, progression);
        apply_derived_caps(&mut stats, &mut health, caps);

        let new_stamina_max = 100.0 + stamina_bonus;
        if (stats.max_stamina - new_stamina_max).abs() > 0.1 {
            let ratio = if stats.max_stamina > 0.0 {
                stats.stamina / stats.max_stamina
            } else {
                1.0
            };
            stats.max_stamina = new_stamina_max;
            stats.stamina = (new_stamina_max * ratio).min(new_stamina_max);
        }
    }
}

/// Cycle each player's elemental infusion through the shared per-player input
/// pipeline. This keeps controller ownership intact for all four couch players.
fn element_switch_system(mut player_q: Query<(&PlayerInput, &mut ArmorSet), With<Player>>) {
    for (input, mut armor) in player_q.iter_mut() {
        armor.active_element = cycle_element(armor.active_element, input.armor_element_delta);
    }
}

fn cycle_element(element: ElementType, direction: i8) -> ElementType {
    match direction.cmp(&0) {
        std::cmp::Ordering::Less => cycle_element_prev(element),
        std::cmp::Ordering::Greater => cycle_element_next(element),
        std::cmp::Ordering::Equal => element,
    }
}

fn cycle_element_next(e: ElementType) -> ElementType {
    match e {
        ElementType::None => ElementType::Fire,
        ElementType::Fire => ElementType::Ice,
        ElementType::Ice => ElementType::Electric,
        ElementType::Electric => ElementType::DarkEnergy,
        ElementType::DarkEnergy => ElementType::Rift,
        ElementType::Rift => ElementType::None,
    }
}

fn cycle_element_prev(e: ElementType) -> ElementType {
    match e {
        ElementType::None => ElementType::Rift,
        ElementType::Fire => ElementType::None,
        ElementType::Ice => ElementType::Fire,
        ElementType::Electric => ElementType::Ice,
        ElementType::DarkEnergy => ElementType::Electric,
        ElementType::Rift => ElementType::DarkEnergy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::damage::Health;
    use crate::combat::upgrades::TechUpgradeId;

    #[test]
    fn element_cycle_honors_each_direction_and_idle_input() {
        assert_eq!(cycle_element(ElementType::Fire, -1), ElementType::None);
        assert_eq!(cycle_element(ElementType::Fire, 0), ElementType::Fire);
        assert_eq!(cycle_element(ElementType::Fire, 1), ElementType::Ice);
    }

    #[test]
    fn element_cycle_wraps_in_both_directions() {
        assert_eq!(cycle_element(ElementType::None, -1), ElementType::Rift);
        assert_eq!(cycle_element(ElementType::Rift, 1), ElementType::None);
    }

    #[test]
    fn armor_recharge_is_bounded_and_frame_rate_independent() {
        assert!((armor_recharge_amount(20.0, 100.0, 18.0, 0.5) - 29.0).abs() < 1e-6);
        assert_eq!(armor_recharge_amount(98.0, 100.0, 18.0, 1.0), 100.0);
        assert_eq!(armor_recharge_amount(20.0, 100.0, 18.0, -1.0), 20.0);
    }

    #[test]
    fn equipment_perks_and_upgrades_rederive_without_changing_authored_bases() {
        let base = PlayerBaseStats {
            max_health: 130.0,
            max_armor: 82.0,
        };
        let mut stats = PlayerStats {
            level: 3,
            max_health: 130.0,
            max_armor: 82.0,
            armor: 41.0,
            ..default()
        };
        let mut health = Health::new(130.0);
        health.current = 65.0;
        let mut armor = ArmorSet {
            chest: Some(ArmorPiece::new(ArmorSlot::Chest, ArmorTier::Steel)),
            ..default()
        };
        let mut progression = PlayerProgression::default();
        progression
            .perks
            .ranks
            .push(("heart_vitality".to_string(), 2));
        progression
            .upgrades
            .ranks
            .push((TechUpgradeId::ArmorPlating, 1));
        progression
            .upgrades
            .ranks
            .push((TechUpgradeId::AegisArmorSuite, 1));

        let equipped_caps = current_derived_caps(base, &stats, &armor, &progression);
        apply_derived_caps(&mut stats, &mut health, equipped_caps);

        assert_eq!(base.max_health, 130.0);
        assert_eq!(base.max_armor, 82.0);
        assert_eq!(stats.max_health, equipped_caps.max_health);
        assert_eq!(stats.max_armor, equipped_caps.max_armor);
        assert!((health.current / health.max - 0.5).abs() < 1e-6);
        assert!((stats.armor / stats.max_armor - 0.5).abs() < 1e-6);

        armor.chest = None;
        progression.perks.ranks.clear();
        progression.upgrades.ranks.clear();
        let reset_caps = current_derived_caps(base, &stats, &armor, &progression);
        apply_derived_caps(&mut stats, &mut health, reset_caps);

        assert_eq!(reset_caps.max_health, 150.0);
        assert_eq!(reset_caps.max_armor, 82.0);
        assert_eq!(stats.max_health, 150.0);
        assert_eq!(stats.max_armor, 82.0);
        assert!((health.current / health.max - 0.5).abs() < 1e-6);
        assert!((stats.armor / stats.max_armor - 0.5).abs() < 1e-6);
    }
}
