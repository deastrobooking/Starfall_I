//! Reusable hierarchical level-of-detail and distance-culling service.
//!
//! Bevy owns camera-aware visibility evaluation and dithered transitions.
//! Starfall owns authoring profiles, hierarchy propagation, and diagnostics.
//! This first slice is render-only: physics and gameplay entities remain live.

use bevy::{camera::visibility::VisibilityRange, prelude::*};

/// Authoring component placed on a renderable or hierarchy root.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct SpatialLod {
    /// Distance where the renderable begins fading into view.
    pub min_distance: f32,
    /// Distance where the renderable is fully visible after fading in.
    pub fade_in_end: f32,
    /// Distance where the renderable begins fading out.
    pub fade_start: f32,
    /// Distance where the renderable is fully culled.
    pub max_distance: f32,
}

impl SpatialLod {
    pub fn new(fade_start: f32, max_distance: f32) -> Self {
        let max_distance = max_distance.max(1.0);
        Self {
            min_distance: 0.0,
            fade_in_end: 0.0,
            fade_start: fade_start.clamp(0.0, max_distance),
            max_distance,
        }
    }

    /// Creates one band in a multi-mesh LOD group. Adjacent tiers should use
    /// the same `min_distance..fade_in_end` and `fade_start..max_distance`
    /// transition so Bevy can crossfade between them without popping.
    pub fn tier(min_distance: f32, fade_in_end: f32, fade_start: f32, max_distance: f32) -> Self {
        let max_distance = max_distance.max(1.0);
        let min_distance = min_distance.clamp(0.0, max_distance);
        let fade_in_end = fade_in_end.clamp(min_distance, max_distance);
        let fade_start = fade_start.clamp(fade_in_end, max_distance);
        Self {
            min_distance,
            fade_in_end,
            fade_start,
            max_distance,
        }
    }

    pub fn foliage_high() -> Self {
        Self::new(850.0, 1_050.0)
    }

    pub fn foliage_proxy() -> Self {
        Self::tier(850.0, 1_050.0, 1_350.0, 1_550.0)
    }

    pub fn landmark(height: f32) -> Self {
        let max_distance = (2_200.0 + height.max(0.0) * 30.0).clamp(2_400.0, 6_000.0);
        Self::new(max_distance - 320.0, max_distance)
    }

    fn visibility_range(self) -> VisibilityRange {
        VisibilityRange {
            start_margin: self.min_distance..self.fade_in_end,
            end_margin: self.fade_start..self.max_distance,
            use_aabb: false,
        }
    }
}

/// Renderable whose `VisibilityRange` is managed by this service.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSpatialLod {
    pub owner: Entity,
}

/// Identifies the inexpensive replacement mesh in a multi-tier LOD group.
#[derive(Component, Reflect, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct SpatialLodProxy;

pub struct SpatialLodPlugin;

impl Plugin for SpatialLodPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SpatialLod>()
            .register_type::<SpatialLodProxy>()
            .add_systems(
                Update,
                (
                    bind_changed_lod_hierarchies,
                    bind_new_lod_renderables,
                    clear_removed_lod_profiles,
                ),
            );
    }
}

fn bind_changed_lod_hierarchies(
    mut commands: Commands,
    profiles: Query<(Entity, &SpatialLod), Changed<SpatialLod>>,
    all_profiles: Query<&SpatialLod>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    renderables: Query<(), With<Mesh3d>>,
) {
    for (owner, profile) in &profiles {
        bind_renderable(&mut commands, owner, owner, *profile, &renderables);
        for descendant in children.iter_descendants(owner) {
            if nearest_profile(descendant, &all_profiles, &parents)
                .is_some_and(|(nearest_owner, _)| nearest_owner == owner)
            {
                bind_renderable(&mut commands, descendant, owner, *profile, &renderables);
            }
        }
    }
}

fn bind_new_lod_renderables(
    mut commands: Commands,
    renderables: Query<Entity, Added<Mesh3d>>,
    profiles: Query<&SpatialLod>,
    parents: Query<&ChildOf>,
) {
    for entity in &renderables {
        let Some((owner, profile)) = nearest_profile(entity, &profiles, &parents) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((profile.visibility_range(), ManagedSpatialLod { owner }));
    }
}

