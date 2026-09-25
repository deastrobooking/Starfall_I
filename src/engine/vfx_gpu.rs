//! GPU compute validation for the Phase 2 update-module kernels
//! (`docs/guides/vfx-forge-plan.md`'s Phase 2 section).
//!
//! `starfall_vfx_graph::compile_update_kernel` turns an emitter's update
//! modules into one WGSL compute shader — this module dispatches that
//! shader once against a fixture of particles, reads the result back, and
//! cross-checks it against the exact CPU reference the live game runs
//! (`vfx::apply_update_module`). This is the same "prove a compute kernel
//! is numerically correct before anything depends on it" pattern
//! `render_lab::probe_gpu` already established in this codebase, applied to
//! particle motion instead of probe ray tracing — see that module first if
//! you're extending this one, since the shape is deliberately identical:
//! `ExtractResourcePlugin` brings CPU-built inputs into the render world,
//! `prepare_pipeline`/`prepare_bind_group` build the compute pipeline once
//! the render-asset buffers exist, `dispatch` runs it from the render
//! graph, and a one-shot `Readback` + observer reads the result back.
//!
//! **What this proves:** the WGSL translation of gravity/drag/curl_noise
//! matches the CPU implementation within floating-point tolerance, on real
//! GPU hardware (Metal on this machine), for a real dispatched compute
//! pass. **What this does not do yet:** run continuously as part of the
//! live game, spawn/despawn particles on the GPU, or render anything —
//! that is steady-state, double-buffered simulation plus an indirect-draw
//! renderer, a materially different (and harder) real-time systems problem
//! than a one-shot correctness proof. See the plan doc for what's next.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{storage_buffer, storage_buffer_read_only};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems};
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::{Render, RenderApp, RenderSystems};

use starfall_vfx_graph::{compile_update_kernel, CompiledModule, GpuModuleRegistry, VfxParam};

use super::vfx::apply_update_module;

/// Result of one GPU/CPU cross-validation run. `passed` is the single fact
/// anything driving this headlessly (a CLI report, a future CI gate) needs;
/// the rest is diagnostic detail for a human reading the printed report.
#[derive(Debug, Clone)]
pub struct VfxGpuValidationReport {
    pub cache_key: String,
    pub particles: usize,
    pub max_absolute_error: f32,
    pub mismatched_particles: usize,
    pub passed: bool,
}

#[derive(Resource, Default)]
pub struct VfxGpuValidationState {
    pub report: Option<VfxGpuValidationReport>,
    pub error: Option<String>,
}

/// One fixture particle's starting state, chosen to exercise the shipped
/// kernels realistically: nonzero position and velocity on every axis, and
/// an age comfortably inside the lifetime so the alive/dead branch this
/// pass also computes never fires — that branch is exact integer/float
/// comparison (`age < lifetime`), not floating-point module math, and
/// isn't what this validation exists to prove.
#[derive(Clone, Copy)]
struct Fixture {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    lifetime: f32,
}

fn fixture_particles() -> Vec<Fixture> {
    vec![
        Fixture {
            position: Vec3::new(0.0, 0.0, 0.0),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            age: 0.0,
            lifetime: 5.0,
        },
        Fixture {
            position: Vec3::new(2.0, 1.0, -1.0),
            velocity: Vec3::new(-0.5, 2.0, 0.5),
            age: 0.6,
            lifetime: 5.0,
        },
        Fixture {
            position: Vec3::new(-3.0, 4.0, 2.0),
            velocity: Vec3::new(0.0, -1.0, 3.0),
            age: 1.2,
            lifetime: 5.0,
        },
        Fixture {
            position: Vec3::new(5.0, -2.0, 0.5),
            velocity: Vec3::new(2.5, 0.2, -2.0),
            age: 2.4,
            lifetime: 5.0,
        },
        Fixture {
            position: Vec3::new(0.1, 0.1, 0.1),
            velocity: Vec3::ZERO,
            age: 0.0,
            lifetime: 5.0,
        },
        Fixture {
            position: Vec3::new(-1.0, -1.0, -1.0),
            velocity: Vec3::new(-4.0, -4.0, -4.0),
            age: 3.9,
            lifetime: 5.0,
        },
    ]
}

/// The update-module chain this validation exercises: all three GPU-covered
/// kinds together, with the realistic values the shipped `impact_spark` /
/// `ember_torch` systems use for gravity/drag and curl_noise respectively —
/// not literally either shipped system, since the point here is coverage of
/// every kernel, not reproducing one specific effect.
fn fixture_update_modules() -> Vec<CompiledModule> {
    vec![
        CompiledModule {
            kind: "gravity",
            params: vec![("strength", VfxParam::Float(11.0))],
        },
        CompiledModule {
            kind: "drag",
            params: vec![("coefficient", VfxParam::Float(0.8))],
        },
        CompiledModule {
            kind: "curl_noise",
            params: vec![
                ("strength", VfxParam::Float(0.8)),
                ("scale", VfxParam::Float(1.0)),
            ],
        },
    ]
}

