//! Runtime projections of Designer-published vehicle and spacecraft recipes.
//!
//! These types deliberately remain separate from Heavy Water's saved vehicle
//! presets, robot-assembly progression, and the campaign command registry.
//! Authored content keeps its stable string identity while the existing player
//! motor and strategic-site systems receive small, bounded runtime descriptors.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::resources::WorldSiteId;
use crate::spaceship_forge::{
    PublishedSpacecraftCatalog, SpacecraftClass, SpacecraftDerivedStats, SpacecraftSpec,
};
use crate::vehicle_forge::{DerivedVehicleStats, VehicleClass, VehicleSpec};

/// The existing character controller remains the sole motor for a mounted
/// authored craft. This prevents a follower collider from fighting the player
/// controller while still letting Forge-derived performance affect play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedRideTuning {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub turbo_sprint_speed: f32,
    pub jet_force: f32,
    pub jet_regen_rate: f32,
    pub jet_max_vertical_velocity: f32,
    pub armor_bonus: f32,
}

impl PublishedRideTuning {
    pub fn from_vehicle(spec: &VehicleSpec) -> Self {
        let stats = spec.derived_stats();
        let tuning = vehicle_tuning(stats, spec.class);
        debug_assert!(tuning.is_finite_and_bounded());
        tuning
    }

    pub fn from_spacecraft(spec: &SpacecraftSpec) -> Option<Self> {
        let tuning = matches!(
            spec.class,
            SpacecraftClass::Fighter | SpacecraftClass::Bomber
        )
        .then(|| spacecraft_tuning(spec.derived_stats(), spec.class));
        debug_assert!(tuning.is_none_or(Self::is_finite_and_bounded));
        tuning
    }

    pub fn is_finite_and_bounded(self) -> bool {
        [
            self.walk_speed,
            self.sprint_speed,
            self.turbo_sprint_speed,
            self.jet_force,
            self.jet_regen_rate,
            self.jet_max_vertical_velocity,
            self.armor_bonus,
        ]
        .into_iter()
        .all(f32::is_finite)
            && (0.18..=0.90).contains(&self.walk_speed)
            && (0.32..=1.80).contains(&self.sprint_speed)
            && (0.40..=2.20).contains(&self.turbo_sprint_speed)
            && (0.0..=0.28).contains(&self.jet_force)
            && (0.0..=140.0).contains(&self.jet_regen_rate)
            && (0.0..=1.35).contains(&self.jet_max_vertical_velocity)
            && (0.0..=48.0).contains(&self.armor_bonus)
    }
}

fn vehicle_tuning(stats: DerivedVehicleStats, class: VehicleClass) -> PublishedRideTuning {
    let class_speed_scale = match class {
        VehicleClass::Boat => 0.72,
        VehicleClass::Car => 1.0,
        VehicleClass::Truck => 0.76,
        VehicleClass::Motorcycle => 1.08,
    };
    let sprint = (stats.estimated_top_speed_kph / 205.0 * class_speed_scale).clamp(0.58, 1.55);
    PublishedRideTuning {
        walk_speed: (0.40 + stats.handling_score / 250.0).clamp(0.40, 0.80),
        sprint_speed: sprint,
        turbo_sprint_speed: (sprint * 1.28).clamp(0.72, 1.95),
        jet_force: 0.0,
        jet_regen_rate: 0.0,
        jet_max_vertical_velocity: 0.0,
        armor_bonus: (stats.durability_score * 0.42).clamp(0.0, 42.0),
    }
}

