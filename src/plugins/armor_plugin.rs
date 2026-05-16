use bevy::prelude::*;

use crate::components::armor::*;
use crate::components::player::{Player, PlayerStats};
use crate::perks::PerkTree;
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct ArmorPlugin;

impl Plugin for ArmorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (apply_armor_health_bonus, element_switch_system).run_if(in_state(AppState::Playing)),
        );
    }
}

/// Keep player max health in sync with total armor health bonuses.
fn apply_armor_health_bonus(
    perks: Res<PerkTree>,
    mut player_q: Query<(&ArmorSet, &mut PlayerStats, &mut crate::damage::Health), With<Player>>,
) {
    for (armor, mut stats, mut health) in player_q.iter_mut() {
        let bonus = armor.total_health_bonus();
        let stamina_bonus = armor.total_stamina_bonus();
        // Recalculate max health from stable sources so armor/perks cannot stack.
        let new_max =
            100.0 + (stats.level.saturating_sub(1) as f32 * 10.0) + bonus + perks.hp_bonus();
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
    }
}

/// Keyboard shortcuts to cycle elemental infusion (for testing/dev).
/// In production this would be driven by a crafting/equipment UI.
fn element_switch_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_q: Query<&mut ArmorSet, With<Player>>,
) {
    if keyboard.just_pressed(KeyCode::BracketLeft) {
        if let Ok(mut armor) = player_q.get_single_mut() {
            armor.active_element = cycle_element_prev(armor.active_element);
        }
    }
    if keyboard.just_pressed(KeyCode::BracketRight) {
        if let Ok(mut armor) = player_q.get_single_mut() {
            armor.active_element = cycle_element_next(armor.active_element);
        }
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
