//! Runtime-resolved published-content catalogs and the procedural material
//! resolution pipeline that reads them.
//!
//! Split out of `engine_tools` because these are genuinely neutral runtime
//! data — stable-content-id lookups and a gameplay system that resolves them
//! against spawned geometry — read by pure gameplay files
//! (`world::published_content`, `plugins::world_plugin`, `plugins::world_plugin::roads`)
//! that have no Designer/authoring concern at all. `engine_tools` still owns
//! populating these catalogs from a live `ForgeProject`
//! (`rebuild_published_content_catalogs`) and the Designer-only manual
//! material picker (`EditorMaterialBinding`'s authoring call sites) — both
//! import these types back, the same direction `published_manifest`/
//! `dialogue_schema` already established.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::engine::rendering::{EnergyMaterial, IceMaterial, LavaMaterial, ShieldMaterial, ToonMaterial, WaterMaterial};
use crate::robots::creature::CreatureSpec;
use crate::world::dialogue_schema::DialogueGraph;

// ── Published materials ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) enum PublishedMaterialHandle {
    Toon(Handle<ToonMaterial>),
    Water(Handle<WaterMaterial>),
    Energy(Handle<EnergyMaterial>),
    Shield(Handle<ShieldMaterial>),
    Ice(Handle<IceMaterial>),
    Lava(Handle<LavaMaterial>),
}

#[derive(Debug, Clone)]
pub(crate) struct PublishedMaterialEntry {
    // Stored at rebuild time, not yet read anywhere — scaffolding for a future
    // incremental-rebuild skip (unchanged material source hash => reuse the
    // existing GPU handle). Narrow this once that lands.
    #[allow(dead_code)]
    pub(crate) source_hash: String,
    pub(crate) handle: PublishedMaterialHandle,
}

/// Runtime lookup keyed by stable Forge content IDs. Bevy handles never enter
/// project files or save data.
#[derive(Resource, Default)]
pub struct PublishedMaterialCatalog {
    entries: BTreeMap<String, PublishedMaterialEntry>,
}

impl PublishedMaterialCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Authoring-only: `engine_tools::rebuild_published_content_catalogs`
    /// rebuilds this catalog wholesale from the live project on every load.
    pub(crate) fn replace(&mut self, entries: BTreeMap<String, PublishedMaterialEntry>) {
        self.entries = entries;
    }
}

// ── Published procedural recipes ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProceduralRecipeKind {
    Biome,
    Road,
    Building,
    Cave,
    City,
}

