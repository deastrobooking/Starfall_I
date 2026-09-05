//! Standalone rendering lab for reproducible multi-view experiments.
//!
//! This capability has no Heavy Water or Forge dependency. It compares stock
//! PBR with opt-in resident meshlets and validates bounded GPU probe traversal.

mod capture;
mod config;
mod geometry;
mod probe_gpu;
pub mod probe_reference;
mod report;
mod scene;

pub use config::{LabConfig, LabRenderer, LabScene, USAGE};
pub use geometry::GeometryBackendReport;
pub use probe_gpu::ProbeValidationReport;
pub use report::{DeviceReport, LabReport, SampleSummary, ViewReport};
pub use scene::SceneInventory;

use std::collections::BTreeMap;

use bevy::{
    app::AppExit,
    camera::visibility::VisibleEntities,
    diagnostic::DiagnosticsStore,
    platform::time::Instant,
    prelude::*,
    render::{
        diagnostic::RenderDiagnosticsPlugin,
        renderer::{RenderAdapterInfo, RenderDevice},
        RenderPlugin,
    },
    window::{PresentMode, WindowResolution},
    winit::WinitSettings,
};

use probe_gpu::{ProbeValidationPlugin, ProbeValidationState};
use scene::LabView;

#[derive(Default)]
struct DiagnosticSamples {
    last: Option<Instant>,
    values: Vec<f64>,
}

#[derive(Resource, Default)]
struct LabState {
    frame: u32,
    frame_times: Vec<f64>,
    diagnostics: BTreeMap<String, DiagnosticSamples>,
    visible: BTreeMap<u32, Vec<f64>>,
    finished: bool,
    validation_wait_frames: u32,
}

/// Launch an isolated renderer. The measured run exits after writing its report.
pub fn run(config: LabConfig) -> AppExit {
    if let Err(error) = config.validate() {
        eprintln!("{error}");
        return AppExit::error();
    }
    if config.output.exists() {
        eprintln!("Report already exists: {}", config.output.display());
        return AppExit::error();
    }
    if config.capture.as_ref().is_some_and(|path| path.exists()) {
        eprintln!("Capture already exists; choose a new filename");
        return AppExit::error();
    }
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!(
                        "Starfall Render Lab — {:?} — {} views",
                        config.scene, config.views
                    ),
                    resolution: WindowResolution::new(config.width, config.height)
                        .with_scale_factor_override(1.0),
                    present_mode: PresentMode::AutoNoVsync,
                    resizable: false,
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            }),
        RenderDiagnosticsPlugin,
        geometry::GeometryBackendPlugin,
    ))
    .insert_resource(WinitSettings::continuous())
    .init_resource::<ProbeValidationState>()
    .insert_resource(ClearColor(Color::srgb(0.035, 0.045, 0.06)))
    .init_resource::<LabState>()
    .add_systems(Startup, scene::setup)
    .add_systems(
        Update,
        (scene::animate, capture::capture_finished_run).chain(),
    )
    .add_systems(Last, collect);
    if config.validate_probes {
        app.add_plugins(ProbeValidationPlugin);
    }
    app.insert_resource(config);
    app.run()
}

