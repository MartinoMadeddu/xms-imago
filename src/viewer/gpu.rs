//! The Viewer on wgpu: its picture is drawn into a texture of its own (with
//! depth and 4x multisampling) before egui draws the interface, then laid into
//! the Viewer's rectangle with egui's paint callback.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;

use super::scene::{Geometry, Scene, VERTEX_FLOATS};
use crate::viewport::textures::Decoded;

/// Samples of the Viewer's own picture.
const SAMPLES: u32 = 4;
/// The picture's format: sRGB, so blending happens in linear light.
const PICTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const MESH_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 8 => Float32x2];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
const LINE_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
const POINT_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32];
/// Floats per vertex dot: position, colour, radius.
const POINT_FLOATS: usize = 8;
/// Floats per instance and per line vertex.
const INSTANCE_FLOATS: usize = 24;
const LINE_FLOATS: usize = 7;

/// One frame of the Viewer, handed to the GPU.
pub struct Frame {
    pub view_proj: glam::Mat4,
    pub eye: glam::Vec3,
    /// The rectangle in physical pixels.
    pub size: [u32; 2],
    /// Background, linear.
    pub clear: [f32; 3],
    pub scene: Arc<Scene>,
    /// Lines of this frame only (Edit Poly's wireframe and manipulator).
    pub overlay: super::edit::Overlay,
}

/// Every buffer stays under this size: wgpu's guaranteed maximum is 256 MB, and
/// a groom or a dense scan can need several times that. Bigger meshes are split
/// into parts, more copies into several instance buffers.
const BUFFER_BYTES: usize = 128 << 20;
const PART_VERTICES: usize = BUFFER_BYTES / (VERTEX_FLOATS * 4);
const PART_INDICES: usize = BUFFER_BYTES / 4;

/// A mesh on the GPU, kept while its geometry lives: one or more parts, each
/// its own vertex and index buffers.
struct GpuMesh {
    geometry: Arc<Geometry>,
    parts: Vec<(wgpu::Buffer, wgpu::Buffer, u32)>,
}

/// A mesh drawn at its copies: one instance buffer per chunk of copies.
struct Draw {
    mesh: usize,
    instances: Vec<(wgpu::Buffer, u32)>,
    blend: bool,
    edges: bool,
    material: Option<usize>,
}

/// The mesh in parts that each fit a buffer. `per` indices make one triangle
/// (3) or line (2). A part takes the vertices its primitives use, renumbered;
/// small meshes are one part, uncopied.
fn parts(g: &Geometry, per: usize) -> Vec<(std::borrow::Cow<'_, [f32]>, std::borrow::Cow<'_, [u32]>)> {
    use std::borrow::Cow;
    let n = g.vertices.len() / VERTEX_FLOATS;
    if n <= PART_VERTICES && g.indices.len() <= PART_INDICES {
        return vec![(Cow::Borrowed(&g.vertices[..]), Cow::Borrowed(&g.indices[..]))];
    }
    let mut out = vec![];
    let mut remap = vec![u32::MAX; n];
    let (mut verts, mut idx, mut used): (Vec<f32>, Vec<u32>, Vec<usize>) = (vec![], vec![], vec![]);
    for prim in g.indices.chunks(per) {
        let new = prim
            .iter()
            .filter(|&&i| remap.get(i as usize) == Some(&u32::MAX))
            .count();
        if verts.len() / VERTEX_FLOATS + new > PART_VERTICES || idx.len() + per > PART_INDICES {
            out.push((
                Cow::Owned(std::mem::take(&mut verts)),
                Cow::Owned(std::mem::take(&mut idx)),
            ));
            for v in used.drain(..) {
                remap[v] = u32::MAX;
            }
        }
        for &i in prim {
            let i = i as usize;
            if i >= n {
                continue;
            }
            if remap[i] == u32::MAX {
                remap[i] = (verts.len() / VERTEX_FLOATS) as u32;
                verts.extend_from_slice(&g.vertices[i * VERTEX_FLOATS..(i + 1) * VERTEX_FLOATS]);
                used.push(i);
            }
            idx.push(remap[i]);
        }
    }
    if !idx.is_empty() {
        out.push((Cow::Owned(verts), Cow::Owned(idx)));
    }
    out
}

