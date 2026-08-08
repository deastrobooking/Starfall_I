//! Runtime bridge between Starfall's canonical player components and the
//! deterministic Heavy Water economy domain.
//!
//! [`Inventory`] and [`PlayerStats`] remain authoritative for local gameplay.
//! This plugin only mirrors their inventory stacks and credit balance into the
//! save-backed [`HeavyEconomyState`]. It deliberately accepts no network input:
//! a future online adapter must establish its own authenticated authority seam
//! instead of feeding remote values through this local bridge.
//!
//! Runtime code must not call domain inventory/wallet transactions directly:
//! the next mirror would restore canonical values. Jewel mount/unmount use the
//! atomic helpers below; vendor purchases/sales use the same boundary. Crafting,
//! build, chest, and mining command producers need equivalent two-sided adapters
//! before they mutate live state.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bevy::prelude::*;

use crate::components::inventory::{
    item_definition as canonical_item_definition, Inventory, InventorySlot,
};
use crate::components::player::{Player, PlayerIndex, PlayerStats};
use crate::engine::game_loop::PlayingSetupSet;
use crate::engine::state::AppState;
use crate::resources::GameSettings;
use crate::world::heavy_economy::{
    item_definition as heavy_item_definition, EconomyError, HeavyEconomyState, InventoryRecord,
    ItemStackRecord, JewelTier, OwnerId, WeaponSocket,
};
use crate::world::heavy_water::HeavyWaterProgress;

/// Heavy Water's executable source seeded this many mining nodes by default.
pub const DEFAULT_HEAVY_MINING_NODE_COUNT: u32 = 28;

/// Starfall currently supports four local split-screen player slots.
pub const MAX_LOCAL_HEAVY_OWNERS: u8 = 4;

/// Explicit trust boundary for this adapter.
///
/// There is intentionally no remote/network-authoritative variant. Networked
/// economy commands must not be routed through this resource.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum HeavyEconomyTrustBoundary {
    #[default]
    OfflineLocalOnly,
}

/// Result of the one-time deterministic mining-layout initialization.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum HeavyMiningSeedStatus {
    #[default]
    WaitingForPlaying,
    Seeded {
        world_seed: u64,
        node_count: usize,
    },
    PreservedExisting {
        world_seed: u64,
        node_count: usize,
    },
    Rejected(String),
}

/// Runtime-only diagnostics for the canonical-to-domain mirror.
///
/// The durable economy itself lives at [`HeavyWaterProgress::economy`] so the
/// existing rotating save path remains its sole persistence boundary.
#[derive(Resource, Debug, Default)]
pub struct HeavyEconomyRuntimeBridge {
    pub trust_boundary: HeavyEconomyTrustBoundary,
    /// Valid local owners observed during the most recent Playing-frame pass.
    pub active_owners: BTreeSet<OwnerId>,
    /// Owners whose complete canonical snapshots were committed this pass.
    pub synchronized_owners: BTreeSet<OwnerId>,
    /// A malformed canonical inventory is reported and left uncommitted.
    pub mirror_failures: BTreeMap<OwnerId, HeavyInventoryConversionError>,
    /// Valid Starfall-only stacks are not spendable Heavy materials. Their
    /// occupied slots are subtracted from the projected Heavy capacity and
    /// retained here solely as runtime diagnostics.
    pub canonical_only_reservations: BTreeMap<OwnerId, Vec<CanonicalOnlySlotReservation>>,
    /// Duplicate ECS entities with the same stable player slot are ambiguous;
    /// neither entity is allowed to win by query iteration order.
    pub duplicate_owners: BTreeSet<OwnerId>,
    /// Player indices outside Starfall's four local slots are never registered.
    pub rejected_player_indices: BTreeSet<u8>,
    pub mirror_passes: u64,
    pub mining_seed_status: HeavyMiningSeedStatus,
}

/// Public ordering hooks for integrations that also need the `Last` schedule.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyEconomyBridgeSet {
    SeedWorld,
    MirrorCanonicalPlayers,
}

pub struct HeavyEconomyPlugin;

impl Plugin for HeavyEconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWaterProgress>()
            .init_resource::<HeavyEconomyRuntimeBridge>()
            .add_systems(
                OnEnter(AppState::Playing),
                (
                    seed_heavy_mining_layout.in_set(HeavyEconomyBridgeSet::SeedWorld),
                    mirror_canonical_players.in_set(HeavyEconomyBridgeSet::MirrorCanonicalPlayers),
                )
                    .chain()
                    .in_set(PlayingSetupSet::InitializeDomains),
            )
            .add_systems(
                Last,
                mirror_canonical_players
                    .in_set(HeavyEconomyBridgeSet::MirrorCanonicalPlayers)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Converts a canonical Starfall item ID to the shared Heavy Water ID.
///
/// IDs are deliberately exact and case-sensitive. Silently trimming or
/// guessing aliases would turn malformed/crafted save input into a different
/// item. Concepts shared by both catalogs use the same stable IDs, so a
/// successful conversion can return the domain catalog's static spelling.
pub fn canonical_item_id_to_heavy(
    canonical_item_id: &str,
) -> Result<&'static str, HeavyItemIdConversionError> {
    if canonical_item_id.is_empty() {
        return Err(HeavyItemIdConversionError::Empty);
    }
    heavy_item_definition(canonical_item_id)
        .map(|definition| definition.id)
        .ok_or_else(|| HeavyItemIdConversionError::Unknown(canonical_item_id.to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyItemIdConversionError {
    Empty,
    Unknown(String),
}

impl fmt::Display for HeavyItemIdConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("item ID is empty"),
            Self::Unknown(item_id) => write!(formatter, "unknown Heavy Water item ID `{item_id}`"),
        }
    }
}

impl std::error::Error for HeavyItemIdConversionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyStackConversionError {
    ItemId(HeavyItemIdConversionError),
    ZeroQuantity {
        item_id: String,
    },
    QuantityExceedsStackLimit {
        item_id: String,
        quantity: u32,
        max_stack: u32,
    },
}