fn spacecraft_tuning(stats: SpacecraftDerivedStats, class: SpacecraftClass) -> PublishedRideTuning {
    let maneuver = (stats.maneuverability / 100.0).clamp(0.0, 1.0);
    let speed = (stats.cruise_speed / 380.0).clamp(0.0, 1.0);
    let power_derate = if stats.power_margin >= 0.0 {
        1.0
    } else {
        (1.0 + stats.power_margin / stats.power_demand.max(1.0)).clamp(0.45, 1.0)
    };
    let class_scale = if class == SpacecraftClass::Bomber {
        0.88
    } else {
        1.0
    };
    PublishedRideTuning {
        walk_speed: (0.36 + maneuver * 0.22).clamp(0.36, 0.62),
        sprint_speed: (0.70 + speed * 0.42).clamp(0.70, 1.12),
        turbo_sprint_speed: (0.94 + speed * 0.50).clamp(0.94, 1.44),
        jet_force: ((0.105 + maneuver * 0.095) * power_derate * class_scale).clamp(0.06, 0.22),
        jet_regen_rate: ((72.0 + speed * 52.0) * power_derate).clamp(44.0, 124.0),
        jet_max_vertical_velocity: ((0.68 + maneuver * 0.42) * class_scale).clamp(0.58, 1.10),
        armor_bonus: (stats.hull_integrity as f32)
            .sqrt()
            .mul_add(0.28, 0.0)
            .clamp(4.0, 48.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedRideKind {
    Ground(VehicleClass),
    Flight(SpacecraftClass),
}

/// A parked or mounted authored craft in the ordinary avatar-motor world.
#[derive(Component, Debug, Clone)]
pub struct PublishedRide {
    pub content_id: String,
    pub display_name: String,
    pub kind: PublishedRideKind,
    pub tuning: PublishedRideTuning,
    pub mount_radius: f32,
    pub mounted_owner: Option<u8>,
}

impl PublishedRide {
    pub fn ground(spec: &VehicleSpec) -> Option<Self> {
        (spec.class != VehicleClass::Boat).then(|| Self {
            content_id: spec.content_id.clone(),
            display_name: spec.display_name.clone(),
            kind: PublishedRideKind::Ground(spec.class),
            tuning: PublishedRideTuning::from_vehicle(spec),
            mount_radius: (spec.length * 0.65).clamp(3.5, 9.0),
            mounted_owner: None,
        })
    }

    pub fn flight(spec: &SpacecraftSpec) -> Option<Self> {
        let tuning = PublishedRideTuning::from_spacecraft(spec)?;
        Some(Self {
            content_id: spec.content_id.clone(),
            display_name: spec.display_name.clone(),
            kind: PublishedRideKind::Flight(spec.class),
            tuning,
            mount_radius: 8.0,
            mounted_owner: None,
        })
    }
}

#[derive(Component, Debug, Clone)]
pub struct PublishedBoatBinding {
    pub content_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedFleetRole {
    Fighter,
    Bomber,
    Destroyer,
    CommandShip,
    OrbitalBase,
}

impl From<SpacecraftClass> for PublishedFleetRole {
    fn from(value: SpacecraftClass) -> Self {
        match value {
            SpacecraftClass::Fighter => Self::Fighter,
            SpacecraftClass::Bomber => Self::Bomber,
            SpacecraftClass::Destroyer => Self::Destroyer,
            SpacecraftClass::CommandShip => Self::CommandShip,
            SpacecraftClass::Base => Self::OrbitalBase,
        }
    }
}

impl PublishedFleetRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fighter => "Fighter",
            Self::Bomber => "Bomber",
            Self::Destroyer => "Destroyer",
            Self::CommandShip => "Command Ship",
            Self::OrbitalBase => "Space Base",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PublishedFleetAsset {
    pub instance_id: String,
    pub content_id: String,
    pub display_name: String,
    pub role: PublishedFleetRole,
    pub spec: SpacecraftSpec,
    pub health: u32,
    pub readiness: u32,
    pub assigned_site: Option<WorldSiteId>,
}

impl PublishedFleetAsset {
    pub fn defense_contribution(&self) -> u32 {
        if self.health == 0 || self.readiness < 30 {
            return 0;
        }
        let stats = self.spec.derived_stats();
        let base = (stats.firepower / 18)
            .saturating_add(stats.hull_integrity / 900)
            .saturating_add(stats.shield_capacity / 700)
            .clamp(1, 80);
        base.saturating_mul(self.readiness.min(100))
            .saturating_mul(self.health.min(100))
            / 10_000
    }
}

/// Session-side strategic projection. Campaign save enums remain unchanged;
/// the stable authored content id is never collapsed into a legacy ship kind.
#[derive(Resource, Debug, Clone, Default)]
pub struct PublishedFleet {
    pub assets: BTreeMap<String, PublishedFleetAsset>,
}

impl PublishedFleet {
    pub fn defense_score_for_site(&self, site: WorldSiteId) -> u32 {
        self.assets
            .values()
            .filter(|asset| asset.assigned_site == Some(site))
            .map(PublishedFleetAsset::defense_contribution)
            .sum()
    }

    /// Add newly published designs without changing snapshots already active
    /// in this play session. Missing designs are removed explicitly so stale
    /// assets cannot defend a site after a same-process republish.
    pub fn reconcile_catalog(
        &mut self,
        catalog: &PublishedSpacecraftCatalog,
        liberated_sites: &[WorldSiteId],
    ) {
        self.assets
            .retain(|content_id, _| catalog.get(content_id).is_some());
        for (index, content_id) in catalog.content_ids().enumerate() {
            let Some(spec) = catalog.get(content_id) else {
                continue;
            };
            self.assets
                .entry(content_id.to_owned())
                .or_insert_with(|| PublishedFleetAsset {
                    instance_id: format!("published-fleet:{content_id}"),
                    content_id: content_id.to_owned(),
                    display_name: spec.display_name.clone(),
                    role: spec.class.into(),
                    spec: spec.clone(),
                    health: 100,
                    readiness: 100,
                    assigned_site: (!liberated_sites.is_empty())
                        .then(|| liberated_sites[index % liberated_sites.len()]),
                });
        }
        for (index, asset) in self.assets.values_mut().enumerate() {
            if asset
                .assigned_site
                .is_none_or(|site| !liberated_sites.contains(&site))
            {
                asset.assigned_site = (!liberated_sites.is_empty())
                    .then(|| liberated_sites[index % liberated_sites.len()]);
            }
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct PublishedFleetVisual {
    pub instance_id: String,
    pub content_id: String,
    pub site: WorldSiteId,
    pub role: PublishedFleetRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vehicle_class_compiles_to_finite_bounded_tuning() {
        for class in VehicleClass::ALL {
            let tuning = PublishedRideTuning::from_vehicle(&VehicleSpec::preset(class));
            assert!(tuning.is_finite_and_bounded(), "{}", class.label());
        }
    }

    #[test]
    fn only_fighter_and_bomber_are_avatar_pilotable() {
        for class in SpacecraftClass::ALL {
            let spec = SpacecraftSpec::preset(class);
            assert_eq!(
                PublishedRide::flight(&spec).is_some(),
                matches!(class, SpacecraftClass::Fighter | SpacecraftClass::Bomber),
                "{}",
                class.label()
            );
        }
    }

    #[test]
    fn catalog_reconciliation_keeps_live_snapshots_and_stable_ids() {
        let mut catalog = PublishedSpacecraftCatalog::default();
        let mut fighter = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        fighter.content_id = "starfall.spaceship.comet".into();
        catalog.replace([fighter.clone()]);
        let mut fleet = PublishedFleet::default();
        fleet.reconcile_catalog(&catalog, &[WorldSiteId(4)]);
        let original = fleet.assets[&fighter.content_id].spec.clone();

        fighter.weapon_mounts += 1;
        catalog.replace([fighter.clone()]);
        fleet.reconcile_catalog(&catalog, &[WorldSiteId(4)]);
        assert_eq!(fleet.assets[&fighter.content_id].spec, original);
        assert_eq!(
            fleet.assets[&fighter.content_id].assigned_site,
            Some(WorldSiteId(4))
        );
    }
}