const VALIDATION_DT: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, ShaderType)]
struct GpuParticle {
    position: Vec4,
    velocity: Vec4,
    age_lifetime: Vec4,
}

#[derive(Clone, Copy, ShaderType)]
struct GpuSimParams {
    dt: Vec4,
}

#[derive(Resource, Clone, ExtractResource)]
struct VfxGpuInputs {
    particles: Handle<ShaderBuffer>,
    module_params: Handle<ShaderBuffer>,
    sim_params: Handle<ShaderBuffer>,
    shader: Handle<Shader>,
    particle_count: u32,
    dispatched: Arc<AtomicBool>,
}

#[derive(Resource)]
struct ReadbackRequest {
    buffer: Handle<ShaderBuffer>,
    dispatched: Arc<AtomicBool>,
    entity: Option<Entity>,
    removed: bool,
}

#[derive(Resource)]
struct ExpectedResults {
    velocities: Vec<Vec3>,
    cache_key: String,
}

/// Not part of the live game's plugin set — only a validation entry point
/// (`examples/vfx_gpu_validate.rs`) adds this. Public because an example is
/// a separate compilation unit that depends on this crate the same way an
/// external consumer would.
pub struct VfxGpuValidationPlugin;

impl Plugin for VfxGpuValidationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VfxGpuValidationState>()
            .add_plugins(ExtractResourcePlugin::<VfxGpuInputs>::default())
            .add_systems(Startup, setup)
            .add_systems(Update, request_readback_once);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(
                    Render,
                    (prepare_pipeline, prepare_bind_group)
                        .chain()
                        .in_set(RenderSystems::PrepareBindGroups),
                )
                .add_systems(RenderGraph, dispatch.in_set(RenderGraphSystems::Render));
        }
    }
}

fn setup(
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut state: ResMut<VfxGpuValidationState>,
) {
    let registry = GpuModuleRegistry::builtin();
    let update_modules = fixture_update_modules();
    let kernel = match compile_update_kernel(&registry, &update_modules) {
        Ok(kernel) => kernel,
        Err(error) => {
            state.error = Some(format!("VFX GPU kernel failed to compile: {error}"));
            return;
        }
    };

    let fixtures = fixture_particles();
    let expected_velocities = fixtures
        .iter()
        .map(|fixture| {
            let mut velocity = fixture.velocity;
            let age_after = fixture.age + VALIDATION_DT;
            for module in &update_modules {
                velocity = apply_update_module(module, velocity, age_after, VALIDATION_DT);
            }
            velocity
        })
        .collect();

    let gpu_particles: Vec<GpuParticle> = fixtures
        .iter()
        .map(|fixture| GpuParticle {
            position: fixture.position.extend(0.0),
            velocity: fixture.velocity.extend(0.0),
            age_lifetime: Vec4::new(fixture.age, fixture.lifetime, 1.0, 0.0),
        })
        .collect();
    let particle_count = gpu_particles.len() as u32;

    let mut particle_buffer = ShaderBuffer::from(gpu_particles);
    particle_buffer.buffer_description.usage |= BufferUsages::COPY_SRC;
    let particles = buffers.add(particle_buffer);

    let module_params_data: Vec<Vec4> = kernel
        .module_params
        .iter()
        .map(|packed| Vec4::from_array(*packed))
        .collect();
    let module_params = buffers.add(ShaderBuffer::from(module_params_data));

    let sim_params = buffers.add(ShaderBuffer::from(GpuSimParams {
        dt: Vec4::new(VALIDATION_DT, 0.0, 0.0, 0.0),
    }));

    let dispatched = Arc::new(AtomicBool::new(false));
    commands.insert_resource(ReadbackRequest {
        buffer: particles.clone(),
        dispatched: dispatched.clone(),
        entity: None,
        removed: false,
    });
    commands.insert_resource(VfxGpuInputs {
        particles,
        module_params,
        sim_params,
        shader: shaders.add(Shader::from_wgsl(
            kernel.wgsl_source.clone(),
            "starfall/vfx_update.wgsl",
        )),
        particle_count,
        dispatched,
    });
    commands.insert_resource(ExpectedResults {
        velocities: expected_velocities,
        cache_key: kernel.cache_key,
    });
}

fn request_readback_once(
    mut commands: Commands,
    request: Option<ResMut<ReadbackRequest>>,
    state: Res<VfxGpuValidationState>,
) {
    if state.report.is_some() || state.error.is_some() {
        return;
    }
    let Some(mut request) = request else {
        return;
    };
    if let Some(entity) = request.entity {
        if !request.removed {
            commands.entity(entity).remove::<Readback>();
            request.removed = true;
        }
    } else if request.dispatched.load(Ordering::Acquire) {
        request.entity = Some(
            commands
                .spawn(Readback::buffer(request.buffer.clone()))
                .observe(check_results)
                .id(),
        );
    }
}