/// Floats into vertex buffers of at most `BUFFER_BYTES`, whole items (`stride`
/// floats) each, with how many items each holds.
fn chunked(device: &wgpu::Device, label: &str, data: &[f32], stride: usize) -> Vec<(wgpu::Buffer, u32)> {
    let per = (BUFFER_BYTES / (stride * 4)).max(1) * stride;
    data.chunks(per)
        .map(|c| {
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(c),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                (c.len() / stride) as u32,
            )
        })
        .collect()
}

struct Target {
    size: [u32; 2],
    colour: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    blit: wgpu::BindGroup,
}

/// Everything the Viewer keeps on the GPU, in egui's callback resources.
pub struct ViewerGpu {
    view_buffer: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    /// Lines on their surface (a little towards the camera), and lines over
    /// everything.
    raised: wgpu::RenderPipeline,
    on_top: wgpu::RenderPipeline,
    top_buffer: Vec<(wgpu::Buffer, u32)>,
    points: wgpu::RenderPipeline,
    point_buffer: Option<(wgpu::Buffer, u32)>,
    frame_lines: [Vec<(wgpu::Buffer, u32)>; 2],
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Textures by file, kept while a material uses them.
    material_layout: wgpu::BindGroupLayout,
    map_sampler: wgpu::Sampler,
    textures: HashMap<PathBuf, (Arc<Decoded>, wgpu::TextureView)>,
    /// A white pixel, for a material without a texture.
    white: wgpu::TextureView,
    /// The scene's materials, and the one for everything without textures.
    materials: Vec<wgpu::BindGroup>,
    plain: wgpu::BindGroup,
    target: Option<Target>,
    meshes: Vec<GpuMesh>,
    draws: Vec<Draw>,
    line_buffer: Vec<(wgpu::Buffer, u32)>,
    generation: u64,
}