impl ProceduralRecipeKind {
    /// Authoring-only: called while rebuilding catalogs from a `ForgeProject`
    /// record, never by gameplay.
    pub(crate) fn from_category(
        category: crate::engine_tools::persistence::ContentCategory,
    ) -> Option<Self> {
        use crate::engine_tools::persistence::ContentCategory;
        match category {
            ContentCategory::Biome => Some(Self::Biome),
            ContentCategory::Road => Some(Self::Road),
            ContentCategory::Building => Some(Self::Building),
            ContentCategory::Cave => Some(Self::Cave),
            ContentCategory::City => Some(Self::City),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PublishedProceduralRecipeEntry {
    pub(crate) source_hash: String,
    pub(crate) kind: ProceduralRecipeKind,
    pub(crate) seed: u64,
    pub(crate) revision: u64,
    pub(crate) material_slots: BTreeMap<String, String>,
    pub(crate) map_points: Vec<PublishedMapPoint>,
}

/// A map annotation exported by a published World Kit recipe. Fast-travel
/// nodes are interactive in the in-game map; landmarks remain visible context.
/// Owned strings keep this interface safe for player-authored content.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedMapPoint {
    pub id: String,
    pub label: String,
    pub position: Vec3,
    pub kind: PublishedMapPointKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedMapPointKind {
    FastTravel,
    Landmark,
}

/// Published deterministic generator inputs keyed by stable Forge content ID.
/// The catalog contains source data only; generated ECS entities remain
/// disposable and can be rebuilt without changing authored identity.
#[derive(Resource, Default)]
pub struct PublishedProceduralRecipeCatalog {
    entries: BTreeMap<String, PublishedProceduralRecipeEntry>,
}

impl PublishedProceduralRecipeCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn material_id(&self, recipe_id: &str, slot: &str) -> Option<&str> {
        self.entries
            .get(recipe_id)?
            .material_slots
            .get(slot)
            .map(String::as_str)
    }

    pub fn generation(&self, recipe_id: &str) -> Option<(ProceduralRecipeKind, u64, u64)> {
        let entry = self.entries.get(recipe_id)?;
        Some((entry.kind, entry.seed, entry.revision))
    }

    pub fn source_hash(&self, recipe_id: &str) -> Option<&str> {
        self.entries
            .get(recipe_id)
            .map(|entry| entry.source_hash.as_str())
    }

    /// All published map annotations in stable content/node order.
    pub fn map_points(&self) -> impl Iterator<Item = &PublishedMapPoint> {
        self.entries
            .values()
            .flat_map(|entry| entry.map_points.iter())
    }

    /// Authoring-only: rebuilt wholesale from the live project on every load.
    pub(crate) fn replace(&mut self, entries: BTreeMap<String, PublishedProceduralRecipeEntry>) {
        self.entries = entries;
    }
}

// ── Published creatures and dialogue ────────────────────────────────────────

/// Published Creature Forge recipes, keyed by stable content id. Enemy
/// spawners and encounter content resolve creatures through this catalog so
/// gameplay only ever consumes published (hashed, validated) records.
#[derive(Resource, Default)]
pub struct PublishedCreatureCatalog {
    entries: BTreeMap<String, CreatureSpec>,
}

impl PublishedCreatureCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn get(&self, content_id: &str) -> Option<&CreatureSpec> {
        self.entries.get(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn content_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Seed the catalog from baked `assets/published/` content at startup.
    ///
    /// Only fills an empty catalog: when a Designer session opens a project,
    /// the workspace rebuild (which loads the *live* store, drafts included)
    /// remains the authority and must not be clobbered by stale baked files.
    pub fn seed_from_published(&mut self, creatures: impl IntoIterator<Item = CreatureSpec>) -> usize {
        if !self.entries.is_empty() {
            return 0;
        }
        for spec in creatures {
            self.entries.insert(spec.content_id.clone(), spec);
        }
        self.entries.len()
    }

    /// Authoring-only: rebuilt wholesale from the live project on every load.
    pub(crate) fn replace(&mut self, entries: BTreeMap<String, CreatureSpec>) {
        self.entries = entries;
    }
}

/// Published Dialogue Forge graphs keyed by stable content id. The runtime
/// dialogue director resolves conversations through this catalog so gameplay
/// only ever consumes published (validated) records, never project drafts.
#[derive(Resource, Default)]
pub struct PublishedDialogueCatalog {
    entries: BTreeMap<String, DialogueGraph>,
}

impl PublishedDialogueCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn get(&self, content_id: &str) -> Option<&DialogueGraph> {
        self.entries.get(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn content_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Seed the catalog from baked `assets/published/` content at startup.
    /// Like the creature catalog, this only fills an empty catalog so a live
    /// Designer workspace remains the authority.
    pub fn seed_from_published(
        &mut self,
        dialogues: impl IntoIterator<Item = (String, DialogueGraph)>,
    ) -> usize {
        if !self.entries.is_empty() {
            return 0;
        }
        for (content_id, graph) in dialogues {
            self.entries.insert(content_id, graph);
        }
        self.entries.len()
    }
}

// ── Procedural material resolution (gameplay system) ────────────────────────

/// Stable appearance request attached to disposable generated geometry.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ProceduralMaterialBinding {
    pub recipe_id: String,
    pub slot: String,
}

impl ProceduralMaterialBinding {
    pub fn new(recipe_id: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            recipe_id: recipe_id.into(),
            slot: slot.into(),
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProceduralMaterialBinding {
    epoch: u64,
    recipe_id: String,
    slot: String,
    material_id: Option<String>,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct ProceduralStandardMaterialFallback(pub(crate) Handle<StandardMaterial>);

#[derive(Resource, Default)]
pub(crate) struct PublishedContentCatalogEpoch(pub(crate) u64);

/// A manually-picked published material, set by the World Kit Inspector's
/// material picker. Procedural resolution clears this when a recipe slot
/// takes over the same object, so a stale manual pick never fights the
/// recipe that generated the object.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorMaterialBinding(pub(crate) String);

pub(crate) fn remove_mesh_material_components(entity: &mut EntityWorldMut<'_>) {
    entity.remove::<MeshMaterial3d<StandardMaterial>>();
    entity.remove::<MeshMaterial3d<ToonMaterial>>();
    entity.remove::<MeshMaterial3d<WaterMaterial>>();
    entity.remove::<MeshMaterial3d<EnergyMaterial>>();
    entity.remove::<MeshMaterial3d<ShieldMaterial>>();
    entity.remove::<MeshMaterial3d<IceMaterial>>();
    entity.remove::<MeshMaterial3d<LavaMaterial>>();
}

pub(crate) fn apply_catalog_material(world: &mut World, entity: Entity, content_id: &str) -> bool {
    if world.get::<Mesh3d>(entity).is_none() {
        return false;
    }
    let handle = world
        .resource::<PublishedMaterialCatalog>()
        .entries
        .get(content_id)
        .map(|entry| entry.handle.clone());
    let Some(handle) = handle else {
        return false;
    };
    let mut target = world.entity_mut(entity);
    remove_mesh_material_components(&mut target);
    match handle {
        PublishedMaterialHandle::Toon(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Water(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Energy(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Shield(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Ice(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Lava(handle) => target.insert(MeshMaterial3d(handle)),
    };
    target.insert(EditorMaterialBinding(content_id.into()));
    true
}

fn restore_procedural_standard_material(world: &mut World, entity: Entity) {
    let fallback = world
        .get::<ProceduralStandardMaterialFallback>(entity)
        .map(|fallback| fallback.0.clone());
    let Some(fallback) = fallback else {
        return;
    };
    let mut target = world.entity_mut(entity);
    remove_mesh_material_components(&mut target);
    target.insert(MeshMaterial3d(fallback));
}

pub(crate) fn resolve_procedural_material_bindings(world: &mut World) {
    let targets = world
        .query::<(
            Entity,
            &ProceduralMaterialBinding,
            Option<&ResolvedProceduralMaterialBinding>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        )>()
        .iter(world)
        .map(|(entity, binding, resolved, standard)| {
            (
                entity,
                binding.clone(),
                resolved.cloned(),
                standard.map(|material| material.0.clone()),
            )
        })
        .collect::<Vec<_>>();

    for (entity, binding, resolved, standard) in targets {
        let epoch = world.resource::<PublishedContentCatalogEpoch>().0;
        if resolved.as_ref().is_some_and(|current| {
            current.epoch == epoch
                && current.recipe_id == binding.recipe_id
                && current.slot == binding.slot
        }) {
            continue;
        }
        let desired = world
            .resource::<PublishedProceduralRecipeCatalog>()
            .entries
            .get(&binding.recipe_id)
            .and_then(|recipe| recipe.material_slots.get(&binding.slot).cloned());
        let Some(material_id) = desired else {
            if resolved
                .as_ref()
                .is_some_and(|current| current.material_id.is_some())
            {
                restore_procedural_standard_material(world, entity);
            }
            world
                .entity_mut(entity)
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: None,
                });
            continue;
        };
        if !world
            .resource::<PublishedMaterialCatalog>()
            .contains(&material_id)
        {
            if resolved
                .as_ref()
                .is_some_and(|current| current.material_id.is_some())
            {
                restore_procedural_standard_material(world, entity);
            }
            world
                .entity_mut(entity)
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: None,
                });
            continue;
        }
        if world
            .get::<ProceduralStandardMaterialFallback>(entity)
            .is_none()
        {
            if let Some(standard) = standard {
                world
                    .entity_mut(entity)
                    .insert(ProceduralStandardMaterialFallback(standard));
            }
        }
        if apply_catalog_material(world, entity, &material_id) {
            world
                .entity_mut(entity)
                .remove::<EditorMaterialBinding>()
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: Some(material_id),
                });
        }
    }
}
