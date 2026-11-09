//! The Viewer on wgpu: its picture is drawn into a texture of its own (with
//! depth and 4x multisampling) before egui draws the interface, then laid into
//! the Viewer's rectangle with egui's paint callback, through the display
//! transform of the view chosen (`xms_colour`).

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
/// The picture's format: half floats, so it keeps light brighter than 1 for
/// the display transform, and blending happens in linear light.
const PICTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Textures' format: sRGB bytes, read back as linear.
const MAP: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Export Movie's frames: the display's colours as bytes, not encoded again.
const MOVIE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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
    /// The display and view the picture is shown in.
    pub display: crate::colour::View,
    /// Export Movie: this frame drawn again at the movie's size and read back.
    pub capture: Option<super::movie::Capture>,
    /// Shown in place of the scene drawn: the render, the overlays over it,
    /// or a snapshot.
    pub background: Option<Background>,
}

/// What the Viewer shows in place of the scene drawn.
pub enum Background {
    /// Viewer > Arnold: the render's pixels as they come in.
    Live(Arc<xms_render::Pixels>),
    /// A snapshot (the snapshots bar).
    Still(Arc<xms_image::Image>),
}

/// The render or snapshot shown, on the GPU: a render's written only where
/// buckets came in.
struct Rendered {
    /// Which render's pixels or snapshot (by address).
    of: usize,
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
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
    /// Its copies: which of the shared instance buffers, from which byte, how
    /// many.
    instances: Vec<(usize, u64, u32)>,
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
    display: Display,
    blit_layout: wgpu::BindGroupLayout,
    egui_format: wgpu::TextureFormat,
    egui_samples: u32,
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
    /// Export Movie: the picture at the movie's size, the display pass into
    /// bytes, and the bytes read back.
    movie: Option<(Target, wgpu::Texture)>,
    movie_display: Option<Display>,
    meshes: Vec<GpuMesh>,
    draws: Vec<Draw>,
    /// Every draw's copies, packed into as few buffers as fit: a scene of
    /// thousands of prims makes a handful of buffers a frame, not one each.
    instance_buffers: Vec<wgpu::Buffer>,
    /// The geometry each of the scene's meshes had when last sent, by its
    /// place in the scene: a deforming mesh's next frame is written into the
    /// same buffers.
    slots: Vec<usize>,
    line_buffer: Vec<(wgpu::Buffer, u32)>,
    generation: u64,
    /// A render laid over the whole picture, behind the overlays.
    background: wgpu::RenderPipeline,
    rendered_layout: wgpu::BindGroupLayout,
    rendered: Option<Rendered>,
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

