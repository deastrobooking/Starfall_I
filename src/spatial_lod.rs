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
    /// Distance where the fade-out begins.
    pub fade_start: f32,
    /// Distance where the renderable is fully culled.
    pub max_distance: f32,
}

impl SpatialLod {
    pub fn new(fade_start: f32, max_distance: f32) -> Self {
        let max_distance = max_distance.max(1.0);
        Self {
            fade_start: fade_start.clamp(0.0, max_distance),
            max_distance,
        }
    }

    pub fn foliage() -> Self {
        Self::new(1_350.0, 1_550.0)
    }

    pub fn landmark(height: f32) -> Self {
        let max_distance = (2_200.0 + height.max(0.0) * 30.0).clamp(2_400.0, 6_000.0);
        Self::new(max_distance - 320.0, max_distance)
    }

    fn visibility_range(self) -> VisibilityRange {
        VisibilityRange {
            start_margin: 0.0..0.0,
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

pub struct SpatialLodPlugin;

impl Plugin for SpatialLodPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SpatialLod>().add_systems(
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
    children: Query<&Children>,
    renderables: Query<(), With<Mesh3d>>,
) {
    for (owner, profile) in &profiles {
        bind_renderable(&mut commands, owner, owner, *profile, &renderables);
        for descendant in children.iter_descendants(owner) {
            bind_renderable(&mut commands, descendant, owner, *profile, &renderables);
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
) {
    for owner in removed.read() {
        for (entity, binding) in &managed {
            if binding.owner == owner {
                commands
                    .entity(entity)
                    .remove::<(VisibilityRange, ManagedSpatialLod)>();
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
        assert_eq!(profile.fade_start, 100.0);
        assert_eq!(profile.max_distance, 100.0);

        let landmark = SpatialLod::landmark(500.0);
        assert!(landmark.max_distance <= 6_000.0);
        assert!(landmark.fade_start < landmark.max_distance);
    }

    #[test]
    fn hierarchy_profile_propagates_to_child_meshes() {
        let mut app = App::new();
        app.add_plugins(SpatialLodPlugin);
        let root = app.world_mut().spawn(SpatialLod::foliage()).id();
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
    fn removing_profile_clears_managed_visibility_range() {
        let mut app = App::new();
        app.add_plugins(SpatialLodPlugin);
        let entity = app
            .world_mut()
            .spawn((Mesh3d::default(), SpatialLod::foliage()))
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
