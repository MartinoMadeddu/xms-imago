//! The Viewer on wgpu: its picture is drawn into a texture of its own (with
//! depth and 4x multisampling) before egui draws the interface, then laid
//! into the Viewer's rectangle with egui's paint callback.

use std::sync::Arc;

use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;

use super::scene::{Geometry, Scene};

/// Samples of the Viewer's own picture.
const SAMPLES: u32 = 4;
/// The picture's format: sRGB, so blending happens in linear light.
const PICTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const MESH_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
const LINE_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
/// Floats per instance and per line vertex.
const INSTANCE_FLOATS: usize = 24;
const LINE_FLOATS: usize = 7;

/// One frame of the Viewer, handed to the GPU.
pub struct Frame {
    pub view_proj: glam::Mat4,
    pub eye:       glam::Vec3,
    /// The rectangle in physical pixels.
    pub size:      [u32; 2],
    /// Background, linear.
    pub clear:     [f32; 3],
    pub scene:     Arc<Scene>,
}

/// A mesh on the GPU, kept while its geometry lives.
struct GpuMesh {
    geometry: Arc<Geometry>,
    vertices: wgpu::Buffer,
    indices:  wgpu::Buffer,
    count:    u32,
}

struct Draw { mesh: usize, instances: wgpu::Buffer, copies: u32, blend: bool, edges: bool }

struct Target {
    size:    [u32; 2],
    colour:  wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth:   wgpu::TextureView,
    blit:    wgpu::BindGroup,
}

/// Everything the Viewer keeps on the GPU, in egui's callback resources.
pub struct ViewerGpu {
    view_buffer: wgpu::Buffer,
    view_group:  wgpu::BindGroup,
    opaque:      wgpu::RenderPipeline,
    blended:     wgpu::RenderPipeline,
    lines:       wgpu::RenderPipeline,
    edges:       wgpu::RenderPipeline,
    blit:        wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    sampler:     wgpu::Sampler,
    target:      Option<Target>,
    meshes:      Vec<GpuMesh>,
    draws:       Vec<Draw>,
    line_buffer: Option<(wgpu::Buffer, u32)>,
    generation:  u64,
}

