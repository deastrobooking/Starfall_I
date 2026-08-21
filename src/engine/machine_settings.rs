//! Machine-local editor and rendering preferences.
//!
//! Campaign, character, and player-control state continues to use Starfall's
//! versioned JSON save contract. These groups are intentionally limited to
//! presentation and Forge workflow choices that may differ per computer.

use bevy::prelude::*;
use bevy::settings::{ReflectSettingsGroup, SettingsGroup};

#[derive(Resource, SettingsGroup, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, SettingsGroup, Default)]
#[settings_group(file = "machine", group = "editor_preferences")]
pub struct EditorPreferences {
    pub semantic_labels_visible: bool,
    pub semantic_label_distance: f32,
    pub semantic_label_budget: u32,
    pub infinite_grid_visible: bool,
    pub infinite_grid_fade_distance: f32,
    pub translation_snap_index: u8,
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            semantic_labels_visible: true,
            semantic_label_distance: 260.0,
            semantic_label_budget: 72,
            infinite_grid_visible: true,
            infinite_grid_fade_distance: 600.0,
            translation_snap_index: 1,
        }
    }
}

impl EditorPreferences {
    pub const MIN_LABEL_DISTANCE: f32 = 80.0;
    pub const MAX_LABEL_DISTANCE: f32 = 800.0;
    pub const LABEL_DISTANCE_STEP: f32 = 40.0;
    pub const MIN_LABEL_BUDGET: u32 = 16;
    pub const MAX_LABEL_BUDGET: u32 = 256;
    pub const LABEL_BUDGET_STEP: u32 = 16;
    pub const TRANSLATION_SNAP_COUNT: u8 = 4;

    pub fn sanitize(&mut self) {
        if !self.semantic_label_distance.is_finite() {
            self.semantic_label_distance = Self::MAX_LABEL_DISTANCE;
        }
        if !self.infinite_grid_fade_distance.is_finite() {
            self.infinite_grid_fade_distance = Self::default().infinite_grid_fade_distance;
        }
        self.semantic_label_distance = self
            .semantic_label_distance
            .clamp(Self::MIN_LABEL_DISTANCE, Self::MAX_LABEL_DISTANCE);
        self.semantic_label_budget = self
            .semantic_label_budget
            .clamp(Self::MIN_LABEL_BUDGET, Self::MAX_LABEL_BUDGET);
        self.infinite_grid_fade_distance = self.infinite_grid_fade_distance.clamp(100.0, 2_000.0);
        self.translation_snap_index %= Self::TRANSLATION_SNAP_COUNT;
    }
}

#[derive(Resource, SettingsGroup, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, SettingsGroup, Default)]
#[settings_group(file = "machine", group = "render_quality")]
pub struct RenderQualitySettings {
    pub shadow_maps_enabled: bool,
    pub directional_shadow_map_size: u32,
    pub forge_contact_shadows: bool,
}

impl Default for RenderQualitySettings {
    fn default() -> Self {
        Self {
            shadow_maps_enabled: true,
            directional_shadow_map_size: 2_048,
            // Four gameplay viewports make screen-space effects expensive.
            // Contact shadows begin as a Forge-only, opt-in preview feature.
            forge_contact_shadows: false,
        }
    }
}

impl RenderQualitySettings {
    pub const SHADOW_MAP_SIZES: [u32; 3] = [1_024, 2_048, 4_096];

    pub fn sanitize(&mut self) {
        self.directional_shadow_map_size = *Self::SHADOW_MAP_SIZES
            .iter()
            .min_by_key(|candidate| candidate.abs_diff(self.directional_shadow_map_size))
            .expect("shadow-map size choices are non-empty");
    }

    pub fn cycle_shadow_map_size(&mut self) -> u32 {
        self.sanitize();
        let index = Self::SHADOW_MAP_SIZES
            .iter()
            .position(|size| *size == self.directional_shadow_map_size)
            .unwrap_or(1);
        self.directional_shadow_map_size =
            Self::SHADOW_MAP_SIZES[(index + 1) % Self::SHADOW_MAP_SIZES.len()];
        self.directional_shadow_map_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_preferences_sanitize_untrusted_disk_values() {
        let mut preferences = EditorPreferences {
            semantic_label_distance: f32::NAN,
            semantic_label_budget: 4_000,
            infinite_grid_fade_distance: 12.0,
            translation_snap_index: 17,
            ..default()
        };
        preferences.sanitize();

        assert_eq!(preferences.semantic_label_distance, 800.0);
        assert_eq!(preferences.semantic_label_budget, 256);
        assert_eq!(preferences.infinite_grid_fade_distance, 100.0);
        assert_eq!(preferences.translation_snap_index, 1);
    }

    #[test]
    fn render_quality_snaps_and_cycles_supported_shadow_sizes() {
        let mut quality = RenderQualitySettings {
            directional_shadow_map_size: 3_000,
            ..default()
        };
        quality.sanitize();
        assert_eq!(quality.directional_shadow_map_size, 2_048);
        assert_eq!(quality.cycle_shadow_map_size(), 4_096);
        assert_eq!(quality.cycle_shadow_map_size(), 1_024);
    }
}