impl fmt::Display for HeavyStackConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ItemId(error) => error.fmt(formatter),
            Self::ZeroQuantity { item_id } => {
                write!(formatter, "item `{item_id}` has a zero-sized stack")
            }
            Self::QuantityExceedsStackLimit {
                item_id,
                quantity,
                max_stack,
            } => write!(
                formatter,
                "item `{item_id}` stack {quantity} exceeds its limit {max_stack}"
            ),
        }
    }
}

impl std::error::Error for HeavyStackConversionError {}

/// Converts one canonical stack without mutating either inventory.
pub fn canonical_stack_to_heavy(
    canonical: &InventorySlot,
) -> Result<ItemStackRecord, HeavyStackConversionError> {
    let item_id = canonical_item_id_to_heavy(&canonical.item_id)
        .map_err(HeavyStackConversionError::ItemId)?;
    if canonical.quantity == 0 {
        return Err(HeavyStackConversionError::ZeroQuantity {
            item_id: item_id.to_owned(),
        });
    }

    let definition = heavy_item_definition(item_id)
        .expect("a converted Heavy Water item ID always has a catalog definition");
    if canonical.quantity > definition.max_stack {
        return Err(HeavyStackConversionError::QuantityExceedsStackLimit {
            item_id: item_id.to_owned(),
            quantity: canonical.quantity,
            max_stack: definition.max_stack,
        });
    }

    Ok(ItemStackRecord {
        item_id: item_id.to_owned(),
        quantity: canonical.quantity,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyInventoryConversionError {
    InvalidCapacity {
        max_slots: usize,
    },
    SlotShapeMismatch {
        max_slots: usize,
        actual_slots: usize,
    },
    InvalidStack {
        slot_index: usize,
        source: HeavyStackConversionError,
    },
}

impl fmt::Display for HeavyInventoryConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity { max_slots } => write!(
                formatter,
                "inventory capacity {max_slots} is outside the supported 0..={} range",
                u16::MAX
            ),
            Self::SlotShapeMismatch {
                max_slots,
                actual_slots,
            } => write!(
                formatter,
                "inventory declares {max_slots} slots but contains {actual_slots}"
            ),
            Self::InvalidStack { slot_index, source } => {
                write!(formatter, "invalid inventory slot {slot_index}: {source}")
            }
        }
    }
}

impl std::error::Error for HeavyInventoryConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidStack { source, .. } => Some(source),
            Self::InvalidCapacity { .. } | Self::SlotShapeMismatch { .. } => None,
        }
    }
}

/// One occupied canonical slot intentionally omitted from Heavy's spendable
/// item projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalOnlySlotReservation {
    pub canonical_slot_index: usize,
    pub item_id: String,
    pub quantity: u32,
}

/// A validated inventory projection plus its non-spendable capacity holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeavyInventoryProjection {
    pub inventory: InventoryRecord,
    pub canonical_only_slots: Vec<CanonicalOnlySlotReservation>,
}

