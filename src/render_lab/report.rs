use std::{collections::BTreeMap, fs, io::Write, path::Path};

use bevy::prelude::*;
use bevy::render::{
    renderer::{RenderAdapterInfo, RenderDevice},
    settings::WgpuFeatures,
};
use serde::{Deserialize, Serialize};

use super::probe_gpu::ProbeValidationReport;
use super::{LabConfig, SceneInventory};

/// Frame-time percentiles use the nearest-rank definition; values are milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleSummary {
    pub samples: usize,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl SampleSummary {
    pub(crate) fn from_samples(values: &[f64]) -> Option<Self> {
        if values.is_empty()
            || values
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let percentile = |percent: usize| sorted[(sorted.len() * percent).div_ceil(100) - 1];
        Some(Self {
            samples: sorted.len(),
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: percentile(50),
            p95: percentile(95),
            p99: percentile(99),
            max: sorted[sorted.len() - 1],
        })
    }
}

/// Actual enabled device features, not assumptions based on OS or GPU branding.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceReport {
    pub adapter: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub enabled_features: String,
    pub meshlet_required_features_available: bool,
    pub missing_meshlet_features: String,
    pub max_texture_dimension_3d: u32,
    pub max_storage_buffer_binding_size: u64,
    pub max_compute_workgroup_storage_size: u32,
    pub max_compute_invocations_per_workgroup: u32,
}

impl DeviceReport {
    pub(crate) fn capture(adapter: &RenderAdapterInfo, device: &RenderDevice) -> Self {
        // Matches the pinned Bevy 0.19.1 MeshletPlugin contract. This only reports
        // capability; the lab does not register that experimental renderer yet.
        let required = WgpuFeatures::TEXTURE_INT64_ATOMIC
            | WgpuFeatures::TEXTURE_ATOMIC
            | WgpuFeatures::SHADER_INT64
            | WgpuFeatures::SUBGROUP
            | WgpuFeatures::DEPTH_CLIP_CONTROL
            | WgpuFeatures::IMMEDIATES;
        let features = device.features();
        let limits = device.limits();
        Self {
            adapter: adapter.name.clone(),
            backend: format!("{:?}", adapter.backend),
            device_type: format!("{:?}", adapter.device_type),
            driver: adapter.driver.clone(),
            enabled_features: format!("{features:?}"),
            meshlet_required_features_available: features.contains(required),
            missing_meshlet_features: format!("{:?}", required.difference(features)),
            max_texture_dimension_3d: limits.max_texture_dimension_3d,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_compute_workgroup_storage_size: limits.max_compute_workgroup_storage_size,
            max_compute_invocations_per_workgroup: limits.max_compute_invocations_per_workgroup,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ViewReport {
    pub view: u32,
    pub viewport_origin: [u32; 2],
    pub viewport_size: [u32; 2],
    /// CPU visibility candidates, not GPU rasterized triangles or draw calls.
    pub visible_mesh_entities: Option<SampleSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LabReport {
    pub schema_version: u32,
    #[serde(default)]
    pub lab_source_fingerprint: String,
    pub engine_version: String,
    pub bevy_version: String,
    pub debug_assertions: bool,
    pub config: LabConfig,
    pub device: DeviceReport,
    pub scene_inventory: SceneInventory,
    pub renderer: String,
    pub present_mode: String,
    pub frame_time_ms: SampleSummary,
    pub views: Vec<ViewReport>,
    /// Independently sampled asynchronous render diagnostics. Do not sum these
    /// as frame time: parent/child spans can overlap and arrive on different frames.
    pub render_diagnostics_ms: BTreeMap<String, SampleSummary>,
    pub gpu_timestamps_observed: bool,
    #[serde(default)]
    pub gpu_timing_unavailable_reason: Option<String>,
    #[serde(default)]
    pub probe_validation: Option<ProbeValidationReport>,
}

impl LabReport {
    /// Exclusive creation keeps benchmark evidence from being silently replaced.
    pub fn write_new(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        write_new(path, &bytes)
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not create report {}: {error}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!(
            "Could not finish report {}: {error}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_preserve_stalls_and_reject_invalid_samples() {
        let values: Vec<_> = (1..=100).rev().map(f64::from).collect();
        let summary = SampleSummary::from_samples(&values).unwrap();
        assert_eq!(summary.mean, 50.5);
        assert_eq!(
            (summary.p50, summary.p95, summary.p99, summary.max),
            (50.0, 95.0, 99.0, 100.0)
        );
        for values in [vec![], vec![1.0, f64::NAN], vec![f64::INFINITY], vec![-1.0]] {
            assert!(SampleSummary::from_samples(&values).is_none());
        }
        assert_eq!(SampleSummary::from_samples(&[7.0]).unwrap().p99, 7.0);
    }

    #[test]
    fn report_creation_does_not_replace_existing_evidence() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("starfall-lab-{}-{nonce}.json", std::process::id()));
        write_new(&path, b"first").unwrap();
        assert!(write_new(&path, b"second").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"first");
        fs::remove_file(path).unwrap();
    }
}