        let rendered_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer render"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let rendered = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer render"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rendered.wgsl").into()),
        });
        let background = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer render"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("viewer render"),
                bind_group_layouts: &[Some(&view_layout), Some(&rendered_layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &rendered,
                entry_point: Some("vs_rendered"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rendered,
                entry_point: Some("fs_rendered"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: PICTURE,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            // Behind everything: the overlays drawn after it all pass.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
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
        });

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
        let display = display(
            device,
            queue,
            &blit_layout,
            egui_format,
            egui_samples,
            &crate::colour::view(crate::colour::Pane::Viewer),
        );
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
                    format: MAP,
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
            display,
            blit_layout,
            egui_format,
            egui_samples,
            sampler,
            material_layout,
            map_sampler,
            textures: HashMap::new(),
            white,
            materials: vec![],
            plain,
            target: None,
            movie: None,
            movie_display: None,
            meshes: vec![],
            draws: vec![],
            instance_buffers: vec![],
            slots: vec![],
            line_buffer: vec![],
            generation: u64::MAX,
            background,
            rendered_layout,
            rendered: None,
        }
    }

    /// The render's beauty on the GPU: a texture made again for another
    /// render or size, else written only where buckets came in (the Viewer
    /// reads as reader 1).
    fn ensure_rendered(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pixels: &Arc<xms_render::Pixels>) {
        let of = Arc::as_ptr(pixels) as usize;
        let kept = self.rendered.as_ref().filter(|r| r.of == of).map(|r| r.texture.size());
        let Some(mut region) = pixels.take("RGBA", 1, kept.is_none()) else {
            return;
        };
        let (w, h) = (region.frame.0 as u32, region.frame.1 as u32);
        if kept.is_none_or(|s| (s.width, s.height) != (w, h)) {
            if (region.width, region.height) != region.frame {
                region = pixels.take("RGBA", 1, true).unwrap_or(region);
            }
            self.rendered = Some(self.rendered_texture(device, of, (w, h)));
        }
        let at = (
            region.x as u32,
            region.y as u32,
            region.width as u32,
            region.height as u32,
        );
        self.write_rendered(queue, at, bytemuck::cast_slice(&region.rgba));
    }

    /// A snapshot's beauty (or first layer) on the GPU, once.
    fn ensure_still(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &Arc<xms_image::Image>) {
        let of = Arc::as_ptr(image) as usize;
        if self.rendered.as_ref().is_some_and(|r| r.of == of) {
            return;
        }
        let layers = image.layers();
        let Some(layer) = layers.iter().find(|l| l.name == "RGBA").or(layers.first()) else {
            return;
        };
        let (w, h) = (image.width as u32, image.height as u32);
        self.rendered = Some(self.rendered_texture(device, of, (w, h)));
        let pixels = crate::image_gpu::layer_pixels(image, layer);
        self.write_rendered(queue, (0, 0, w, h), bytemuck::cast_slice(&pixels));
    }

    fn rendered_texture(&self, device: &wgpu::Device, of: usize, (w, h): (u32, u32)) -> Rendered {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewer render"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer render"),
            layout: &self.rendered_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
            }],
        });
        Rendered { of, texture, group }
    }

    /// RGBA floats into the texture of what is shown, at (x, y, width, height).
    fn write_rendered(&self, queue: &wgpu::Queue, (x, y, w, h): (u32, u32, u32, u32), rgba: &[u8]) {
        let Some(r) = self.rendered.as_ref().filter(|_| w > 0 && h > 0) else {
            return;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &r.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 16),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// The picture's textures, made again when the rectangle changes size.
    fn ensure_target(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        self.target = Some(self.make_target(device, size));
    }

    /// Textures to draw a picture this size into: multisampled colour and
    /// depth, and the picture they resolve to.
    fn make_target(&self, device: &wgpu::Device, size: [u32; 2]) -> Target {
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
        Target {
            size,
            colour,
            resolve,
            depth,
            blit,
        }
    }

    /// The view's matrix, eye and size in pixels, for the next draw.
    fn write_view(&self, queue: &wgpu::Queue, view_proj: glam::Mat4, eye: glam::Vec3, size: [u32; 2]) {
        let mut view = [0f32; 24];
        view[..16].copy_from_slice(&view_proj.to_cols_array());
        view[16..19].copy_from_slice(&eye.to_array());
        view[20] = size[0] as f32;
        view[21] = size[1] as f32;
        queue.write_buffer(&self.view_buffer, 0, bytemuck::cast_slice(&view));
    }

    /// Export Movie: the picture drawn at the movie's size, through the
    /// display transform the Viewer shows it in, into bytes read back and
    /// handed to ffmpeg. Waits for the GPU: a movie is made frame by frame.
    fn capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        c: &super::movie::Capture,
        eye: glam::Vec3,
        clear: [f32; 3],
    ) {
        let size = [c.size[0].max(1), c.size[1].max(1)];
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        if self.movie.as_ref().is_none_or(|(t, _)| t.size != size) {
            let target = self.make_target(device, size);
            let bytes = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("viewer movie frame"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: MOVIE,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.movie = Some((target, bytes));
        }
        if self.movie_display.as_ref().is_none_or(|d| d.view != self.display.view) {
            self.movie_display = Some(display(device, queue, &self.blit_layout, MOVIE, 1, &self.display.view));
        }
        let (Some((t, bytes)), Some(shown)) = (&self.movie, &self.movie_display) else {
            return;
        };
        self.write_view(queue, c.view_proj, eye, size);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewer movie"),
        });
        self.render(&mut encoder, t, clear, None);
        {
            let view = bytes.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewer movie display"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&shown.pipeline);
            pass.set_bind_group(0, &t.blit, &[]);
            pass.set_bind_group(1, &shown.group, &[]);
            pass.draw(0..3, 0..1);
        }
        let row = size[0] * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewer movie frame"),
            size: padded as u64 * size[1] as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            bytes.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(size[1]),
                },
            },
            extent,
        );
        let index = queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r.is_ok());
        });
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: None,
        });
        if rx.recv() != Ok(true) {
            return;
        }
        let pixels = {
            let Ok(data) = slice.get_mapped_range() else { return };
            let mut pixels = Vec::with_capacity((row * size[1]) as usize);
            for r in data.chunks(padded as usize) {
                pixels.extend_from_slice(&r[..row as usize]);
            }
            pixels
        };
        buffer.unmap();
        c.done(pixels);
    }

    /// The scene's meshes and lines into GPU buffers. Meshes already there (the
    /// same geometry) are kept.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) {
        let _t = crate::timings::span(format!("viewer: send {} draws to the GPU", scene.meshes.len()));
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

        // Meshes by their geometry: a scene of thousands finds each at once.
        let mut meshes: Vec<GpuMesh> = vec![];
        let ptr = |g: &Arc<Geometry>| Arc::as_ptr(g) as usize;
        let mut index: HashMap<usize, usize> = HashMap::new();
        let mut old: HashMap<usize, GpuMesh> = std::mem::take(&mut self.meshes)
            .into_iter()
            .map(|m| (ptr(&m.geometry), m))
            .collect();
        let was = std::mem::take(&mut self.slots);
        self.draws.clear();
        // Floats of copies a buffer holds; each draw's put after the last's.
        let per = (BUFFER_BYTES / (INSTANCE_FLOATS * 4)).max(1) * INSTANCE_FLOATS;
        let mut packs: Vec<Vec<f32>> = vec![vec![]];
        for (slot, d) in scene.meshes.iter().enumerate() {
            self.slots.push(ptr(&d.geometry));
            if d.geometry.indices.is_empty() || d.instances.is_empty() {
                continue;
            }
            let key = ptr(&d.geometry);
            let mesh = match index.get(&key) {
                Some(&i) => i,
                None => {
                    // Its own, else what was in its place last time when only
                    // its points and normals moved (a deforming mesh), its
                    // vertices written into the same buffer.
                    let moved = was.get(slot).and_then(|prev| {
                        let m = old.get(prev)?;
                        let same = m.parts.len() == 1
                            && m.geometry.vertices.len() == d.geometry.vertices.len()
                            && m.geometry.indices == d.geometry.indices;
                        same.then_some(*prev)
                    });
                    let gm = match (old.remove(&key), moved) {
                        (Some(m), _) => m,
                        (None, Some(prev)) => {
                            let mut m = old.remove(&prev).expect("found above");
                            queue.write_buffer(&m.parts[0].0, 0, bytemuck::cast_slice(&d.geometry.vertices));
                            m.geometry = d.geometry.clone();
                            m
                        }
                        (None, None) => GpuMesh {
                            geometry: d.geometry.clone(),
                            parts: parts(&d.geometry, if d.edges { 2 } else { 3 })
                                .into_iter()
                                .map(|(v, i)| {
                                    (
                                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                            label: Some("viewer vertices"),
                                            contents: bytemuck::cast_slice(&v),
                                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
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
                    index.insert(key, meshes.len() - 1);
                    meshes.len() - 1
                }
            };
            let mut instances = vec![];
            for c in d.instances.chunks(per) {
                if packs.last().is_some_and(|p| p.len() + c.len() > per) {
                    packs.push(vec![]);
                }
                let at = packs.len() - 1;
                let offset = (packs[at].len() * 4) as u64;
                packs[at].extend_from_slice(c);
                instances.push((at, offset, (c.len() / INSTANCE_FLOATS) as u32));
            }
            self.draws.push(Draw {
                mesh,
                instances,
                blend: d.blend,
                edges: d.edges,
                material: d.material,
            });
        }
        self.meshes = meshes;
        self.instance_buffers = packs
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("viewer instances"),
                    contents: bytemuck::cast_slice(p),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();
        self.line_buffer = line_buffer(device, &scene.lines);
        self.top_buffer = line_buffer(device, &scene.top);
        self.generation = scene.generation;
    }

    /// The picture: the scene drawn, or (`shown`) what is shown in its place,
    /// the overlays over it when `shown` is true.
    fn render(&self, encoder: &mut wgpu::CommandEncoder, t: &Target, clear: [f32; 3], shown: Option<bool>) {
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
        if let Some(overlays) = shown {
            if let Some(r) = &self.rendered {
                pass.set_pipeline(&self.background);
                pass.set_bind_group(1, &r.group, &[]);
                pass.draw(0..3, 0..1);
            }
            if overlays {
                self.overlays(&mut pass);
            }
            return;
        }
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
        self.raised_lines(&mut pass);
        pass.set_pipeline(&self.blended);
        for d in self.draws.iter().filter(|d| d.blend && !d.edges) {
            self.draw_mesh(&mut pass, d);
        }
        self.top_lines(&mut pass);
    }

    /// Edit Poly's wireframe and vertices, on their surface.
    fn raised_lines(&self, pass: &mut wgpu::RenderPass<'_>) {
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
    }

    /// Over everything: the skeleton and the manipulator.
    fn top_lines(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.on_top);
        for (buffer, count) in self.top_buffer.iter().chain(&self.frame_lines[1]) {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
    }

    /// Over a render: what you work with, not the scene drawn.
    fn overlays(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.raised_lines(pass);
        self.top_lines(pass);
    }

    fn draw_mesh(&self, pass: &mut wgpu::RenderPass<'_>, d: &Draw) {
        let m = &self.meshes[d.mesh];
        let material = d.material.and_then(|i| self.materials.get(i)).unwrap_or(&self.plain);
        pass.set_bind_group(1, material, &[]);
        for (vertices, indices, count) in &m.parts {
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for &(buffer, offset, copies) in &d.instances {
                let bytes = copies as u64 * (INSTANCE_FLOATS * 4) as u64;
                pass.set_vertex_buffer(1, self.instance_buffers[buffer].slice(offset..offset + bytes));
                pass.draw_indexed(0..*count, 0, 0..copies);
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
        format: MAP,
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

/// The pass laying the picture into egui's, for one display and view: the
/// view's transform as WGSL before `blit.wgsl`, its uniforms and LUTs in bind
/// group 1.
struct Display {
    view: crate::colour::View,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

/// The display transform `from` colour space (empty: the scene's) to `view`, as
/// WGSL with its uniforms and LUTs in bind group 1: OpenColorIO's, or sRGB's
/// curve when it cannot be made (the reason printed, for `who`).
pub(crate) fn colour_shader(from: &str, view: &crate::colour::View, who: &str) -> xms_colour::Shader {
    crate::colour::Colour::get()
        .shader(from, view, "imago_display", 1)
        .unwrap_or_else(|e| {
            eprintln!("{who} colour: {e}; shown through sRGB's curve");
            xms_colour::Shader {
                text: include_str!("display_fallback.wgsl").into(),
                function: "imago_display".into(),
                group: 1,
                uniforms: None,
                textures: vec![],
            }
        })
}

/// A display transform's uniforms and LUTs on the GPU: its bind group and the
/// group's layout (group 1 of the shaders that call `imago_display`).
pub(crate) fn colour_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shader: &xms_colour::Shader,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = vec![];
    let mut buffers: Vec<(u32, wgpu::Buffer)> = vec![];
    if let Some((binding, bytes)) = &shader.uniforms {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: *binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(bytes.len() as u64),
            },
            count: None,
        });
        buffers.push((
            *binding,
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("display uniforms"),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            }),
        ));
    }
    let dimension = |d: u32| match d {
        1 => (wgpu::TextureDimension::D1, wgpu::TextureViewDimension::D1),
        2 => (wgpu::TextureDimension::D2, wgpu::TextureViewDimension::D2),
        _ => (wgpu::TextureDimension::D3, wgpu::TextureViewDimension::D3),
    };
    let mut views: Vec<(u32, wgpu::TextureView)> = vec![];
    for t in &shader.textures {
        let (dim, view_dim) = dimension(t.dimensions);
        // Read with textureLoad alone: no filtering needed.
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: t.binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: view_dim,
                multisampled: false,
            },
            count: None,
        });
        let size = wgpu::Extent3d {
            width: t.size[0],
            height: t.size[1],
            depth_or_array_layers: t.size[2],
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("display LUT"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: dim,
            format: if t.rgba {
                wgpu::TextureFormat::Rgba32Float
            } else {
                wgpu::TextureFormat::R32Float
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &t.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(t.bytes_per_row),
                rows_per_image: Some(t.size[1]),
            },
            size,
        );
        views.push((
            t.binding,
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(view_dim),
                ..Default::default()
            }),
        ));
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("display"),
        entries: &entries,
    });
    let mut resources: Vec<wgpu::BindGroupEntry> = buffers
        .iter()
        .map(|(binding, b)| wgpu::BindGroupEntry {
            binding: *binding,
            resource: b.as_entire_binding(),
        })
        .collect();
    resources.extend(views.iter().map(|(binding, v)| wgpu::BindGroupEntry {
        binding: *binding,
        resource: wgpu::BindingResource::TextureView(v),
    }));
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("display"),
        layout: &layout,
        entries: &resources,
    });
    (layout, group)
}