/// Stages a complete canonical inventory projection.
///
/// Shared Heavy item stacks retain their relative order. A valid
/// Starfall-only stack (for example a native robot part) is deliberately not
/// copied into the spendable Heavy catalog; instead, its occupied slot is
/// removed from the projected maximum. Consequently the projection has
/// exactly as many empty slots as the canonical inventory, without inventing a
/// sellable/craftable alias. Unknown IDs, invalid quantities, or malformed slot
/// geometry reject the whole projection.
pub fn canonical_inventory_to_heavy(
    canonical: &Inventory,
) -> Result<HeavyInventoryProjection, HeavyInventoryConversionError> {
    let max_slots = u16::try_from(canonical.max_slots).map_err(|_| {
        HeavyInventoryConversionError::InvalidCapacity {
            max_slots: canonical.max_slots,
        }
    })?;
    if canonical.slots.len() != canonical.max_slots {
        return Err(HeavyInventoryConversionError::SlotShapeMismatch {
            max_slots: canonical.max_slots,
            actual_slots: canonical.slots.len(),
        });
    }

    let mut slots = Vec::with_capacity(canonical.slots.len());
    let mut canonical_only_slots = Vec::new();
    for (slot_index, slot) in canonical.slots.iter().enumerate() {
        let Some(stack) = slot else {
            slots.push(None);
            continue;
        };
        match canonical_stack_to_heavy(stack) {
            Ok(converted) => slots.push(Some(converted)),
            Err(
                source @ HeavyStackConversionError::ItemId(HeavyItemIdConversionError::Unknown(_)),
            ) => {
                let Some(definition) = canonical_item_definition(&stack.item_id) else {
                    return Err(HeavyInventoryConversionError::InvalidStack { slot_index, source });
                };
                if stack.quantity == 0 {
                    return Err(HeavyInventoryConversionError::InvalidStack {
                        slot_index,
                        source: HeavyStackConversionError::ZeroQuantity {
                            item_id: stack.item_id.clone(),
                        },
                    });
                }
                if stack.quantity > definition.max_stack {
                    return Err(HeavyInventoryConversionError::InvalidStack {
                        slot_index,
                        source: HeavyStackConversionError::QuantityExceedsStackLimit {
                            item_id: stack.item_id.clone(),
                            quantity: stack.quantity,
                            max_stack: definition.max_stack,
                        },
                    });
                }
                canonical_only_slots.push(CanonicalOnlySlotReservation {
                    canonical_slot_index: slot_index,
                    item_id: stack.item_id.clone(),
                    quantity: stack.quantity,
                });
            }
            Err(source) => {
                return Err(HeavyInventoryConversionError::InvalidStack { slot_index, source });
            }
        }
    }

    let reserved_slots = u16::try_from(canonical_only_slots.len())
        .expect("reservations cannot outnumber a u16-bounded canonical inventory");
    let projected_max_slots = max_slots
        .checked_sub(reserved_slots)
        .expect("every reservation came from an occupied canonical slot");
    debug_assert_eq!(slots.len(), usize::from(projected_max_slots));
    Ok(HeavyInventoryProjection {
        inventory: InventoryRecord {
            max_slots: projected_max_slots,
            slots,
        },
        canonical_only_slots,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMirrorOutcome {
    pub registered_owner: bool,
    pub canonical_fields_changed: bool,
    pub canonical_only_slots: Vec<CanonicalOnlySlotReservation>,
}

/// Commits a validated canonical projection and wallet mirror for one owner.
///
/// Domain-owned fields such as jewel mounts and replay-protection receipts are
/// preserved. Call [`canonical_inventory_to_heavy`] first; its all-or-nothing
/// projection ensures malformed canonical data never reaches this commit seam.
pub fn mirror_canonical_projection(
    economy: &mut HeavyEconomyState,
    owner: OwnerId,
    projection: HeavyInventoryProjection,
    canonical_credits: u32,
) -> CanonicalMirrorOutcome {
    let registered_owner = economy.register_owner(owner);
    let account = economy
        .owners
        .get_mut(&owner)
        .expect("register_owner must leave an owner account available");
    let canonical_credits = u64::from(canonical_credits);
    let canonical_fields_changed =
        account.inventory != projection.inventory || account.credits != canonical_credits;
    if canonical_fields_changed {
        account.inventory = projection.inventory;
        account.credits = canonical_credits;
    }
    CanonicalMirrorOutcome {
        registered_owner,
        canonical_fields_changed,
        canonical_only_slots: projection.canonical_only_slots,
    }
}

/// Failure from a two-authority jewel transaction.
///
/// The adapter stages both sides, so every variant leaves the supplied
/// canonical components and durable economy byte-for-byte unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum HeavyCanonicalTransactionError {
    InventoryProjection(HeavyInventoryConversionError),
    Economy(EconomyError),
    CanonicalInventoryDebitMissing {
        item_id: String,
        quantity: u32,
    },
    CanonicalInventoryCreditRejected {
        item_id: String,
        quantity: u32,
        leftover: u32,
    },
    CanonicalCreditBalanceOutOfRange {
        balance: u64,
    },
    ProjectionMismatch {
        owner: OwnerId,
    },
}

impl fmt::Display for HeavyCanonicalTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InventoryProjection(error) => error.fmt(formatter),
            Self::Economy(error) => {
                write!(formatter, "Heavy economy rejected transaction: {error:?}")
            }
            Self::CanonicalInventoryDebitMissing { item_id, quantity } => write!(
                formatter,
                "canonical inventory is missing {quantity} of `{item_id}`"
            ),
            Self::CanonicalInventoryCreditRejected {
                item_id,
                quantity,
                leftover,
            } => write!(
                formatter,
                "canonical inventory could not accept {quantity} of `{item_id}` ({leftover} left)"
            ),
            Self::CanonicalCreditBalanceOutOfRange { balance } => write!(
                formatter,
                "Heavy credit balance {balance} exceeds Starfall's wallet limit"
            ),
            Self::ProjectionMismatch { owner } => write!(
                formatter,
                "canonical and Heavy inventory projections diverged for owner {}",
                owner.0
            ),
        }
    }
}

impl std::error::Error for HeavyCanonicalTransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InventoryProjection(error) => Some(error),
            Self::Economy(_)
            | Self::CanonicalInventoryDebitMissing { .. }
            | Self::CanonicalInventoryCreditRejected { .. }
            | Self::CanonicalCreditBalanceOutOfRange { .. }
            | Self::ProjectionMismatch { .. } => None,
        }
    }
}

impl From<HeavyInventoryConversionError> for HeavyCanonicalTransactionError {
    fn from(error: HeavyInventoryConversionError) -> Self {
        Self::InventoryProjection(error)
    }
}

impl From<EconomyError> for HeavyCanonicalTransactionError {
    fn from(error: EconomyError) -> Self {
        Self::Economy(error)
    }
}

/// Mounts a jewel while atomically updating both canonical ECS inventory and
/// the Heavy damage ledger.
///
/// Direct calls to [`HeavyEconomyState::mount_jewel`] are domain tests/tools,
/// not a safe runtime integration: the recurring canonical mirror would put
/// its inventory debit back on the next frame. This seam starts from canonical
/// inventory/credits, runs the domain operation on a clone, applies the exact
/// canonical jewel delta, reprojects, validates, and only then commits both.
#[allow(dead_code)] // Public command seam; no gameplay jewel-command producer exists yet.
pub fn mount_jewel_from_canonical_atomic(
    economy: &mut HeavyEconomyState,
    canonical_inventory: &mut Inventory,
    canonical_stats: &PlayerStats,
    owner: OwnerId,
    transaction_id: u64,
    weapon: WeaponSocket,
    tier: JewelTier,
) -> Result<(), HeavyCanonicalTransactionError> {
    let mut staged_economy =
        stage_economy_from_canonical(economy, owner, canonical_inventory, canonical_stats)?;
    let previous = staged_economy
        .account(owner)?
        .jewel_mounts
        .get(&weapon)
        .copied();
    staged_economy.mount_jewel(owner, transaction_id, weapon, tier)?;

    let mut staged_inventory = canonical_inventory.clone();
    remove_canonical_item_exact(&mut staged_inventory, tier.item_id(), 1)?;
    if let Some(previous) = previous {
        add_canonical_item_exact(&mut staged_inventory, previous.item_id(), 1)?;
    }
    validate_canonical_domain_alignment(
        &staged_economy,
        owner,
        &staged_inventory,
        canonical_stats,
    )?;

    *canonical_inventory = staged_inventory;
    *economy = staged_economy;
    Ok(())
}

