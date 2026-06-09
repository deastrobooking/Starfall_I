use bevy::prelude::*;

use crate::components::armor::*;
use crate::components::player::{Player, PlayerIndex, PlayerStats};
use crate::perks::PerkTree;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct ArmorPlugin;

impl Plugin for ArmorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                sync_armor_upgrade_state,
                apply_armor_health_bonus,
                element_switch_system,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn sync_armor_upgrade_state(
    upgrades: Res<UpgradeLedger>,
    mut player_q: Query<&mut ArmorSet, With<Player>>,
) {
    let state = ArmorUpgradeState {
        shield_defense_bonus: upgrades.armor_shield_defense_bonus(),
        hardened_reduction: upgrades.armor_hardened_reduction(),
        retaliation_damage: upgrades.armor_retaliation_damage(),
        retaliation_radius: upgrades.armor_retaliation_radius(),
        speed_mult: upgrades.armor_speed_mult(),
        strength_mult: upgrades.armor_strength_mult(),
        jump_mult: upgrades.armor_jump_mult(),
    };

    for mut armor in player_q.iter_mut() {
        if armor.upgrade_state != state {
            armor.upgrade_state = state;
        }
    }
}

/// Keep player max health in sync with total armor health bonuses.
fn apply_armor_health_bonus(
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
    mut player_q: Query<(&ArmorSet, &mut PlayerStats, &mut crate::damage::Health), With<Player>>,
) {
    for (armor, mut stats, mut health) in player_q.iter_mut() {
        let bonus = armor.total_health_bonus();
        let stamina_bonus = armor.total_stamina_bonus();
        // Recalculate max health from stable sources so armor/perks cannot stack.
        let new_max = 100.0
            + (stats.level.saturating_sub(1) as f32 * 10.0)
            + bonus
            + perks.hp_bonus()
            + upgrades.armor_health_bonus();
        if (stats.max_health - new_max).abs() > 0.1 {
            let ratio = health.current / health.max;
            stats.max_health = new_max;
            health.max = new_max;
            health.current = new_max * ratio;
        }
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

        let new_armor_max = 100.0 + armor.upgrade_state.shield_defense_bonus * 0.8;
        if (stats.max_armor - new_armor_max).abs() > 0.1 {
            let ratio = if stats.max_armor > 0.0 {
                stats.armor / stats.max_armor
            } else {
                1.0
            };
            stats.max_armor = new_armor_max;
            stats.armor = (new_armor_max * ratio).min(new_armor_max);
        }
    }
}

/// Keyboard shortcuts to cycle elemental infusion (for testing/dev).
/// In production this would be driven by a crafting/equipment UI.
fn element_switch_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_q: Query<(&PlayerIndex, &mut ArmorSet), With<Player>>,
) {
    let direction = if keyboard.just_pressed(KeyCode::BracketLeft) {
        Some(-1)
    } else if keyboard.just_pressed(KeyCode::BracketRight) {
        Some(1)
    } else {
        None
    };

    let Some(direction) = direction else {
        return;
    };

    for (index, mut armor) in player_q.iter_mut() {
        if index.0 != 0 {
            continue;
        }
        armor.active_element = if direction < 0 {
            cycle_element_prev(armor.active_element)
        } else {
            cycle_element_next(armor.active_element)
        };
        break;
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