impl ViewerGpu {
    /// `egui_format` and `egui_samples`: the format and multisampling of the
    /// pass egui draws the interface in, where the picture is laid.
    pub fn new(device: &wgpu::Device, egui_format: wgpu::TextureFormat, egui_samples: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer blit"), source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });

        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewer view"), size: 80, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer view"), layout: &view_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_buffer.as_entire_binding() }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer"), bind_group_layouts: &[&view_layout], push_constant_ranges: &[],
        });

        let mesh_buffers = [
            wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &MESH_ATTRS },
            wgpu::VertexBufferLayout { array_stride: (INSTANCE_FLOATS * 4) as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &INSTANCE_ATTRS },
        ];
        let line_buffers = [
            wgpu::VertexBufferLayout { array_stride: (LINE_FLOATS * 4) as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &LINE_ATTRS },
        ];
        let pipeline = |label: &str, vs: &str, fs: &str, buffers: &[wgpu::VertexBufferLayout], topology, write_depth: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: vs, buffers, compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader, entry_point: fs, compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: PICTURE, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    // Reversed depth: nearer is larger.
                    format: DEPTH, depth_write_enabled: write_depth, depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: Default::default(), bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false },
                multiview: None,
            })
        };
        let opaque  = pipeline("viewer opaque", "vs_mesh", "fs_mesh", &mesh_buffers, wgpu::PrimitiveTopology::TriangleList, true);
        let blended = pipeline("viewer blended", "vs_mesh", "fs_mesh", &mesh_buffers, wgpu::PrimitiveTopology::TriangleList, false);
        let lines   = pipeline("viewer lines", "vs_line", "fs_line", &line_buffers, wgpu::PrimitiveTopology::LineList, true);
        let edges   = pipeline("viewer edges", "vs_edge", "fs_mesh", &mesh_buffers, wgpu::PrimitiveTopology::LineList, true);

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
                },
            ],
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer blit"), bind_group_layouts: &[&blit_layout], push_constant_ranges: &[],
        });
        // egui's target is usually not sRGB: the picture is encoded on the way.
        let fs = if egui_format.is_srgb() { "fs_blit_linear" } else { "fs_blit_encode" };
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer blit"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState { module: &blit_shader, entry_point: "vs_blit", buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader, entry_point: fs, compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: egui_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: egui_samples.max(1), mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewer blit"),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        ViewerGpu {
            view_buffer, view_group, opaque, blended, lines, edges, blit, blit_layout, sampler,
            target: None, meshes: vec![], draws: vec![], line_buffer: None, generation: u64::MAX,
        }
    }

    /// The picture's textures, made again when the rectangle changes size.
    fn ensure_target(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) { return; }
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let texture = |label, format, samples, usage| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: extent, mip_level_count: 1, sample_count: samples,
            dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[],
        }).create_view(&wgpu::TextureViewDescriptor::default());
        let colour  = texture("viewer colour", PICTURE, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = texture("viewer picture", PICTURE, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth   = texture("viewer depth", DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let blit = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer blit"), layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolve) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.target = Some(Target { size, colour, resolve, depth, blit });
    }

    /// The scene's meshes and lines into GPU buffers. Meshes already there
    /// (the same geometry) are kept.
    fn upload(&mut self, device: &wgpu::Device, scene: &Scene) {
        let mut meshes: Vec<GpuMesh> = vec![];
        let mut old: Vec<GpuMesh> = std::mem::take(&mut self.meshes);
        self.draws.clear();
        for d in &scene.meshes {
            if d.geometry.indices.is_empty() || d.instances.is_empty() { continue; }
            let mesh = match meshes.iter().position(|m| Arc::ptr_eq(&m.geometry, &d.geometry)) {
                Some(i) => i,
                None => {
                    let gm = match old.iter().position(|m| Arc::ptr_eq(&m.geometry, &d.geometry)) {
                        Some(i) => old.swap_remove(i),
                        None => GpuMesh {
                            geometry: d.geometry.clone(),
                            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("viewer vertices"), contents: bytemuck::cast_slice(&d.geometry.vertices), usage: wgpu::BufferUsages::VERTEX,
                            }),
                            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("viewer indices"), contents: bytemuck::cast_slice(&d.geometry.indices), usage: wgpu::BufferUsages::INDEX,
                            }),
                            count: d.geometry.indices.len() as u32,
                        },
                    };
                    meshes.push(gm);
                    meshes.len() - 1
                }
            };
            let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("viewer instances"), contents: bytemuck::cast_slice(&d.instances), usage: wgpu::BufferUsages::VERTEX,
            });
            self.draws.push(Draw { mesh, instances, copies: (d.instances.len() / INSTANCE_FLOATS) as u32, blend: d.blend, edges: d.edges });
        }
        self.meshes = meshes;
        self.line_buffer = (!scene.lines.is_empty()).then(|| (
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("viewer lines"), contents: bytemuck::cast_slice(&scene.lines), usage: wgpu::BufferUsages::VERTEX,
            }),
            (scene.lines.len() / LINE_FLOATS) as u32,
        ));
        self.generation = scene.generation;
    }

    fn render(&self, encoder: &mut wgpu::CommandEncoder, clear: [f32; 3]) {
        let Some(t) = &self.target else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.colour,
                resolve_target: Some(&t.resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.view_group, &[]);
        // Surfaces, their edges, lines (grid, boxes, curves), then what is
        // see-through.
        pass.set_pipeline(&self.opaque);
        for d in self.draws.iter().filter(|d| !d.blend && !d.edges) { self.draw_mesh(&mut pass, d); }
        pass.set_pipeline(&self.edges);
        for d in self.draws.iter().filter(|d| d.edges) { self.draw_mesh(&mut pass, d); }
        if let Some((buffer, count)) = &self.line_buffer {
            pass.set_pipeline(&self.lines);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
        pass.set_pipeline(&self.blended);
        for d in self.draws.iter().filter(|d| d.blend && !d.edges) { self.draw_mesh(&mut pass, d); }
    }

    fn draw_mesh<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, d: &'a Draw) {
        let m = &self.meshes[d.mesh];
        pass.set_vertex_buffer(0, m.vertices.slice(..));
        pass.set_vertex_buffer(1, d.instances.slice(..));
        pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..m.count, 0, 0..d.copies);
    }
}

/// The Viewer's paint callback: draws the picture before egui's pass, and
/// lays it into the Viewer's rectangle during it.
pub struct ViewerPaint(pub Frame);

impl CallbackTrait for ViewerPaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = resources.get_mut::<ViewerGpu>() else { return vec![] };
        let f = &self.0;
        gpu.ensure_target(device, [f.size[0].max(1), f.size[1].max(1)]);
        if gpu.generation != f.scene.generation { gpu.upload(device, &f.scene); }
        let mut view = [0f32; 20];
        view[..16].copy_from_slice(&f.view_proj.to_cols_array());
        view[16..19].copy_from_slice(&f.eye.to_array());
        queue.write_buffer(&gpu.view_buffer, 0, bytemuck::cast_slice(&view));
        gpu.render(encoder, f.clear);
        vec![]
    }

    fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'a>, resources: &'a CallbackResources) {
        let Some(gpu) = resources.get::<ViewerGpu>() else { return };
        let Some(t) = &gpu.target else { return };
        pass.set_pipeline(&gpu.blit);
        pass.set_bind_group(0, &t.blit, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Put the Viewer's GPU state where its paint callback finds it.
pub fn install(render_state: &egui_wgpu::RenderState, egui_samples: u32) {
    let gpu = ViewerGpu::new(&render_state.device, render_state.target_format, egui_samples);
    render_state.renderer.write().callback_resources.insert(gpu);
}

#[allow(dead_code)]
fn _assert_send_sync() { fn f<T: Send + Sync>() {} f::<ViewerPaint>(); }