/// Unmounts a jewel through the same two-sided atomic seam as
/// [`mount_jewel_from_canonical_atomic`].
#[allow(dead_code)] // Public command seam; no gameplay jewel-command producer exists yet.
pub fn unmount_jewel_to_canonical_atomic(
    economy: &mut HeavyEconomyState,
    canonical_inventory: &mut Inventory,
    canonical_stats: &PlayerStats,
    owner: OwnerId,
    transaction_id: u64,
    weapon: WeaponSocket,
) -> Result<JewelTier, HeavyCanonicalTransactionError> {
    let mut staged_economy =
        stage_economy_from_canonical(economy, owner, canonical_inventory, canonical_stats)?;
    let tier = staged_economy.unmount_jewel(owner, transaction_id, weapon)?;

    let mut staged_inventory = canonical_inventory.clone();
    add_canonical_item_exact(&mut staged_inventory, tier.item_id(), 1)?;
    validate_canonical_domain_alignment(
        &staged_economy,
        owner,
        &staged_inventory,
        canonical_stats,
    )?;

    *canonical_inventory = staged_inventory;
    *economy = staged_economy;
    Ok(tier)
}

/// Buys one catalog line while committing vendor stock, canonical inventory,
/// and canonical credits together or not at all.
pub fn buy_from_vendor_canonical_atomic(
    economy: &mut HeavyEconomyState,
    canonical_inventory: &mut Inventory,
    canonical_stats: &mut PlayerStats,
    owner: OwnerId,
    transaction_id: u64,
    vendor_id: &str,
    item_id: &str,
    quantity: u32,
) -> Result<u32, HeavyCanonicalTransactionError> {
    let mut staged_economy =
        stage_economy_from_canonical(economy, owner, canonical_inventory, canonical_stats)?;
    let balance =
        staged_economy.buy_from_vendor(owner, transaction_id, vendor_id, item_id, quantity)?;
    let canonical_balance = u32::try_from(balance).map_err(|_| {
        HeavyCanonicalTransactionError::CanonicalCreditBalanceOutOfRange { balance }
    })?;

    let mut staged_inventory = canonical_inventory.clone();
    add_canonical_item_exact(&mut staged_inventory, item_id, quantity)?;
    let mut staged_stats = canonical_stats.clone();
    staged_stats.credits = canonical_balance;
    validate_canonical_domain_alignment(&staged_economy, owner, &staged_inventory, &staged_stats)?;

    *canonical_inventory = staged_inventory;
    canonical_stats.credits = canonical_balance;
    *economy = staged_economy;
    Ok(canonical_balance)
}

/// Sells a canonical stack through the same all-or-nothing vendor boundary as
/// [`buy_from_vendor_canonical_atomic`].
pub fn sell_to_vendor_canonical_atomic(
    economy: &mut HeavyEconomyState,
    canonical_inventory: &mut Inventory,
    canonical_stats: &mut PlayerStats,
    owner: OwnerId,
    transaction_id: u64,
    vendor_id: &str,
    item_id: &str,
    quantity: u32,
) -> Result<u32, HeavyCanonicalTransactionError> {
    let mut staged_economy =
        stage_economy_from_canonical(economy, owner, canonical_inventory, canonical_stats)?;
    let balance =
        staged_economy.sell_to_vendor(owner, transaction_id, vendor_id, item_id, quantity)?;
    let canonical_balance = u32::try_from(balance).map_err(|_| {
        HeavyCanonicalTransactionError::CanonicalCreditBalanceOutOfRange { balance }
    })?;

    let mut staged_inventory = canonical_inventory.clone();
    remove_canonical_item_exact(&mut staged_inventory, item_id, quantity)?;
    let mut staged_stats = canonical_stats.clone();
    staged_stats.credits = canonical_balance;
    validate_canonical_domain_alignment(&staged_economy, owner, &staged_inventory, &staged_stats)?;

    *canonical_inventory = staged_inventory;
    canonical_stats.credits = canonical_balance;
    *economy = staged_economy;
    Ok(canonical_balance)
}

fn stage_economy_from_canonical(
    economy: &HeavyEconomyState,
    owner: OwnerId,
    canonical_inventory: &Inventory,
    canonical_stats: &PlayerStats,
) -> Result<HeavyEconomyState, HeavyCanonicalTransactionError> {
    let projection = canonical_inventory_to_heavy(canonical_inventory)?;
    let mut staged = economy.clone();
    mirror_canonical_projection(&mut staged, owner, projection, canonical_stats.credits);
    Ok(staged)
}

fn remove_canonical_item_exact(
    inventory: &mut Inventory,
    item_id: &str,
    quantity: u32,
) -> Result<(), HeavyCanonicalTransactionError> {
    if inventory.remove_item(item_id, quantity) {
        Ok(())
    } else {
        Err(
            HeavyCanonicalTransactionError::CanonicalInventoryDebitMissing {
                item_id: item_id.to_owned(),
                quantity,
            },
        )
    }
}

fn add_canonical_item_exact(
    inventory: &mut Inventory,
    item_id: &str,
    quantity: u32,
) -> Result<(), HeavyCanonicalTransactionError> {
    let definition = canonical_item_definition(item_id)
        .ok_or_else(|| EconomyError::UnknownItem(item_id.to_owned()))?;
    let leftover = inventory.add_item(item_id, quantity, definition.max_stack);
    if leftover == 0 {
        Ok(())
    } else {
        Err(
            HeavyCanonicalTransactionError::CanonicalInventoryCreditRejected {
                item_id: item_id.to_owned(),
                quantity,
                leftover,
            },
        )
    }
}

fn validate_canonical_domain_alignment(
    economy: &HeavyEconomyState,
    owner: OwnerId,
    canonical_inventory: &Inventory,
    canonical_stats: &PlayerStats,
) -> Result<(), HeavyCanonicalTransactionError> {
    let projection = canonical_inventory_to_heavy(canonical_inventory)?;
    projection.inventory.validate()?;
    economy.validate()?;
    let account = economy.account(owner)?;
    if account.inventory != projection.inventory
        || account.credits != u64::from(canonical_stats.credits)
    {
        return Err(HeavyCanonicalTransactionError::ProjectionMismatch { owner });
    }
    Ok(())
}