fn check_results(
    event: On<ReadbackComplete>,
    expected: Res<ExpectedResults>,
    mut state: ResMut<VfxGpuValidationState>,
    mut commands: Commands,
) {
    if state.report.is_some() || state.error.is_some() {
        return;
    }
    let actual: Vec<GpuParticle> = event.to_shader_type();
    if actual.len() != expected.velocities.len() {
        state.error = Some("VFX GPU particle count does not match the fixture".into());
        return;
    }
    let mut max_absolute_error = 0.0f32;
    let mut mismatched_particles = 0usize;
    for (particle, expected_velocity) in actual.iter().zip(&expected.velocities) {
        let actual_velocity = particle.velocity.truncate();
        let mut mismatch = false;
        for (a, b) in actual_velocity
            .to_array()
            .into_iter()
            .zip(expected_velocity.to_array())
        {
            let error = (a - b).abs();
            if !error.is_finite() || error > 0.002 {
                mismatch = true;
            }
            if error.is_finite() {
                max_absolute_error = max_absolute_error.max(error);
            }
        }
        mismatched_particles += usize::from(mismatch);
    }
    let report = VfxGpuValidationReport {
        cache_key: expected.cache_key.clone(),
        particles: actual.len(),
        max_absolute_error,
        mismatched_particles,
        passed: mismatched_particles == 0,
    };
    println!(
        "VFX GPU kernel validation ({}): {} particles, {} mismatches, max error {}",
        report.cache_key, report.particles, report.mismatched_particles, report.max_absolute_error
    );
    state.report = Some(report);
    commands.entity(event.entity).despawn();
}

#[derive(Resource)]
struct VfxGpuPipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
}

#[derive(Resource)]
struct VfxGpuBindGroup(BindGroup);

fn prepare_pipeline(
    mut commands: Commands,
    inputs: Option<Res<VfxGpuInputs>>,
    existing: Option<Res<VfxGpuPipeline>>,
    cache: Res<PipelineCache>,
) {
    if existing.is_some() {
        return;
    }
    let Some(inputs) = inputs else {
        return;
    };
    let layout = BindGroupLayoutDescriptor::new(
        "vfx gpu validation",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer::<Vec<GpuParticle>>(false),
                storage_buffer_read_only::<Vec<Vec4>>(false),
                storage_buffer_read_only::<GpuSimParams>(false),
            ),
        ),
    );
    let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("vfx gpu validation".into()),
        layout: vec![layout.clone()],
        shader: inputs.shader.clone(),
        entry_point: Some("update_particles".into()),
        ..default()
    });
    commands.insert_resource(VfxGpuPipeline { layout, id });
}

fn prepare_bind_group(
    mut commands: Commands,
    inputs: Option<Res<VfxGpuInputs>>,
    pipeline: Option<Res<VfxGpuPipeline>>,
    existing: Option<Res<VfxGpuBindGroup>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(inputs), Some(pipeline)) = (inputs, pipeline) else {
        return;
    };
    let (Some(particles), Some(module_params), Some(sim_params)) = (
        buffers.get(&inputs.particles),
        buffers.get(&inputs.module_params),
        buffers.get(&inputs.sim_params),
    ) else {
        return;
    };
    let group = device.create_bind_group(
        "vfx gpu validation",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            particles.buffer.as_entire_buffer_binding(),
            module_params.buffer.as_entire_buffer_binding(),
            sim_params.buffer.as_entire_buffer_binding(),
        )),
    );
    commands.insert_resource(VfxGpuBindGroup(group));
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    inputs: Option<Res<VfxGpuInputs>>,
    pipeline: Option<Res<VfxGpuPipeline>>,
    group: Option<Res<VfxGpuBindGroup>>,
    mut submitted: Local<bool>,
) {
    if *submitted {
        return;
    }
    let (Some(inputs), Some(pipeline), Some(group)) = (inputs, pipeline, group) else {
        return;
    };
    let Some(pipeline) = cache.get_compute_pipeline(pipeline.id) else {
        return;
    };
    let workgroups = inputs.particle_count.div_ceil(64).max(1);
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("Starfall VFX GPU validation"),
            ..default()
        });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group.0, &[]);
    pass.dispatch_workgroups(workgroups, 1, 1);
    *submitted = true;
    inputs.dispatched.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_module_chain_covers_every_gpu_kernel() {
        let registry = GpuModuleRegistry::builtin();
        let modules = fixture_update_modules();
        let kernel = compile_update_kernel(&registry, &modules).expect("compiles");
        assert_eq!(kernel.cache_key, "gravity+drag+curl_noise");
    }

    #[test]
    fn readback_waits_for_dispatch_and_is_extracted_for_only_one_frame() {
        let dispatched = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.init_resource::<VfxGpuValidationState>()
            .insert_resource(ReadbackRequest {
                buffer: Handle::default(),
                dispatched: dispatched.clone(),
                entity: None,
                removed: false,
            })
            .add_systems(Update, request_readback_once);
        app.update();
        assert!(app.world().resource::<ReadbackRequest>().entity.is_none());
        dispatched.store(true, Ordering::Release);
        app.update();
        let entity = app.world().resource::<ReadbackRequest>().entity.unwrap();
        assert!(app.world().get::<Readback>(entity).is_some());
        app.update();
        assert!(app.world().get::<Readback>(entity).is_none());
    }
}
