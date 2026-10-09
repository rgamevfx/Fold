//! Worker-owned offscreen spatial adapter. Bevy is reconstructible rendering
//! state; Fold owns immutable inputs, stable IDs, rational time and frame bindings.
//!
//! Initial integration: triangle meshes, positive TRS instances, perspective
//! or orthographic camera and unshadowed directional lights. Packed cloners, spatial text,
//! picking, and accounting for Bevy's internal allocations remain unqualified.
//! Host accounting covers output/validation images, not all Bevy allocations.
//! Stored RGB at half-float saturation is rejected, including exact ±65504.
//! GPU statistics cover Fold passes; Bevy internal pass costs are not yet measured.
mod depth;
use super::{GpuFrame, Host, Statistics, host, precision, validation::Completion};
pub use crate::spatial::*;
use b::IntoScheduleConfigs;
use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, ManualTextureViewHandle, RenderTarget},
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    prelude as b,
    render::{
        RenderApp, RenderPlugin,
        mesh::RenderMesh,
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache, PrimitiveTopology},
        renderer::{
            RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
            WgpuWrapper,
        },
        settings::RenderCreation,
        texture::{ManualTextureView, ManualTextureViews},
        view::{Hdr, ViewDepthTexture},
    },
    time::TimeUpdateStrategy,
};
pub use depth::Depth;
#[derive(b::Component)]
struct ExactTransform(b::Mat4);
fn exact_transforms(
    mut entities: b::Query<(&ExactTransform, &mut b::GlobalTransform), b::Changed<ExactTransform>>,
) {
    for (matrix, mut global) in &mut entities {
        *global = b::GlobalTransform::from(matrix.0);
    }
}
use fold_foundation::ObjectId;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

const TARGET: ManualTextureViewHandle = ManualTextureViewHandle(1);

