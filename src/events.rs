use bevy::prelude::*;

// ── Player ────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct PlayerDamagedEvent {
    pub player_index: Option<u8>,
    pub amount: f32,
    pub remaining: f32,
}

#[derive(Message, Debug)]
pub struct PlayerHealedEvent {
    pub amount: f32,
    pub health: f32,
}

#[derive(Message, Debug)]
pub struct PlayerDiedEvent;

#[derive(Message, Debug)]
pub struct PlayerDodgeEvent;

#[derive(Message, Debug)]
pub struct PlayerParryEvent {
    pub player_index: Option<u8>,
    pub success: bool,
}

#[derive(Message, Debug)]
pub struct PlayerLevelUpEvent {
    pub level: u32,
}

#[derive(Message, Debug)]
pub struct PlayerStaminaChangedEvent {
    pub stamina: f32,
}

// ── Enemy ─────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct EnemyDamagedEvent {
    pub entity: Entity,
    pub damage: f32,
    pub position: Vec3,
}

#[derive(Message, Debug)]
pub struct EnemyKilledEvent {
    pub enemy_type: String,
    pub credits: u32,
    pub experience: u32,
    pub position: Vec3,
}

#[derive(Message, Debug)]
pub struct EnemySpawnedEvent {
    pub enemy_type: String,
    pub position: Vec3,
}

// ── Weapon ────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct WeaponFiredEvent;

#[derive(Message, Debug)]
pub struct WeaponSwitchedEvent {
    pub weapon_name: String,
}

#[derive(Message, Debug)]
pub struct WeaponReloadedEvent;

// ── Combat ────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct ComboHitEvent {
    pub combo_name: String,
    pub attack_name: String,
    pub combo_index: usize,
}

#[derive(Message, Debug)]
pub struct ComboFinishedEvent {
    pub combo_name: String,
}

/// Renderer-agnostic description of a confirmed combat impact. Keeping this
/// separate from enemy lifecycle events lets VFX distinguish critical and
/// elemental hits without coupling health logic to a particular renderer.
#[derive(Message, Debug, Clone, Copy)]
pub struct CombatImpactEvent {
    pub position: Vec3,
    pub damage: f32,
    pub damage_type: crate::damage::DamageType,
    pub is_critical: bool,
}

// ── Loot / Chest ──────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct LootCollectedEvent {
    pub loot_type: String,
    pub amount: u32,
}

#[derive(Message, Debug)]
pub struct ChestOpenedEvent;

// ── Waves ─────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct WaveStartedEvent {
    pub wave: u32,
}

#[derive(Message, Debug)]
pub struct WaveCompletedEvent;

// ── Inventory ─────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct InventoryChangedEvent;

#[derive(Message, Debug)]
pub struct ItemPickedUpEvent {
    pub item_id: String,
    pub quantity: u32,
}

// ── Boss ──────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct BossSpawnedEvent {
    pub wave: u32,
    pub position: Vec3,
}

// ── UI ────────────────────────────────────────────────────────────────────────
#[derive(Message, Debug)]
pub struct UiMessageEvent {
    pub text: String,
    pub duration: f32,
}

#[derive(Message, Debug)]
pub struct UiDamageNumberEvent {
    pub position: Vec3,
    pub damage: f32,
    pub is_critical: bool,
}

// ── Starfall I - Chapters / Discoverables ─────────────────────────────────────
#[derive(Message, Debug)]
pub struct ChapterStartedEvent {
    pub chapter: u8,
}

#[derive(Message, Debug)]
pub struct EncounterStepAdvancedEvent {
    pub step_index: usize,
}

#[derive(Message, Debug)]
pub struct ChapterCompletedEvent {
    pub chapter: u8,
}

#[derive(Message, Debug)]
pub struct BossDefeatedEvent {
    pub name: String,
    pub chapter: u8,
}

#[derive(Message, Debug)]
pub struct DiscoverableCollectedEvent {
    pub kind_label: String,
    pub raw_id: String,
}

#[derive(Message, Debug)]
pub struct CompanionRecruitedEvent {
    pub name: String,
    pub player_index: u8,
}

#[derive(Message, Debug)]
pub struct RadioChatterEvent {
    pub speaker: String,
    pub text: String,
    pub faction: crate::components::faction::Faction,
    pub duration: f32,
}

// ── Plugin registration ───────────────────────────────────────────────────────
pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Player
            .add_message::<PlayerDamagedEvent>()
            .add_message::<PlayerHealedEvent>()
            .add_message::<PlayerDiedEvent>()
            .add_message::<PlayerDodgeEvent>()
            .add_message::<PlayerParryEvent>()
            .add_message::<PlayerLevelUpEvent>()
            .add_message::<PlayerStaminaChangedEvent>()
            // Enemy
            .add_message::<EnemyDamagedEvent>()
            .add_message::<EnemyKilledEvent>()
            .add_message::<EnemySpawnedEvent>()
            // Weapon
            .add_message::<WeaponFiredEvent>()
            .add_message::<WeaponSwitchedEvent>()
            .add_message::<WeaponReloadedEvent>()
            // Combat
            .add_message::<ComboHitEvent>()
            .add_message::<ComboFinishedEvent>()
            .add_message::<CombatImpactEvent>()
            // Loot
            .add_message::<LootCollectedEvent>()
            .add_message::<ChestOpenedEvent>()
            // Waves
            .add_message::<WaveStartedEvent>()
            .add_message::<WaveCompletedEvent>()
            // Inventory
            .add_message::<InventoryChangedEvent>()
            .add_message::<ItemPickedUpEvent>()
            // Boss
            .add_message::<BossSpawnedEvent>()
            // UI
            .add_message::<UiMessageEvent>()
            .add_message::<UiDamageNumberEvent>()
            // Starfall I
            .add_message::<ChapterStartedEvent>()
            .add_message::<EncounterStepAdvancedEvent>()
            .add_message::<ChapterCompletedEvent>()
            .add_message::<BossDefeatedEvent>()
            .add_message::<DiscoverableCollectedEvent>()
            .add_message::<CompanionRecruitedEvent>()
            .add_message::<RadioChatterEvent>();
    }
}
