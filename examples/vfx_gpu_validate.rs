//! One-shot GPU/CPU cross-validation for the VFX Phase 2 update-module
//! kernels (`docs/guides/vfx-forge-plan.md`). Dispatches the compiled WGSL
//! kernel for gravity+drag+curl_noise against a fixture of particles, reads
//! the result back, and compares it to the exact CPU reference the live
//! game runs. Prints a report and exits non-zero on any mismatch or error —
//! mirrors `render_lab --validate-probes`'s shape for the same reason.
//!
//! Run with: `cargo run --example vfx_gpu_validate --features heavy-water-demo`

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::window::{PresentMode, WindowResolution};

use starfall_i::engine::vfx_gpu::{VfxGpuValidationPlugin, VfxGpuValidationState};

fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Starfall VFX GPU Validation".into(),
                    resolution: WindowResolution::new(320, 240),
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
    )
    .add_plugins(VfxGpuValidationPlugin)
    .init_resource::<WaitState>()
    .add_systems(Last, check_done);
    app.run()
}

#[derive(Resource, Default)]
struct WaitState {
    frames: u32,
}

fn check_done(
    state: Res<VfxGpuValidationState>,
    mut wait: ResMut<WaitState>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(error) = &state.error {
        eprintln!("VFX GPU validation error: {error}");
        exit.write(AppExit::error());
        return;
    }
    if let Some(report) = &state.report {
        println!(
            "cache_key={} particles={} mismatches={} max_error={} passed={}",
            report.cache_key,
            report.particles,
            report.mismatched_particles,
            report.max_absolute_error,
            report.passed
        );
        exit.write(if report.passed {
            AppExit::Success
        } else {
            AppExit::error()
        });
        return;
    }
    wait.frames += 1;
    if wait.frames >= 600 {
        eprintln!("VFX GPU validation timed out waiting for a readback");
        exit.write(AppExit::error());
    }
}