fn collect(
    config: Res<LabConfig>,
    time: Res<Time<Real>>,
    diagnostics: Res<DiagnosticsStore>,
    adapter: Res<RenderAdapterInfo>,
    device: Res<RenderDevice>,
    inventory: Res<SceneInventory>,
    geometry: Res<GeometryBackendReport>,
    validation: Res<ProbeValidationState>,
    views: Query<(&LabView, &Camera, &VisibleEntities)>,
    mut state: ResMut<LabState>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.finished {
        return;
    }
    if config.validate_probes {
        if let Some(error) = &validation.error {
            error!("Probe validation failed: {error}");
            state.finished = true;
            exit.write(AppExit::error());
            return;
        }
        match &validation.report {
            Some(report) if !report.passed => {
                error!("GPU probe output disagrees with CPU reference");
                state.finished = true;
                exit.write(AppExit::error());
                return;
            }
            None => {
                state.validation_wait_frames += 1;
                if state.validation_wait_frames >= 600 {
                    error!("GPU probe validation timed out");
                    state.finished = true;
                    exit.write(AppExit::error());
                }
                return;
            }
            _ => {}
        }
    }
    let measuring = state.frame >= config.warmup_frames;
    for diagnostic in diagnostics.iter() {
        let path = diagnostic.path().to_string();
        // Bevy 0.19.1's encoder WriteTimestamp is a no-op on macOS, yet
        // elapsed_gpu diagnostics may still be published with zero values.
        if !usable_diagnostic(&path, cfg!(target_os = "macos")) {
            continue;
        }
        if !path.starts_with("render/")
            || !(path.ends_with("elapsed_cpu") || path.ends_with("elapsed_gpu"))
        {
            continue;
        }
        let Some(measurement) = diagnostic.measurement() else {
            continue;
        };
        let samples = state.diagnostics.entry(path).or_default();
        if samples.last != Some(measurement.time) {
            samples.last = Some(measurement.time);
            if measuring && measurement.value.is_finite() && measurement.value >= 0.0 {
                samples.values.push(measurement.value);
            }
        }
    }
    if measuring {
        state.frame_times.push(time.delta_secs_f64() * 1000.0);
        for (view, _, visible) in &views {
            state
                .visible
                .entry(view.0)
                .or_default()
                .push(visible.get(std::any::TypeId::of::<Mesh3d>()).len() as f64);
        }
    }
    state.frame += 1;
    if state.frame_times.len() < config.measured_frames as usize {
        return;
    }
    state.finished = true;
    let Some(frame_time_ms) = SampleSummary::from_samples(&state.frame_times) else {
        error!("Invalid frame measurements; report not written");
        exit.write(AppExit::error());
        return;
    };
    let layout = config::view_rects(config.views, UVec2::new(config.width, config.height));
    let mut view_reports: Vec<_> = views
        .iter()
        .filter_map(|(view, camera, _)| {
            let rect = camera.physical_viewport_rect()?;
            let tile = layout.get(view.0 as usize)?;
            if rect.size() != tile.size() {
                return None;
            }
            Some(ViewReport {
                view: view.0,
                viewport_origin: tile.min.to_array(),
                viewport_size: rect.size().to_array(),
                visible_mesh_entities: state
                    .visible
                    .get(&view.0)
                    .and_then(|values| SampleSummary::from_samples(values)),
            })
        })
        .collect();
    view_reports.sort_by_key(|view| view.view);
    if view_reports.len() != config.views as usize {
        error!("Not all requested views rendered; report not written");
        exit.write(AppExit::error());
        return;
    }
    let render_diagnostics_ms: BTreeMap<_, _> = state
        .diagnostics
        .iter()
        .filter_map(|(name, samples)| {
            Some((name.clone(), SampleSummary::from_samples(&samples.values)?))
        })
        .collect();
    let report = LabReport {
        schema_version: 1,
        lab_source_fingerprint: source_fingerprint(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        bevy_version: "0.19.1".into(),
        debug_assertions: cfg!(debug_assertions),
        config: config.clone(),
        device: DeviceReport::capture(&adapter, &device),
        scene_inventory: inventory.clone(),
        renderer: match geometry.active {
            LabRenderer::Pbr => "bevy_standard_pbr",
            LabRenderer::Meshlets => "bevy_meshlets_with_pbr_floor",
        }
        .into(),
        geometry_backend: Some(geometry.clone()),
        present_mode: "auto_no_vsync".into(),
        view_composition: "per_view_texture_then_ui_composite".into(),
        frame_time_ms,
        views: view_reports,
        gpu_timestamps_observed: render_diagnostics_ms
            .keys()
            .any(|path| path.ends_with("elapsed_gpu")),
        gpu_timing_unavailable_reason: if cfg!(target_os = "macos") {
            Some(
                "Bevy 0.19.1 disables encoder timestamp writes on macOS; GPU durations omitted"
                    .into(),
            )
        } else if !render_diagnostics_ms
            .keys()
            .any(|path| path.ends_with("elapsed_gpu"))
        {
            Some("No GPU timing observations were returned".into())
        } else {
            None
        },
        render_diagnostics_ms,
        probe_validation: validation.report.clone(),
    };
    match report.write_new(&config.output) {
        Ok(()) => {
            println!(
                "Report {}: {} views, {:.2} ms mean / {:.2} ms p95 / {:.2} ms p99",
                config.output.display(),
                config.views,
                report.frame_time_ms.mean,
                report.frame_time_ms.p95,
                report.frame_time_ms.p99
            );
            if config.capture.is_none() {
                exit.write(AppExit::Success);
            }
        }
        Err(error) => {
            error!("{error}");
            exit.write(AppExit::error());
        }
    }
}

fn usable_diagnostic(path: &str, macos: bool) -> bool {
    !(macos && path.ends_with("elapsed_gpu"))
}

// A build-time-source identity, not a cryptographic integrity check. Reports
// remain attributable even while the local working tree is uncommitted.
fn source_fingerprint() -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for source in [
        include_str!("mod.rs"),
        include_str!("config.rs"),
        include_str!("capture.rs"),
        include_str!("report.rs"),
        include_str!("scene.rs"),
        include_str!("geometry.rs"),
        include_str!("probe_reference.rs"),
        include_str!("probe_gpu.rs"),
        include_str!("probe_trace.wgsl"),
    ] {
        for byte in source.bytes().chain(std::iter::once(0)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_macos_timestamp_writes_cannot_be_reported_as_zero_gpu_cost() {
        assert!(!usable_diagnostic("render/clustering/elapsed_gpu", true));
        assert!(usable_diagnostic("render/clustering/elapsed_cpu", true));
        assert!(usable_diagnostic("render/clustering/elapsed_gpu", false));
    }
}
