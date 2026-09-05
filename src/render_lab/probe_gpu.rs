//! One-shot GPU/CPU comparison before the lab warmup interval.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use bevy::{
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{Readback, ReadbackComplete},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{storage_buffer, storage_buffer_read_only},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems},
        storage::{GpuShaderBuffer, ShaderBuffer},
        Render, RenderApp, RenderSystems,
    },
};
use serde::{Deserialize, Serialize};

use super::probe_reference::{validation_fixture, ProbeRay};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeValidationReport {
    pub fixture: String,
    pub rays: usize,
    pub workgroups: u32,
    pub storage_bytes: u64,
    pub readback_bytes: u64,
    pub max_absolute_error: f32,
    pub mismatched_rays: usize,
    pub passed: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ProbeValidationState {
    pub report: Option<ProbeValidationReport>,
    pub error: Option<String>,
}

#[derive(Resource)]
struct ExpectedResults {
    values: Vec<[f32; 4]>,
    report: ProbeValidationReport,
}

#[derive(Clone, ShaderType)]
struct GpuRay {
    origin_distance: Vec4,
    direction: Vec4,
}

#[derive(ShaderType)]
struct ProbeParams {
    origin_cell_size: Vec4,
    dims_ray_count: UVec4,
}

#[derive(Resource, Clone, ExtractResource)]
struct ProbeInputs {
    voxels: Handle<ShaderBuffer>,
    rays: Handle<ShaderBuffer>,
    results: Handle<ShaderBuffer>,
    params: Handle<ShaderBuffer>,
    shader: Handle<Shader>,
    workgroups: u32,
    dispatched: Arc<AtomicBool>,
}

#[derive(Resource)]
struct ReadbackRequest {
    buffer: Handle<ShaderBuffer>,
    dispatched: Arc<AtomicBool>,
    entity: Option<Entity>,
    removed: bool,
}

pub(crate) struct ProbeValidationPlugin;

impl Plugin for ProbeValidationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<ProbeInputs>::default())
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
    device: Res<RenderDevice>,
    mut state: ResMut<ProbeValidationState>,
) {
    let result = (|| {
        let (scene, mut rays) = validation_fixture()?;
        // Include exactly zero direction axes and outside-volume entries in the
        // GPU fixture as well as the spherical probe directions.
        for (slot, (origin, direction, distance)) in [
            ([2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([6.0, 2.0, 2.0], [-1.0, 0.0, 0.0], 20.0),
            ([-2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([8.0, 2.0, 2.0], [-1.0, 0.0, 0.0], 20.0),
            ([2.0, 8.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([4.2, 2.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            rays[slot] = ProbeRay::new(origin, direction, distance)?;
        }
        let config = scene.config();
        let plan = config.allocation_plan()?;
        let limits = device.limits();
        if limits.max_storage_buffers_per_shader_stage < 4
            || limits.max_compute_invocations_per_workgroup < 64
            || limits.max_compute_workgroup_size_x < 64
            || limits.max_compute_workgroups_per_dimension < plan.workgroups
            || limits.max_storage_buffer_binding_size < u64::from(plan.voxel_count) * 16
            || limits.max_storage_buffer_binding_size < u64::from(plan.ray_count) * 32
        {
            return Err(
                "Device does not support the bounded probe validation dispatch".to_string(),
            );
        }
        let expected = rays.iter().map(|ray| scene.trace(*ray)).collect();
        let gpu_rays: Vec<_> = rays
            .iter()
            .map(|ray| GpuRay {
                origin_distance: ray.origin_and_distance().into(),
                direction: ray.direction().into(),
            })
            .collect();
        let cells: Vec<Vec4> = scene.voxels().iter().copied().map(Into::into).collect();
        let mut output = ShaderBuffer::from(vec![Vec4::splat(-2.0); rays.len()]);
        output.buffer_description.usage |= BufferUsages::COPY_SRC;
        let results = buffers.add(output);
        let dispatched = Arc::new(AtomicBool::new(false));
        commands.insert_resource(ReadbackRequest {
            buffer: results.clone(),
            dispatched: dispatched.clone(),
            entity: None,
            removed: false,
        });
        commands.insert_resource(ProbeInputs {
            voxels: buffers.add(ShaderBuffer::from(cells)),
            rays: buffers.add(ShaderBuffer::from(gpu_rays)),
            results,
            params: buffers.add(ShaderBuffer::from(ProbeParams {
                origin_cell_size: Vec4::new(
                    config.voxel_origin[0],
                    config.voxel_origin[1],
                    config.voxel_origin[2],
                    config.cell_size,
                ),
                dims_ray_count: UVec4::new(
                    config.voxel_dims[0],
                    config.voxel_dims[1],
                    config.voxel_dims[2],
                    plan.ray_count,
                ),
            })),
            shader: shaders.add(Shader::from_wgsl(
                include_str!("probe_trace.wgsl"),
                "starfall/probe_trace.wgsl",
            )),
            workgroups: plan.workgroups,
            dispatched,
        });
        commands.insert_resource(ExpectedResults {
            values: expected,
            report: ProbeValidationReport {
                fixture: "voxel_wall_v1".into(),
                rays: rays.len(),
                workgroups: plan.workgroups,
                storage_bytes: plan.storage_bytes,
                readback_bytes: plan.readback_bytes,
                max_absolute_error: 0.0,
                mismatched_rays: 0,
                passed: false,
            },
        });
        Ok(())
    })();
    if let Err(error) = result {
        state.error = Some(error);
    }
}

fn request_readback_once(
    mut commands: Commands,
    request: Option<ResMut<ReadbackRequest>>,
    state: Res<ProbeValidationState>,
) {
    if state.report.is_some() || state.error.is_some() {
        return;
    }
    let Some(mut request) = request else {
        return;
    };
    if let Some(entity) = request.entity {
        if !request.removed {
            // One extraction interval only. Keep the entity alive to receive
            // its asynchronous result, without allocating another staging copy.
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
    mut state: ResMut<ProbeValidationState>,
    mut commands: Commands,
) {
    if state.report.is_some() || state.error.is_some() {
        return;
    }
    let actual: Vec<Vec4> = event.to_shader_type();
    let mut report = expected.report.clone();
    if actual.len() != expected.values.len() {
        state.error = Some("GPU probe output length does not match the fixture".into());
        return;
    }
    for (actual, expected) in actual.iter().zip(&expected.values) {
        let mut mismatch = false;
        for (a, b) in actual.to_array().into_iter().zip(expected) {
            let error = (a - b).abs();
            if !error.is_finite() || error > 0.002 {
                mismatch = true;
            }
            if error.is_finite() {
                report.max_absolute_error = report.max_absolute_error.max(error);
            }
        }
        report.mismatched_rays += usize::from(mismatch);
    }
    report.passed = report.mismatched_rays == 0;
    println!(
        "GPU probe validation: {} rays, {} mismatches, max error {}",
        report.rays, report.mismatched_rays, report.max_absolute_error
    );
    state.report = Some(report);
    commands.entity(event.entity).despawn();
}

#[derive(Resource)]
struct ProbePipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
}
#[derive(Resource)]
struct ProbeBindGroup(BindGroup);

fn prepare_pipeline(
    mut commands: Commands,
    inputs: Option<Res<ProbeInputs>>,
    existing: Option<Res<ProbePipeline>>,
    cache: Res<PipelineCache>,
) {
    if existing.is_some() {
        return;
    }
    let Some(inputs) = inputs else {
        return;
    };
    let layout = BindGroupLayoutDescriptor::new(
        "probe validation",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer_read_only::<Vec<Vec4>>(false),
                storage_buffer_read_only::<Vec<GpuRay>>(false),
                storage_buffer::<Vec<Vec4>>(false),
                storage_buffer_read_only::<ProbeParams>(false),
            ),
        ),
    );
    let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("probe validation".into()),
        layout: vec![layout.clone()],
        shader: inputs.shader.clone(),
        entry_point: Some("trace_probes".into()),
        ..default()
    });
    commands.insert_resource(ProbePipeline { layout, id });
}

fn prepare_bind_group(
    mut commands: Commands,
    inputs: Option<Res<ProbeInputs>>,
    pipeline: Option<Res<ProbePipeline>>,
    existing: Option<Res<ProbeBindGroup>>,
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
    let (Some(voxels), Some(rays), Some(results), Some(params)) = (
        buffers.get(&inputs.voxels),
        buffers.get(&inputs.rays),
        buffers.get(&inputs.results),
        buffers.get(&inputs.params),
    ) else {
        return;
    };
    let group = device.create_bind_group(
        "probe validation",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            voxels.buffer.as_entire_buffer_binding(),
            rays.buffer.as_entire_buffer_binding(),
            results.buffer.as_entire_buffer_binding(),
            params.buffer.as_entire_buffer_binding(),
        )),
    );
    commands.insert_resource(ProbeBindGroup(group));
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    inputs: Option<Res<ProbeInputs>>,
    pipeline: Option<Res<ProbePipeline>>,
    group: Option<Res<ProbeBindGroup>>,
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
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("Starfall probe validation"),
            ..default()
        });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group.0, &[]);
    pass.dispatch_workgroups(inputs.workgroups, 1, 1);
    *submitted = true;
    inputs.dispatched.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readback_waits_for_dispatch_and_is_extracted_for_only_one_frame() {
        let dispatched = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.init_resource::<ProbeValidationState>()
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
        app.update();
        assert_eq!(
            app.world().resource::<ReadbackRequest>().entity,
            Some(entity)
        );
        assert!(app.world().get_entity(entity).is_ok());
        assert!(app.world().get::<Readback>(entity).is_none());
    }
}