impl ViewerGpu {
    /// `egui_format` and `egui_samples`: the format and multisampling of the
    /// pass egui draws the interface in, where the picture is laid.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        egui_format: wgpu::TextureFormat,
        egui_samples: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });

        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewer view"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer view"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer material"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Where each map's UVs read it from: a UDIM texture's tiles
                // side by side in one image (`MapPlace` in the shader).
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer meshes"),
            bind_group_layouts: &[Some(&view_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer lines"),
            bind_group_layouts: &[Some(&view_layout)],
            immediate_size: 0,
        });

        let mesh_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: (VERTEX_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &MESH_ATTRS,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: (INSTANCE_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &INSTANCE_ATTRS,
            }),
        ];
        let line_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: (LINE_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &LINE_ATTRS,
        })];
        let pipeline_with = |label: &str,
                             layout: &wgpu::PipelineLayout,
                             vs: &str,
                             fs: &str,
                             buffers: &[Option<wgpu::VertexBufferLayout>],
                             topology,
                             write_depth: bool,
                             compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: PICTURE,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    // Reversed depth: nearer is larger.
                    format: DEPTH,
                    depth_write_enabled: Some(write_depth),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        topology,
                        write_depth: bool| {
            pipeline_with(
                label,
                layout,
                vs,
                fs,
                buffers,
                topology,
                write_depth,
                wgpu::CompareFunction::GreaterEqual,
            )
        };
        let raised = pipeline_with(
            "viewer raised lines",
            &line_layout,
            "vs_line_raised",
            "fs_line",
            &line_buffers,
            wgpu::PrimitiveTopology::LineList,
            false,
            wgpu::CompareFunction::GreaterEqual,
        );
        let on_top = pipeline_with(
            "viewer lines on top",
            &line_layout,
            "vs_line",
            "fs_line",
            &line_buffers,
            wgpu::PrimitiveTopology::LineList,
            false,
            wgpu::CompareFunction::Always,
        );
        let point_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: (POINT_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &POINT_ATTRS,
        })];
        let points = pipeline_with(
            "viewer points",
            &line_layout,
            "vs_point",
            "fs_point",
            &point_buffers,
            wgpu::PrimitiveTopology::TriangleList,
            false,
            wgpu::CompareFunction::GreaterEqual,
        );
        let opaque = pipeline(
            "viewer opaque",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &mesh_buffers,
            wgpu::PrimitiveTopology::TriangleList,
            true,
        );
        let blended = pipeline(
            "viewer blended",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &mesh_buffers,
            wgpu::PrimitiveTopology::TriangleList,
            false,
        );
        let lines = pipeline(
            "viewer lines",
            &line_layout,
            "vs_line",
            "fs_line",
            &line_buffers,
            wgpu::PrimitiveTopology::LineList,
            true,
        );
        let edges = pipeline(
            "viewer edges",
            &layout,
            "vs_edge",
            "fs_mesh",
            &mesh_buffers,
            wgpu::PrimitiveTopology::LineList,
            true,
        );

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer blit"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        // egui's target is usually not sRGB: the picture is encoded on the way.
        let fs = if egui_format.is_srgb() {
            "fs_blit_linear"
        } else {
            "fs_blit_encode"
        };
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer blit"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_blit"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: egui_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: egui_samples.max(1),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewer blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Textures repeat outside the unit square, and their smaller copies
        // keep distant surfaces from shimmering.
        let map_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewer maps"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("viewer white"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: PICTURE,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[255, 255, 255, 255],
            )
            .create_view(&wgpu::TextureViewDescriptor::default());
        let plain = material_group(device, &material_layout, &white, &white, &map_sampler, [1, 1], [1, 1]);

        ViewerGpu {
            view_buffer,
            view_group,
            opaque,
            blended,
            lines,
            edges,
            raised,
            on_top,
            top_buffer: vec![],
            frame_lines: [vec![], vec![]],
            points,
            point_buffer: None,
            blit,
            blit_layout,
            sampler,
            material_layout,
            map_sampler,
            textures: HashMap::new(),
            white,
            materials: vec![],
            plain,
            target: None,
            meshes: vec![],
            draws: vec![],
            line_buffer: vec![],
            generation: u64::MAX,
        }
    }

    /// The picture's textures, made again when the rectangle changes size.
    fn ensure_target(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let texture = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let colour = texture(
            "viewer colour",
            PICTURE,
            SAMPLES,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let resolve = texture(
            "viewer picture",
            PICTURE,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture("viewer depth", DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let blit = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&resolve),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.target = Some(Target {
            size,
            colour,
            resolve,
            depth,
            blit,
        });
    }

    /// The scene's meshes and lines into GPU buffers. Meshes already there (the
    /// same geometry) are kept.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) {
        // Textures: the ones already on the GPU are kept, new ones uploaded
        // with their smaller copies, unused ones let go.
        let mut textures: HashMap<PathBuf, (Arc<Decoded>, wgpu::TextureView)> = HashMap::new();
        for m in &scene.materials {
            for (path, decoded) in m.colour.iter().chain(m.emissive.iter()) {
                if textures.contains_key(path) {
                    continue;
                }
                let view = match self.textures.remove(path) {
                    Some((d, v)) if Arc::ptr_eq(&d, decoded) => v,
                    // Too big for the GPU even at its smallest: drawn untextured.
                    _ => match upload_texture(device, queue, decoded) {
                        Some(v) => v,
                        None => continue,
                    },
                };
                textures.insert(path.clone(), (decoded.clone(), view));
            }
        }
        self.textures = textures;
        self.materials = scene
            .materials
            .iter()
            .map(|m| {
                let view = |t: &Option<(PathBuf, Arc<Decoded>)>| {
                    t.as_ref()
                        .and_then(|(p, _)| self.textures.get(p))
                        .map(|(_, v)| v)
                        .unwrap_or(&self.white)
                };
                let tiles = |t: &Option<(PathBuf, Arc<Decoded>)>| {
                    t.as_ref()
                        .filter(|(p, _)| self.textures.contains_key(p))
                        .map_or([1, 1], |(_, d)| d.tiles)
                };
                material_group(
                    device,
                    &self.material_layout,
                    view(&m.colour),
                    view(&m.emissive),
                    &self.map_sampler,
                    tiles(&m.colour),
                    tiles(&m.emissive),
                )
            })
            .collect();

        let mut meshes: Vec<GpuMesh> = vec![];
        let mut old: Vec<GpuMesh> = std::mem::take(&mut self.meshes);
        self.draws.clear();
        for d in &scene.meshes {
            if d.geometry.indices.is_empty() || d.instances.is_empty() {
                continue;
            }
            let mesh = match meshes.iter().position(|m| Arc::ptr_eq(&m.geometry, &d.geometry)) {
                Some(i) => i,
                None => {
                    let gm = match old.iter().position(|m| Arc::ptr_eq(&m.geometry, &d.geometry)) {
                        Some(i) => old.swap_remove(i),
                        None => GpuMesh {
                            geometry: d.geometry.clone(),
                            parts: parts(&d.geometry, if d.edges { 2 } else { 3 })
                                .into_iter()
                                .map(|(v, i)| {
                                    (
                                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                            label: Some("viewer vertices"),
                                            contents: bytemuck::cast_slice(&v),
                                            usage: wgpu::BufferUsages::VERTEX,
                                        }),
                                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                            label: Some("viewer indices"),
                                            contents: bytemuck::cast_slice(&i),
                                            usage: wgpu::BufferUsages::INDEX,
                                        }),
                                        i.len() as u32,
                                    )
                                })
                                .collect(),
                        },
                    };
                    meshes.push(gm);
                    meshes.len() - 1
                }
            };
            let instances = chunked(device, "viewer instances", &d.instances, INSTANCE_FLOATS);
            self.draws.push(Draw {
                mesh,
                instances,
                blend: d.blend,
                edges: d.edges,
                material: d.material,
            });
        }
        self.meshes = meshes;
        self.line_buffer = line_buffer(device, &scene.lines);
        self.top_buffer = line_buffer(device, &scene.top);
        self.generation = scene.generation;
    }

    fn render(&self, encoder: &mut wgpu::CommandEncoder, clear: [f32; 3]) {
        let Some(t) = &self.target else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.colour,
                depth_slice: None,
                resolve_target: Some(&t.resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear[0] as f64,
                        g: clear[1] as f64,
                        b: clear[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.view_group, &[]);
        // Surfaces, their edges, lines (grid, boxes, curves), then what is
        // see-through.
        pass.set_pipeline(&self.opaque);
        for d in self.draws.iter().filter(|d| !d.blend && !d.edges) {
            self.draw_mesh(&mut pass, d);
        }
        pass.set_pipeline(&self.edges);
        for d in self.draws.iter().filter(|d| d.edges) {
            self.draw_mesh(&mut pass, d);
        }
        pass.set_pipeline(&self.lines);
        for (buffer, count) in &self.line_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
        // Edit Poly's wireframe and vertices, on their surface.
        pass.set_pipeline(&self.raised);
        for (buffer, count) in &self.frame_lines[0] {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
        if let Some((buffer, count)) = &self.point_buffer {
            pass.set_pipeline(&self.points);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, 0..*count);
        }
        pass.set_pipeline(&self.blended);
        for d in self.draws.iter().filter(|d| d.blend && !d.edges) {
            self.draw_mesh(&mut pass, d);
        }
        // Over everything: the skeleton and the manipulator.
        pass.set_pipeline(&self.on_top);
        for (buffer, count) in self.top_buffer.iter().chain(&self.frame_lines[1]) {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
    }

    fn draw_mesh(&self, pass: &mut wgpu::RenderPass<'_>, d: &Draw) {
        let m = &self.meshes[d.mesh];
        let material = d.material.and_then(|i| self.materials.get(i)).unwrap_or(&self.plain);
        pass.set_bind_group(1, material, &[]);
        for (vertices, indices, count) in &m.parts {
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for (instances, copies) in &d.instances {
                pass.set_vertex_buffer(1, instances.slice(..));
                pass.draw_indexed(0..*count, 0, 0..*copies);
            }
        }
    }
}

/// Lines into vertex buffers, whole lines each (two vertices).
fn line_buffer(device: &wgpu::Device, lines: &[f32]) -> Vec<(wgpu::Buffer, u32)> {
    chunked(device, "viewer lines", lines, LINE_FLOATS * 2)
        .into_iter()
        .map(|(b, n)| (b, n * 2))
        .collect()
}

/// A material's bind group: its colour and emissive textures and the sampler.
/// How a map's UVs read it: scale then offset, as `MapPlace` in the shader. The
/// GPU's UVs are (u, 1 - v); a UDIM image holds `tiles` across and up, tile
/// 1001 at the bottom left, so u is divided by the tiles across and v by the
/// tiles up, from the bottom.
pub fn map_place(tiles: [u32; 2]) -> [f32; 4] {
    let (sx, sy) = (1.0 / tiles[0].max(1) as f32, 1.0 / tiles[1].max(1) as f32);
    [sx, sy, 0.0, 1.0 - sy]
}

fn material_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    colour: &wgpu::TextureView,
    emissive: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    colour_tiles: [u32; 2],
    emissive_tiles: [u32; 2],
) -> wgpu::BindGroup {
    let place: Vec<f32> = map_place(colour_tiles)
        .into_iter()
        .chain(map_place(emissive_tiles))
        .collect();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("viewer map place"),
        contents: bytemuck::cast_slice(&place),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewer material"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(colour),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(emissive),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: buffer.as_entire_binding(),
            },
        ],
    })
}