fn clear_removed_lod_profiles(
    mut commands: Commands,
    mut removed: RemovedComponents<SpatialLod>,
    managed: Query<(Entity, &ManagedSpatialLod)>,
    profiles: Query<&SpatialLod>,
    parents: Query<&ChildOf>,
) {
    for owner in removed.read() {
        for (entity, binding) in &managed {
            if binding.owner == owner {
                if let Some((fallback_owner, fallback)) =
                    nearest_profile(entity, &profiles, &parents)
                {
                    commands.entity(entity).insert((
                        fallback.visibility_range(),
                        ManagedSpatialLod {
                            owner: fallback_owner,
                        },
                    ));
                } else {
                    commands
                        .entity(entity)
                        .remove::<(VisibilityRange, ManagedSpatialLod)>();
                }
            }
        }
    }
}

fn bind_renderable(
    commands: &mut Commands,
    entity: Entity,
    owner: Entity,
    profile: SpatialLod,
    renderables: &Query<(), With<Mesh3d>>,
) {
    if renderables.contains(entity) {
        commands
            .entity(entity)
            .insert((profile.visibility_range(), ManagedSpatialLod { owner }));
    }
}

fn nearest_profile(
    entity: Entity,
    profiles: &Query<&SpatialLod>,
    parents: &Query<&ChildOf>,
) -> Option<(Entity, SpatialLod)> {
    let mut current = entity;
    for _ in 0..128 {
        if let Ok(profile) = profiles.get(current) {
            return Some((current, *profile));
        }
        current = parents.get(current).ok()?.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_profiles_are_ordered_and_bounded() {
        let profile = SpatialLod::new(500.0, 100.0);
        assert_eq!(profile.min_distance, 0.0);
        assert_eq!(profile.fade_in_end, 0.0);
        assert_eq!(profile.fade_start, 100.0);
        assert_eq!(profile.max_distance, 100.0);

        let landmark = SpatialLod::landmark(500.0);
        assert!(landmark.max_distance <= 6_000.0);
        assert!(landmark.fade_start < landmark.max_distance);

        let proxy = SpatialLod::tier(1_000.0, 900.0, 800.0, 700.0);
        assert_eq!(proxy.min_distance, 700.0);
        assert_eq!(proxy.fade_in_end, 700.0);
        assert_eq!(proxy.fade_start, 700.0);
        assert_eq!(proxy.max_distance, 700.0);
    }

    #[test]
    fn hierarchy_profile_propagates_to_child_meshes() {
        let mut app = App::new();
        app.add_plugins(SpatialLodPlugin);
        let root = app
            .world_mut()
            .spawn(SpatialLod::new(1_350.0, 1_550.0))
            .id();
        let child = app.world_mut().spawn(Mesh3d::default()).id();
        app.world_mut().entity_mut(root).add_child(child);

        app.update();
        app.update();

        let binding = app.world().get::<ManagedSpatialLod>(child).unwrap();
        let range = app.world().get::<VisibilityRange>(child).unwrap();
        assert_eq!(binding.owner, root);
        assert_eq!(range.end_margin, 1_350.0..1_550.0);
    }

    #[test]
    fn nested_tier_profile_overrides_hierarchy_profile() {
        let mut app = App::new();
        app.add_plugins(SpatialLodPlugin);
        let root = app.world_mut().spawn(SpatialLod::foliage_high()).id();
        let proxy = app
            .world_mut()
            .spawn((Mesh3d::default(), SpatialLod::foliage_proxy()))
            .id();
        app.world_mut().entity_mut(root).add_child(proxy);

        app.update();
        app.update();

        let range = app.world().get::<VisibilityRange>(proxy).unwrap();
        assert_eq!(range.start_margin, 850.0..1_050.0);
        assert_eq!(range.end_margin, 1_350.0..1_550.0);
        assert_eq!(
            app.world().get::<ManagedSpatialLod>(proxy).unwrap().owner,
            proxy
        );
    }

    #[test]
    fn removing_profile_clears_managed_visibility_range() {
        let mut app = App::new();
        app.add_plugins(SpatialLodPlugin);
        let entity = app
            .world_mut()
            .spawn((Mesh3d::default(), SpatialLod::new(1_350.0, 1_550.0)))
            .id();
        app.update();
        assert!(app.world().get::<VisibilityRange>(entity).is_some());

        app.world_mut().entity_mut(entity).remove::<SpatialLod>();
        app.update();
        app.update();

        assert!(app.world().get::<VisibilityRange>(entity).is_none());
        assert!(app.world().get::<ManagedSpatialLod>(entity).is_none());
    }
}