// Register retention even on cancellation, preparation failure, or unwinding:
// Bevy may have submitted before returning from an update.
struct SubmissionLease {
    host: Host,
    image: Arc<host::Image>,
}
impl Drop for SubmissionLease {
    fn drop(&mut self) {
        let _ = Completion::submitted(self.host.queue(), self.image.clone());
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Preparation {
    pub meshes: usize,
    pub materials: usize,
    pub spawned_instances: usize,
    pub updated_instances: usize,
    pub removed_instances: usize,
    pub updates: usize,
}

/// Scene image can only be borrowed against the exact requested revision,
/// camera, time, duration and dimensions. Publication still checks current intent.
pub struct Frame {
    binding: Binding,
    image: GpuFrame,
    depth: Arc<host::Image>,
    near: f32,
    far: f32,
    projection: Projection,
    pub preparation: Preparation,
}
impl Frame {
    pub(super) fn into_image(self, expected: &Binding) -> Result<GpuFrame, String> {
        self.image(expected)?;
        Ok(self.image)
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn image(&self, expected: &Binding) -> Result<&GpuFrame, String> {
        if &self.binding != expected {
            return Err("Spatial frame does not match request".into());
        }
        Ok(&self.image)
    }
    pub fn readback(
        &mut self,
        expected: &Binding,
        cancel: &fold_media::Cancel,
    ) -> Result<crate::Frame, String> {
        self.image(expected)?;
        self.image.readback(cancel)
    }
}

/// Construct and use off the UI thread. One adapter retains one render world;
/// device loss stops it and recovery constructs a new adapter on a new Host.
pub struct Adapter {
    app: b::App,
    host: Host,
    camera: b::Entity,
    meshes: BTreeMap<ObjectId, (Arc<Mesh>, b::Handle<b::Mesh>)>,
    materials: BTreeMap<ObjectId, (Material, b::Handle<b::StandardMaterial>)>,
    instances: BTreeMap<ObjectId, (Instance, b::Entity)>,
    lights: BTreeMap<ObjectId, b::Entity>,
    precision: Option<(bool, wgpu::ComputePipeline)>,
}
impl Adapter {
    pub fn new(host: Host) -> Result<Self, String> {
        host.check()?;
        if host.adapter().get_info().backend != wgpu::Backend::Vulkan
            || !wgpu::Limits::default().check_limits(&host.device().limits())
        {
            return Err(
                "Spatial adapter requires the qualified Vulkan/default-limits device profile"
                    .into(),
            );
        }
        let creation = RenderCreation::manual(
            RenderDevice::from(host.device().clone()),
            RenderQueue(Arc::new(WgpuWrapper::new(host.queue().clone()))),
            RenderAdapterInfo(WgpuWrapper::new(host.adapter().get_info())),
            RenderAdapter(Arc::new(WgpuWrapper::new(host.adapter().clone()))),
            RenderInstance(Arc::new(WgpuWrapper::new(host.instance().clone()))),
        );
        let mut app = b::App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            bevy::diagnostic::FrameCountPlugin,
            bevy::diagnostic::DiagnosticsPlugin,
            bevy::time::TimePlugin,
            bevy::transform::TransformPlugin,
            bevy::asset::AssetPlugin::default(),
            bevy::window::WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..Default::default()
            },
            RenderPlugin {
                render_creation: creation,
                synchronous_pipeline_compilation: true,
                ..Default::default()
            },
            bevy::image::ImagePlugin::default(),
            bevy::mesh::MeshPlugin,
            bevy::camera::CameraPlugin,
            bevy::light::LightPlugin,
            bevy::core_pipeline::CorePipelinePlugin,
            bevy::pbr::PbrPlugin::default(),
        ));
        app.add_systems(
            b::PostUpdate,
            exact_transforms
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        app.finish();
        app.cleanup();
        app.world_mut()
            .resource_mut::<b::GlobalAmbientLight>()
            .brightness = 0.;
        let camera = app
            .world_mut()
            .spawn((
                b::Camera3d {
                    depth_texture_usages: (wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC)
                        .into(),
                    ..Default::default()
                },
                b::Camera {
                    clear_color: b::ClearColorConfig::Custom(b::Color::NONE),
                    ..Default::default()
                },
                RenderTarget::TextureView(TARGET),
                Hdr,
                b::Msaa::Off,
                Tonemapping::None,
                DebandDither::Disabled,
            ))
            .id();
        host.check()?;
        Ok(Self {
            app,
            host,
            camera,
            meshes: BTreeMap::new(),
            materials: BTreeMap::new(),
            instances: BTreeMap::new(),
            lights: BTreeMap::new(),
            precision: None,
        })
    }

    /// Updates retained resources and submits a bound, immutable RGBA16F frame.
    /// Two preparation updates settle extraction and pipeline creation. There
    /// are no history-dependent effects and both updates use the same frozen time.
    pub fn render(
        &mut self,
        scene: &Scene,
        binding: Binding,
        cancel: &fold_media::Cancel,
    ) -> Result<Frame, String> {
        cancel.check()?;
        self.host.check()?;
        scene.validate(&binding)?;
        let begin = std::time::Instant::now();
        // Reserve the output before mutating retained state. Images held by old
        // frames or in-flight submissions cannot be selected for reuse.
        let image = self
            .host
            .image_format(binding.dimensions, wgpu::TextureFormat::Rgba16Float)?;
        let depth = self
            .host
            .image_format(binding.dimensions, wgpu::TextureFormat::Depth32Float)?;
        let _submission = SubmissionLease {
            host: self.host.clone(),
            image: image.clone(),
        };
        let mut preparation = self.prepare(scene);
        self.app
            .world_mut()
            .resource_mut::<ManualTextureViews>()
            .insert(
                TARGET,
                ManualTextureView {
                    texture_view: image.view.clone().into(),
                    size: b::UVec2::from(binding.dimensions),
                    view_format: wgpu::TextureFormat::Rgba16Float,
                },
            );
        for _ in 0..2 {
            cancel.check()?;
            {
                // Bevy uses validation scopes on this same device. Synchronous
                // compilation keeps those scopes within this shared lock.
                let _compiler = super::shader::COMPILE_LOCK
                    .lock()
                    .map_err(|_| "Shader compiler lock poisoned")?;
                self.app.update();
            }
            preparation.updates += 1;
            self.host.check()?;
        }
        let render_world = self.app.sub_app(RenderApp).world();
        for pipeline in render_world.resource::<PipelineCache>().pipelines() {
            match &pipeline.state {
                CachedPipelineState::Ok(_) => {}
                CachedPipelineState::Err(error) => {
                    return Err(format!("Spatial pipeline failed: {error}"));
                }
                _ => return Err("Spatial pipelines are still preparing; retry this request".into()),
            }
        }
        let assets = render_world.resource::<RenderAssets<RenderMesh>>();
        if self
            .meshes
            .values()
            .any(|(_, handle)| assets.get(handle.id()).is_none())
        {
            return Err("Spatial meshes are still preparing; retry this request".into());
        }
        // Copy before the next render can reuse Bevy's depth attachment. Both
        // copies and later render passes execute in order on this same queue.
        let source_depth = {
            let world = self.app.sub_app_mut(RenderApp).world_mut();
            let mut query = world.query::<&ViewDepthTexture>();
            query
                .single(world)
                .map_err(|_| "Spatial view depth is unavailable")?
                .texture
                .clone()
        };
        if source_depth.format() != wgpu::TextureFormat::Depth32Float
            || source_depth.size() != depth.texture.size()
            || source_depth.sample_count() != 1
        {
            return Err("Unsupported spatial depth attachment".into());
        }
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::DepthOnly,
                ..source_depth.as_image_copy()
            },
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::DepthOnly,
                ..depth.texture.as_image_copy()
            },
            depth.texture.size(),
        );
        self.host.submit(encoder, vec![depth.clone()], vec![]);
        // Bevy submits on the very same queue. Completion owns the target lease
        // independently of this adapter and of the returned frame.
        let ready = Completion::submitted(self.host.queue(), image.clone());
        let frame = GpuFrame {
            image,
            host: self.host.clone(),
            ready,
            dependencies: vec![],
            working_space: crate::WorkingSpace::LegacyLinearSrgb,
            config_identity: None,
            timing: Some(crate::frame::Timing {
                timestamp: binding.time,
                duration: binding.duration,
            }),
            lease_id: host::NEXT_ID.fetch_add(1, Ordering::Relaxed),
            statistics: Statistics {
                evaluate_cpu_nanoseconds: begin.elapsed().as_nanos(),
                ..Default::default()
            },
        };
        // Validate every rendered channel before publication. The shared pass
        // rejects nonfinite HDR/alpha even when no CPU readback is requested.
        let image = precision::convert(&self.host, &mut self.precision, frame, cancel, true)?;
        Ok(Frame {
            binding,
            image,
            depth,
            near: scene.camera.near,
            far: scene.camera.far,
            projection: scene.camera.projection,
            preparation,
        })
    }

    fn prepare(&mut self, scene: &Scene) -> Preparation {
        let mut result = Preparation::default();
        for (id, source) in &scene.meshes {
            if self
                .meshes
                .get(id)
                .is_some_and(|(old, _)| Arc::ptr_eq(old, source) || old.as_ref() == source.as_ref())
            {
                continue;
            }
            let mesh = b::Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(b::Mesh::ATTRIBUTE_POSITION, source.positions.clone())
            .with_inserted_attribute(b::Mesh::ATTRIBUTE_NORMAL, source.normals.clone())
            .with_inserted_indices(bevy::mesh::Indices::U32(source.indices.clone()));
            let mut assets = self.app.world_mut().resource_mut::<b::Assets<b::Mesh>>();
            if let Some((old, handle)) = self.meshes.get_mut(id) {
                *assets.get_mut(handle.id()).unwrap() = mesh;
                *old = source.clone();
            } else {
                self.meshes.insert(*id, (source.clone(), assets.add(mesh)));
            }
            result.meshes += 1;
        }
        for (id, source) in &scene.materials {
            if self.materials.get(id).is_some_and(|(old, _)| old == source) {
                continue;
            }
            let material = b::StandardMaterial {
                base_color: b::Color::linear_rgba(
                    source.color[0],
                    source.color[1],
                    source.color[2],
                    source.color[3],
                ),
                unlit: source.unlit,
                perceptual_roughness: source.roughness,
                reflectance: source.reflectance,
                alpha_mode: if source.color[3] < 1. {
                    b::AlphaMode::Blend
                } else {
                    b::AlphaMode::Opaque
                },
                ..Default::default()
            };
            let mut assets = self
                .app
                .world_mut()
                .resource_mut::<b::Assets<b::StandardMaterial>>();
            if let Some((old, handle)) = self.materials.get_mut(id) {
                *assets.get_mut(handle.id()).unwrap() = material;
                *old = *source;
            } else {
                self.materials.insert(*id, (*source, assets.add(material)));
            }
            result.materials += 1;
        }
        let instance_ids: std::collections::BTreeSet<_> =
            scene.instances.iter().map(|instance| instance.id).collect();
        self.instances.retain(|id, (_, entity)| {
            if instance_ids.contains(id) {
                return true;
            }
            self.app.world_mut().despawn(*entity);
            result.removed_instances += 1;
            false
        });
        for instance in &scene.instances {
            if self
                .instances
                .get(&instance.id)
                .is_some_and(|(old, _)| old == instance)
            {
                continue;
            }
            let matrix = instance.transform.affine.unwrap_or_else(|| {
                fold_foundation::affine::Affine::trs(
                    instance.transform.translation.map(f64::from),
                    instance.transform.rotation.map(f64::from),
                    instance.transform.scale.map(f64::from),
                )
            });
            let components = (
                ExactTransform(b::Mat4::from_cols_array_2d(&matrix.matrix_f32())),
                b::Mesh3d(self.meshes[&instance.mesh].1.clone()),
                b::MeshMaterial3d(self.materials[&instance.material].1.clone()),
                b::Transform {
                    translation: instance.transform.translation.into(),
                    rotation: b::Quat::from_array(instance.transform.rotation),
                    scale: instance.transform.scale.into(),
                },
            );
            if let Some((old, entity)) = self.instances.get_mut(&instance.id) {
                self.app.world_mut().entity_mut(*entity).insert(components);
                *old = instance.clone();
                result.updated_instances += 1;
            } else {
                let entity = self.app.world_mut().spawn(components).id();
                self.instances
                    .insert(instance.id, (instance.clone(), entity));
                result.spawned_instances += 1;
            }
        }
        self.meshes.retain(|id, (_, handle)| {
            if scene.meshes.contains_key(id) {
                return true;
            }
            self.app
                .world_mut()
                .resource_mut::<b::Assets<b::Mesh>>()
                .remove(handle.id());
            false
        });
        self.materials.retain(|id, (_, handle)| {
            if scene.materials.contains_key(id) {
                return true;
            }
            self.app
                .world_mut()
                .resource_mut::<b::Assets<b::StandardMaterial>>()
                .remove(handle.id());
            false
        });
        let camera = &scene.camera;
        self.app.world_mut().entity_mut(self.camera).insert((
            b::Transform::from_translation(camera.position.into())
                .looking_at(camera.target.into(), b::Vec3::from(camera.up)),
            match camera.projection {
                crate::spatial::Projection::Perspective { vertical_fov } => {
                    b::Projection::Perspective(b::PerspectiveProjection {
                        fov: vertical_fov,
                        near: camera.near,
                        near_clip_plane: b::Vec4::new(0., 0., -1., -camera.near),
                        far: camera.far,
                        ..Default::default()
                    })
                }
                crate::spatial::Projection::Orthographic { vertical_size } => {
                    b::Projection::Orthographic(b::OrthographicProjection {
                        near: camera.near,
                        far: camera.far,
                        scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                            viewport_height: vertical_size,
                        },
                        ..b::OrthographicProjection::default_3d()
                    })
                }
            },
            Exposure {
                ev100: camera.ev100,
            },
        ));
        self.lights.retain(|id, entity| {
            if scene.lights.iter().any(|light| light.id == *id) {
                return true;
            }
            self.app.world_mut().despawn(*entity);
            false
        });
        for light in &scene.lights {
            let direction = b::Vec3::from(light.direction).normalize();
            let components = (
                b::DirectionalLight {
                    illuminance: light.illuminance,
                    color: b::Color::linear_rgb(light.color[0], light.color[1], light.color[2]),
                    shadows_enabled: false,
                    ..Default::default()
                },
                b::Transform::from_rotation(b::Quat::from_rotation_arc(b::Vec3::NEG_Z, direction)),
            );
            if let Some(entity) = self.lights.get(&light.id) {
                self.app.world_mut().entity_mut(*entity).insert(components);
            } else {
                self.lights
                    .insert(light.id, self.app.world_mut().spawn(components).id());
            }
        }
        result
    }
}