/// Maps a stable local player slot to its Heavy economy owner key.
pub const fn local_player_owner(player_index: u8) -> Option<OwnerId> {
    if player_index < MAX_LOCAL_HEAVY_OWNERS {
        Some(OwnerId(player_index))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyMiningSeedOutcome {
    Seeded { world_seed: u64, node_count: usize },
    PreservedExisting { world_seed: u64, node_count: usize },
}

/// Seeds Heavy Water's deterministic mining layout only when none is saved.
///
/// The mutation is staged on a clone and validated before commit, preventing a
/// partial layout if an ID counter overflows or another record is malformed.
/// A persisted non-zero world seed wins over the settings fallback.
pub fn ensure_default_heavy_mining_layout(
    economy: &mut HeavyEconomyState,
    fallback_world_seed: u64,
) -> Result<HeavyMiningSeedOutcome, crate::world::heavy_economy::EconomyError> {
    if !economy.mining_nodes.is_empty() {
        return Ok(HeavyMiningSeedOutcome::PreservedExisting {
            world_seed: economy.world_seed,
            node_count: economy.mining_nodes.len(),
        });
    }

    let mut staged = economy.clone();
    if staged.world_seed == 0 {
        staged.world_seed = fallback_world_seed;
    }
    staged.seed_mining_nodes(DEFAULT_HEAVY_MINING_NODE_COUNT)?;
    staged.validate()?;
    let outcome = HeavyMiningSeedOutcome::Seeded {
        world_seed: staged.world_seed,
        node_count: staged.mining_nodes.len(),
    };
    *economy = staged;
    Ok(outcome)
}

fn seed_heavy_mining_layout(
    settings: Res<GameSettings>,
    mut progress: ResMut<HeavyWaterProgress>,
    mut bridge: ResMut<HeavyEconomyRuntimeBridge>,
) {
    if bridge.mining_seed_status != HeavyMiningSeedStatus::WaitingForPlaying {
        return;
    }
    bridge.mining_seed_status =
        match ensure_default_heavy_mining_layout(&mut progress.economy, settings.world_seed) {
            Ok(HeavyMiningSeedOutcome::Seeded {
                world_seed,
                node_count,
            }) => HeavyMiningSeedStatus::Seeded {
                world_seed,
                node_count,
            },
            Ok(HeavyMiningSeedOutcome::PreservedExisting {
                world_seed,
                node_count,
            }) => HeavyMiningSeedStatus::PreservedExisting {
                world_seed,
                node_count,
            },
            Err(error) => HeavyMiningSeedStatus::Rejected(format!("{error:?}")),
        };
}

fn mirror_canonical_players(
    players: Query<(&PlayerIndex, &Inventory, &PlayerStats), With<Player>>,
    mut progress: ResMut<HeavyWaterProgress>,
    mut bridge: ResMut<HeavyEconomyRuntimeBridge>,
) {
    debug_assert_eq!(
        bridge.trust_boundary,
        HeavyEconomyTrustBoundary::OfflineLocalOnly
    );
    let mut candidates = BTreeMap::new();
    let mut active_owners = BTreeSet::new();
    let mut duplicate_owners = BTreeSet::new();
    let mut rejected_player_indices = BTreeSet::new();

    for (player_index, inventory, stats) in &players {
        let Some(owner) = local_player_owner(player_index.0) else {
            rejected_player_indices.insert(player_index.0);
            continue;
        };
        active_owners.insert(owner);
        if candidates.contains_key(&owner) {
            duplicate_owners.insert(owner);
            continue;
        }
        candidates.insert(
            owner,
            canonical_inventory_to_heavy(inventory).map(|projection| (projection, stats.credits)),
        );
    }

    // Never select a duplicate owner based on nondeterministic ECS iteration.
    for owner in &duplicate_owners {
        candidates.remove(owner);
    }

    let mut synchronized_owners = BTreeSet::new();
    let mut mirror_failures = BTreeMap::new();
    let mut canonical_only_reservations = BTreeMap::new();
    for (owner, candidate) in candidates {
        match candidate {
            Ok((projection, credits)) => {
                let outcome =
                    mirror_canonical_projection(&mut progress.economy, owner, projection, credits);
                if !outcome.canonical_only_slots.is_empty() {
                    canonical_only_reservations.insert(owner, outcome.canonical_only_slots);
                }
                synchronized_owners.insert(owner);
            }
            Err(error) => {
                mirror_failures.insert(owner, error);
            }
        }
    }

    bridge.active_owners = active_owners;
    bridge.synchronized_owners = synchronized_owners;
    bridge.mirror_failures = mirror_failures;
    bridge.canonical_only_reservations = canonical_only_reservations;
    bridge.duplicate_owners = duplicate_owners;
    bridge.rejected_player_indices = rejected_player_indices;
    bridge.mirror_passes = bridge.mirror_passes.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::heavy_economy::{JewelTier, WeaponSocket};

    fn inventory_with_slots(slots: Vec<Option<InventorySlot>>) -> Inventory {
        Inventory {
            max_slots: slots.len(),
            slots,
        }
    }

    fn total_jewels(inventory: &Inventory, economy: &HeavyEconomyState, owner: OwnerId) -> u32 {
        let loose = [JewelTier::Rough, JewelTier::Cut, JewelTier::Flawless]
            .into_iter()
            .map(|tier| inventory.count(tier.item_id()))
            .sum::<u32>();
        let mounted = economy
            .account(owner)
            .map(|account| account.jewel_mounts.len() as u32)
            .unwrap_or(0);
        loose + mounted
    }

    #[test]
    fn item_ids_are_exact_catalog_keys() {
        assert_eq!(
            canonical_item_id_to_heavy("power_jewel_flawless"),
            Ok("power_jewel_flawless")
        );
        assert_eq!(
            canonical_item_id_to_heavy(""),
            Err(HeavyItemIdConversionError::Empty)
        );
        assert_eq!(
            canonical_item_id_to_heavy(" Health_Pack "),
            Err(HeavyItemIdConversionError::Unknown(
                " Health_Pack ".to_owned()
            ))
        );
        assert_eq!(
            canonical_item_id_to_heavy("robot_scrap_frame"),
            Err(HeavyItemIdConversionError::Unknown(
                "robot_scrap_frame".to_owned()
            ))
        );
    }

    #[test]
    fn stack_conversion_enforces_nonzero_catalog_limits() {
        let valid = InventorySlot {
            item_id: "health_pack".to_owned(),
            quantity: 10,
        };
        assert_eq!(
            canonical_stack_to_heavy(&valid),
            Ok(ItemStackRecord {
                item_id: "health_pack".to_owned(),
                quantity: 10,
            })
        );

        let zero = InventorySlot {
            quantity: 0,
            ..valid.clone()
        };
        assert!(matches!(
            canonical_stack_to_heavy(&zero),
            Err(HeavyStackConversionError::ZeroQuantity { .. })
        ));

        let oversized = InventorySlot {
            quantity: 11,
            ..valid
        };
        assert_eq!(
            canonical_stack_to_heavy(&oversized),
            Err(HeavyStackConversionError::QuantityExceedsStackLimit {
                item_id: "health_pack".to_owned(),
                quantity: 11,
                max_stack: 10,
            })
        );
    }

    #[test]
    fn inventory_conversion_preserves_slot_layout() {
        let canonical = inventory_with_slots(vec![
            Some(InventorySlot {
                item_id: "scrap_metal".to_owned(),
                quantity: 12,
            }),
            None,
            Some(InventorySlot {
                item_id: "power_jewel_rough".to_owned(),
                quantity: 2,
            }),
        ]);
        let projection = canonical_inventory_to_heavy(&canonical).unwrap();
        assert!(projection.canonical_only_slots.is_empty());
        let converted = projection.inventory;
        assert_eq!(converted.max_slots, 3);
        assert_eq!(converted.slots[1], None);
        assert_eq!(converted.slots[0].as_ref().unwrap().quantity, 12);
        assert_eq!(
            converted.slots[2].as_ref().unwrap().item_id,
            "power_jewel_rough"
        );
    }

    #[test]
    fn canonical_only_stacks_reserve_capacity_without_becoming_spendable() {
        let canonical = inventory_with_slots(vec![
            Some(InventorySlot {
                item_id: "energy_core".to_owned(),
                quantity: 3,
            }),
            Some(InventorySlot {
                item_id: "robot_scrap_frame".to_owned(),
                quantity: 12,
            }),
            None,
            Some(InventorySlot {
                item_id: "robot_star_drive".to_owned(),
                quantity: 2,
            }),
        ]);

        let projection = canonical_inventory_to_heavy(&canonical).unwrap();
        assert_eq!(projection.inventory.max_slots, 2);
        assert_eq!(projection.inventory.slots.len(), 2);
        assert_eq!(projection.inventory.occupied_slots(), 1);
        assert_eq!(projection.inventory.item_count("energy_core"), 3);
        assert_eq!(projection.inventory.item_count("robot_scrap_frame"), 0);
        assert_eq!(projection.inventory.slots[1], None);
        assert_eq!(
            projection.canonical_only_slots,
            vec![
                CanonicalOnlySlotReservation {
                    canonical_slot_index: 1,
                    item_id: "robot_scrap_frame".to_owned(),
                    quantity: 12,
                },
                CanonicalOnlySlotReservation {
                    canonical_slot_index: 3,
                    item_id: "robot_star_drive".to_owned(),
                    quantity: 2,
                },
            ]
        );
        projection.inventory.validate().unwrap();
    }

    #[test]
    fn fully_canonical_only_inventory_projects_to_valid_zero_capacity() {
        let canonical = inventory_with_slots(vec![
            Some(InventorySlot {
                item_id: "robot_scrap_frame".to_owned(),
                quantity: 99,
            }),
            Some(InventorySlot {
                item_id: "robot_command_deck".to_owned(),
                quantity: 10,
            }),
        ]);

        let projection = canonical_inventory_to_heavy(&canonical).unwrap();
        assert_eq!(projection.inventory, InventoryRecord::new(0));
        assert_eq!(projection.canonical_only_slots.len(), 2);
        projection.inventory.validate().unwrap();
        assert!(!projection.inventory.can_add("gear", 1).unwrap());
    }

    #[test]
    fn malformed_canonical_only_stack_still_rejects_the_projection() {
        let canonical = inventory_with_slots(vec![Some(InventorySlot {
            item_id: "robot_scrap_frame".to_owned(),
            quantity: 100,
        })]);
        assert_eq!(
            canonical_inventory_to_heavy(&canonical),
            Err(HeavyInventoryConversionError::InvalidStack {
                slot_index: 0,
                source: HeavyStackConversionError::QuantityExceedsStackLimit {
                    item_id: "robot_scrap_frame".to_owned(),
                    quantity: 100,
                    max_stack: 99,
                },
            })
        );
    }

    #[test]
    fn inventory_conversion_rejects_mismatched_slot_geometry() {
        let malformed = Inventory {
            max_slots: 2,
            slots: vec![None],
        };
        assert_eq!(
            canonical_inventory_to_heavy(&malformed),
            Err(HeavyInventoryConversionError::SlotShapeMismatch {
                max_slots: 2,
                actual_slots: 1,
            })
        );
    }

    #[test]
    fn malformed_inventory_does_not_touch_an_owner_account() {
        let owner = OwnerId(2);
        let mut economy = HeavyEconomyState::new(71);
        economy.register_owner(owner);
        economy.owners.get_mut(&owner).unwrap().credits = 900;
        let before = economy.clone();
        let malformed = inventory_with_slots(vec![Some(InventorySlot {
            item_id: "not_a_real_item".to_owned(),
            quantity: 1,
        })]);

        assert!(canonical_inventory_to_heavy(&malformed).is_err());
        assert_eq!(economy, before);
    }

    #[test]
    fn mirror_updates_only_canonical_fields_and_registers_once() {
        let owner = OwnerId(1);
        let mut economy = HeavyEconomyState::new(9);
        assert!(economy.register_owner(owner));
        {
            let account = economy.owners.get_mut(&owner).unwrap();
            account
                .jewel_mounts
                .insert(WeaponSocket::Rifle, JewelTier::Cut);
            account.applied_transactions.insert(55);
        }
        let inventory = inventory_with_slots(vec![Some(InventorySlot {
            item_id: "energy_core".to_owned(),
            quantity: 3,
        })]);

        let projection = canonical_inventory_to_heavy(&inventory).unwrap();
        let outcome = mirror_canonical_projection(&mut economy, owner, projection, 123);
        assert_eq!(
            outcome,
            CanonicalMirrorOutcome {
                registered_owner: false,
                canonical_fields_changed: true,
                canonical_only_slots: Vec::new(),
            }
        );
        let account = economy.account(owner).unwrap();
        assert_eq!(account.credits, 123);
        assert_eq!(account.inventory.item_count("energy_core"), 3);
        assert_eq!(account.jewel_mounts[&WeaponSocket::Rifle], JewelTier::Cut);
        assert!(account.applied_transactions.contains(&55));

        let newcomer = OwnerId(3);
        let projection = canonical_inventory_to_heavy(&inventory).unwrap();
        let outcome = mirror_canonical_projection(&mut economy, newcomer, projection, 8);
        assert!(outcome.registered_owner);
        assert_eq!(economy.account(newcomer).unwrap().credits, 8);
    }

    #[test]
    fn atomic_jewel_flow_survives_recurring_mirror_without_duplication_or_loss() {
        let owner = OwnerId(0);
        let stats = PlayerStats {
            credits: 345,
            ..default()
        };
        let mut inventory = inventory_with_slots(vec![
            Some(InventorySlot {
                item_id: JewelTier::Rough.item_id().to_owned(),
                quantity: 1,
            }),
            Some(InventorySlot {
                item_id: JewelTier::Cut.item_id().to_owned(),
                quantity: 1,
            }),
            Some(InventorySlot {
                item_id: "robot_scrap_frame".to_owned(),
                quantity: 7,
            }),
            None,
        ]);
        let mut economy = HeavyEconomyState::new(44);

        mount_jewel_from_canonical_atomic(
            &mut economy,
            &mut inventory,
            &stats,
            owner,
            10,
            WeaponSocket::Rifle,
            JewelTier::Rough,
        )
        .unwrap();
        assert_eq!(inventory.count(JewelTier::Rough.item_id()), 0);
        assert_eq!(inventory.count(JewelTier::Cut.item_id()), 1);
        assert_eq!(inventory.count("robot_scrap_frame"), 7);
        assert_eq!(total_jewels(&inventory, &economy, owner), 2);
        assert_eq!(economy.account(owner).unwrap().credits, 345);

        // This is the same projection the Last-schedule mirror performs. The
        // mounted jewel stays debited because canonical inventory was updated
        // in the same atomic transaction.
        let projection = canonical_inventory_to_heavy(&inventory).unwrap();
        let mirror = mirror_canonical_projection(&mut economy, owner, projection, stats.credits);
        assert!(!mirror.canonical_fields_changed);
        assert_eq!(inventory.count(JewelTier::Rough.item_id()), 0);
        assert_eq!(total_jewels(&inventory, &economy, owner), 2);

        let inventory_before_replay = inventory.clone();
        let economy_before_replay = economy.clone();
        assert_eq!(
            mount_jewel_from_canonical_atomic(
                &mut economy,
                &mut inventory,
                &stats,
                owner,
                10,
                WeaponSocket::Rifle,
                JewelTier::Cut,
            ),
            Err(HeavyCanonicalTransactionError::Economy(
                EconomyError::AlreadyApplied(10)
            ))
        );
        assert_eq!(inventory, inventory_before_replay);
        assert_eq!(economy, economy_before_replay);

        mount_jewel_from_canonical_atomic(
            &mut economy,
            &mut inventory,
            &stats,
            owner,
            11,
            WeaponSocket::Rifle,
            JewelTier::Cut,
        )
        .unwrap();
        assert_eq!(inventory.count(JewelTier::Rough.item_id()), 1);
        assert_eq!(inventory.count(JewelTier::Cut.item_id()), 0);
        assert_eq!(total_jewels(&inventory, &economy, owner), 2);
        assert_eq!(
            economy.account(owner).unwrap().jewel_mounts[&WeaponSocket::Rifle],
            JewelTier::Cut
        );

        assert_eq!(
            unmount_jewel_to_canonical_atomic(
                &mut economy,
                &mut inventory,
                &stats,
                owner,
                12,
                WeaponSocket::Rifle,
            )
            .unwrap(),
            JewelTier::Cut
        );
        assert_eq!(inventory.count(JewelTier::Rough.item_id()), 1);
        assert_eq!(inventory.count(JewelTier::Cut.item_id()), 1);
        assert_eq!(inventory.count("robot_scrap_frame"), 7);
        assert_eq!(total_jewels(&inventory, &economy, owner), 2);
        assert!(!economy
            .account(owner)
            .unwrap()
            .jewel_mounts
            .contains_key(&WeaponSocket::Rifle));
    }

    #[test]
    fn failed_unmount_with_only_reserved_capacity_commits_neither_side() {
        let owner = OwnerId(2);
        let stats = PlayerStats::default();
        let mut inventory = inventory_with_slots(vec![Some(InventorySlot {
            item_id: "robot_command_deck".to_owned(),
            quantity: 1,
        })]);
        let mut economy = HeavyEconomyState::new(91);
        economy.register_owner(owner);
        {
            let account = economy.owners.get_mut(&owner).unwrap();
            account.inventory = InventoryRecord::new(0);
            account
                .jewel_mounts
                .insert(WeaponSocket::Laser, JewelTier::Flawless);
        }
        let inventory_before = inventory.clone();
        let economy_before = economy.clone();

        assert_eq!(
            unmount_jewel_to_canonical_atomic(
                &mut economy,
                &mut inventory,
                &stats,
                owner,
                70,
                WeaponSocket::Laser,
            ),
            Err(HeavyCanonicalTransactionError::Economy(
                EconomyError::InventoryFull {
                    item_id: JewelTier::Flawless.item_id().to_owned(),
                    quantity: 1,
                }
            ))
        );
        assert_eq!(inventory, inventory_before);
        assert_eq!(economy, economy_before);
    }

    #[test]
    fn vendor_buy_and_sell_commit_wallet_inventory_and_stock_together() {
        let owner = OwnerId(0);
        let mut stats = PlayerStats {
            credits: 1_000,
            ..default()
        };
        let mut inventory = inventory_with_slots(vec![None, None, None]);
        let mut economy = HeavyEconomyState::new(12);
        let vendor_id = "general_shop_1";
        let item_id = "health_pack";
        let stock = economy.vendors[vendor_id]
            .items
            .iter()
            .find(|line| line.item_id == item_id)
            .unwrap()
            .clone();

        let balance = buy_from_vendor_canonical_atomic(
            &mut economy,
            &mut inventory,
            &mut stats,
            owner,
            100,
            vendor_id,
            item_id,
            2,
        )
        .unwrap();
        assert_eq!(balance, 1_000 - stock.buy_price * 2);
        assert_eq!(stats.credits, balance);
        assert_eq!(inventory.count(item_id), 2);
        assert_eq!(
            economy.vendors[vendor_id]
                .items
                .iter()
                .find(|line| line.item_id == item_id)
                .unwrap()
                .stock,
            stock.stock - 2
        );

        let balance = sell_to_vendor_canonical_atomic(
            &mut economy,
            &mut inventory,
            &mut stats,
            owner,
            101,
            vendor_id,
            item_id,
            1,
        )
        .unwrap();
        assert_eq!(balance, 1_000 - stock.buy_price * 2 + stock.sell_price);
        assert_eq!(stats.credits, balance);
        assert_eq!(inventory.count(item_id), 1);
        let projection = canonical_inventory_to_heavy(&inventory).unwrap();
        assert_eq!(
            economy.account(owner).unwrap().inventory,
            projection.inventory
        );
        assert_eq!(economy.account(owner).unwrap().credits, u64::from(balance));
    }

    #[test]
    fn vendor_failures_and_credit_overflow_roll_back_every_authority() {
        let owner = OwnerId(1);
        let mut stats = PlayerStats {
            credits: 10_000,
            ..default()
        };
        let mut inventory = inventory_with_slots(vec![Some(InventorySlot {
            item_id: "gear".to_owned(),
            quantity: 1,
        })]);
        let mut economy = HeavyEconomyState::new(33);
        let inventory_before = inventory.clone();
        let stats_before = stats.clone();
        let economy_before = economy.clone();
        assert!(buy_from_vendor_canonical_atomic(
            &mut economy,
            &mut inventory,
            &mut stats,
            owner,
            200,
            "general_shop_1",
            "health_pack",
            1,
        )
        .is_err());
        assert_eq!(inventory, inventory_before);
        assert_eq!(stats.credits, stats_before.credits);
        assert_eq!(economy, economy_before);

        stats.credits = u32::MAX;
        let inventory_before = inventory.clone();
        let economy_before = economy.clone();
        assert!(matches!(
            sell_to_vendor_canonical_atomic(
                &mut economy,
                &mut inventory,
                &mut stats,
                owner,
                201,
                "general_shop_1",
                "gear",
                1,
            ),
            Err(HeavyCanonicalTransactionError::CanonicalCreditBalanceOutOfRange { .. })
        ));
        assert_eq!(inventory, inventory_before);
        assert_eq!(stats.credits, u32::MAX);
        assert_eq!(economy, economy_before);
    }

    #[test]
    fn mining_seed_is_deterministic_and_never_replaces_saved_nodes() {
        let mut first = HeavyEconomyState::new(0);
        let mut second = HeavyEconomyState::new(0);
        assert_eq!(
            ensure_default_heavy_mining_layout(&mut first, 42).unwrap(),
            HeavyMiningSeedOutcome::Seeded {
                world_seed: 42,
                node_count: DEFAULT_HEAVY_MINING_NODE_COUNT as usize,
            }
        );
        ensure_default_heavy_mining_layout(&mut second, 42).unwrap();
        assert_eq!(first.mining_nodes, second.mining_nodes);

        let saved = first.clone();
        assert_eq!(
            ensure_default_heavy_mining_layout(&mut first, 999).unwrap(),
            HeavyMiningSeedOutcome::PreservedExisting {
                world_seed: 42,
                node_count: DEFAULT_HEAVY_MINING_NODE_COUNT as usize,
            }
        );
        assert_eq!(first, saved);
    }

    #[test]
    fn failed_mining_seed_is_atomic() {
        let mut economy = HeavyEconomyState::new(17);
        economy.next_mining_node_id = u64::MAX;
        let before = economy.clone();
        assert!(ensure_default_heavy_mining_layout(&mut economy, 99).is_err());
        assert_eq!(economy, before);
    }

    #[test]
    fn local_owner_mapping_is_bounded_to_split_screen_slots() {
        assert_eq!(local_player_owner(0), Some(OwnerId(0)));
        assert_eq!(local_player_owner(3), Some(OwnerId(3)));
        assert_eq!(local_player_owner(4), None);
        assert_eq!(local_player_owner(u8::MAX), None);
    }
}