/// The Viewer's display pass for `view`. A view whose transform cannot be made
/// is shown through sRGB's curve, and the reason printed.
fn display(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    blit_layout: &wgpu::BindGroupLayout,
    egui_format: wgpu::TextureFormat,
    egui_samples: u32,
    view: &crate::colour::View,
) -> Display {
    let shader = colour_shader("", view, "Viewer");
    let (layout, group) = colour_group(device, queue, &shader);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("viewer blit"),
        source: wgpu::ShaderSource::Wgsl(format!("{}\n{}", shader.text, include_str!("blit.wgsl")).into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("viewer blit"),
        bind_group_layouts: &[Some(blit_layout), Some(&layout)],
        immediate_size: 0,
    });
    // An sRGB target encodes what it is given: the display's colours are
    // decoded for it.
    let fs = if egui_format.is_srgb() {
        "fs_blit_srgb"
    } else {
        "fs_blit"
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("viewer blit"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_blit"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
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
    Display {
        view: view.clone(),
        pipeline,
        group,
    }
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
        if gpu.display.view != f.display {
            gpu.display = display(
                device,
                queue,
                &gpu.blit_layout,
                gpu.egui_format,
                gpu.egui_samples,
                &f.display,
            );
        }
        if gpu.generation != f.scene.generation {
            gpu.upload(device, queue, &f.scene);
        }
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
        match &f.background {
            Some(Background::Live(p)) => gpu.ensure_rendered(device, queue, p),
            Some(Background::Still(image)) => gpu.ensure_still(device, queue, image),
            None => gpu.rendered = None,
        }
        if let Some(c) = f.capture.as_ref().filter(|c| c.wanted()) {
            gpu.capture(device, queue, c, f.eye, f.clear);
        }
        gpu.write_view(queue, f.view_proj, f.eye, f.size);
        if let Some(t) = &gpu.target {
            let overlays = f.background.as_ref().map(|b| matches!(b, Background::Live(_)));
            gpu.render(encoder, t, f.clear, overlays);
        }
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
        pass.set_pipeline(&gpu.display.pipeline);
        pass.set_bind_group(0, &t.blit, &[]);
        pass.set_bind_group(1, &gpu.display.group, &[]);
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
