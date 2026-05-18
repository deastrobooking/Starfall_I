use bevy::prelude::*;

// ── Player ────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct PlayerDamagedEvent {
    pub player_index: Option<u8>,
    pub amount: f32,
    pub remaining: f32,
}

#[derive(Event, Debug)]
pub struct PlayerHealedEvent {
    pub amount: f32,
    pub health: f32,
}

#[derive(Event, Debug)]
pub struct PlayerDiedEvent;

#[derive(Event, Debug)]
pub struct PlayerDodgeEvent;

#[derive(Event, Debug)]
pub struct PlayerParryEvent {
    pub success: bool,
}

#[derive(Event, Debug)]
pub struct PlayerLevelUpEvent {
    pub level: u32,
}

#[derive(Event, Debug)]
pub struct PlayerStaminaChangedEvent {
    pub stamina: f32,
}

// ── Enemy ─────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct EnemyDamagedEvent {
    pub entity: Entity,
    pub damage: f32,
    pub position: Vec3,
}

#[derive(Event, Debug)]
pub struct EnemyKilledEvent {
    pub enemy_type: String,
    pub credits: u32,
    pub experience: u32,
    pub position: Vec3,
}

#[derive(Event, Debug)]
pub struct EnemySpawnedEvent {
    pub enemy_type: String,
    pub position: Vec3,
}

// ── Weapon ────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct WeaponFiredEvent;

#[derive(Event, Debug)]
pub struct WeaponSwitchedEvent {
    pub weapon_name: String,
}

#[derive(Event, Debug)]
pub struct WeaponReloadedEvent;

// ── Combat ────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct ComboHitEvent {
    pub combo_name: String,
    pub attack_name: String,
    pub combo_index: usize,
}

#[derive(Event, Debug)]
pub struct ComboFinishedEvent {
    pub combo_name: String,
}

// ── Loot / Chest ──────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct LootCollectedEvent {
    pub loot_type: String,
    pub amount: u32,
}

#[derive(Event, Debug)]
pub struct ChestOpenedEvent;

// ── Waves ─────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct WaveStartedEvent {
    pub wave: u32,
}

#[derive(Event, Debug)]
pub struct WaveCompletedEvent;

// ── Inventory ─────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct InventoryChangedEvent;

#[derive(Event, Debug)]
pub struct ItemPickedUpEvent {
    pub item_id: String,
    pub quantity: u32,
}

// ── Boss ──────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct BossSpawnedEvent {
    pub wave: u32,
    pub position: Vec3,
}

// ── UI ────────────────────────────────────────────────────────────────────────
#[derive(Event, Debug)]
pub struct UiMessageEvent {
    pub text: String,
    pub duration: f32,
}

#[derive(Event, Debug)]
pub struct UiDamageNumberEvent {
    pub position: Vec3,
    pub damage: f32,
    pub is_critical: bool,
}

// ── Starfall I - Chapters / Discoverables ─────────────────────────────────────
#[derive(Event, Debug)]
pub struct ChapterStartedEvent {
    pub chapter: u8,
}

#[derive(Event, Debug)]
pub struct EncounterStepAdvancedEvent {
    pub step_index: usize,
}

#[derive(Event, Debug)]
pub struct ChapterCompletedEvent {
    pub chapter: u8,
}

#[derive(Event, Debug)]
pub struct BossDefeatedEvent {
    pub name: String,
    pub chapter: u8,
}

#[derive(Event, Debug)]
pub struct DiscoverableCollectedEvent {
    pub kind_label: String,
    pub raw_id: String,
}

#[derive(Event, Debug)]
pub struct CompanionRecruitedEvent {
    pub name: String,
    pub player_index: u8,
}

#[derive(Event, Debug)]
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
            .add_event::<PlayerDamagedEvent>()
            .add_event::<PlayerHealedEvent>()
            .add_event::<PlayerDiedEvent>()
            .add_event::<PlayerDodgeEvent>()
            .add_event::<PlayerParryEvent>()
            .add_event::<PlayerLevelUpEvent>()
            .add_event::<PlayerStaminaChangedEvent>()
            // Enemy
            .add_event::<EnemyDamagedEvent>()
            .add_event::<EnemyKilledEvent>()
            .add_event::<EnemySpawnedEvent>()
            // Weapon
            .add_event::<WeaponFiredEvent>()
            .add_event::<WeaponSwitchedEvent>()
            .add_event::<WeaponReloadedEvent>()
            // Combat
            .add_event::<ComboHitEvent>()
            .add_event::<ComboFinishedEvent>()
            // Loot
            .add_event::<LootCollectedEvent>()
            .add_event::<ChestOpenedEvent>()
            // Waves
            .add_event::<WaveStartedEvent>()
            .add_event::<WaveCompletedEvent>()
            // Inventory
            .add_event::<InventoryChangedEvent>()
            .add_event::<ItemPickedUpEvent>()
            // Boss
            .add_event::<BossSpawnedEvent>()
            // UI
            .add_event::<UiMessageEvent>()
            .add_event::<UiDamageNumberEvent>()
            // Starfall I
            .add_event::<ChapterStartedEvent>()
            .add_event::<EncounterStepAdvancedEvent>()
            .add_event::<ChapterCompletedEvent>()
            .add_event::<BossDefeatedEvent>()
            .add_event::<DiscoverableCollectedEvent>()
            .add_event::<CompanionRecruitedEvent>()
            .add_event::<RadioChatterEvent>();
    }
}