/// A decoded image and its smaller copies, as an sRGB texture. Larger than the
/// GPU takes (8,192 pixels a side is the guaranteed maximum; a 16K texture is
/// not unusual), the image starts from its first smaller copy that fits. `None`
/// when none does.
fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, d: &Decoded) -> Option<wgpu::TextureView> {
    let max = device.limits().max_texture_dimension_2d;
    let (mut w, mut h, mut at, mut levels) = (d.width, d.height, 0usize, d.levels.max(1));
    while w > max || h > max {
        if levels <= 1 {
            return None;
        }
        at += (w * h * 4) as usize;
        (w, h, levels) = ((w / 2).max(1), (h / 2).max(1), levels - 1);
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewer map"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: PICTURE,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..levels {
        let size = (w * h * 4) as usize;
        if at + size > d.pixels.len() {
            break;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &d.pixels[at..at + size],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        at += size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Some(texture.create_view(&wgpu::TextureViewDescriptor::default()))
}

/// The Viewer's paint callback: draws the picture before egui's pass, and lays
/// it into the Viewer's rectangle during it.
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
        let Some(gpu) = resources.get_mut::<ViewerGpu>() else {
            return vec![];
        };
        let f = &self.0;
        gpu.ensure_target(device, [f.size[0].max(1), f.size[1].max(1)]);
        if gpu.generation != f.scene.generation {
            gpu.upload(device, queue, &f.scene);
        }
        let mut view = [0f32; 24];
        view[..16].copy_from_slice(&f.view_proj.to_cols_array());
        view[16..19].copy_from_slice(&f.eye.to_array());
        view[20] = f.size[0] as f32;
        view[21] = f.size[1] as f32;
        queue.write_buffer(&gpu.view_buffer, 0, bytemuck::cast_slice(&view));
        gpu.frame_lines = [
            line_buffer(device, &f.overlay.depth),
            line_buffer(device, &f.overlay.top),
        ];
        gpu.point_buffer = (!f.overlay.points.is_empty()).then(|| {
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("viewer points"),
                    contents: bytemuck::cast_slice(&f.overlay.points),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                (f.overlay.points.len() / POINT_FLOATS) as u32,
            )
        });
        gpu.render(encoder, f.clear);
        vec![]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(gpu) = resources.get::<ViewerGpu>() else {
            return;
        };
        let Some(t) = &gpu.target else { return };
        pass.set_pipeline(&gpu.blit);
        pass.set_bind_group(0, &t.blit, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Put the Viewer's GPU state where its paint callback finds it.
pub fn install(render_state: &egui_wgpu::RenderState, egui_samples: u32) {
    let gpu = ViewerGpu::new(
        &render_state.device,
        &render_state.queue,
        render_state.target_format,
        egui_samples,
    );
    render_state.renderer.write().callback_resources.insert(gpu);
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn f<T: Send + Sync>() {}
    f::<ViewerPaint>();
}

#[cfg(test)]
mod map_place_tests {
    use super::map_place;

    /// An ordinary texture is read as it is; a UDIM texture of 2 by 2 tiles
    /// puts the middle of tile 1002 (UV 1.5, 0.5) at the middle of the image's
    /// bottom-right quarter.
    #[test]
    fn udim_uvs_land_on_their_tile() {
        assert_eq!(map_place([1, 1]), [1.0, 1.0, 0.0, 0.0]);
        let p = map_place([2, 2]);
        let (u, v) = (1.5f32, 0.5f32);
        let gpu = [u, 1.0 - v];
        let read = [gpu[0] * p[0] + p[2], gpu[1] * p[1] + p[3]];
        assert_eq!(read, [0.75, 0.75]);
    }
}
