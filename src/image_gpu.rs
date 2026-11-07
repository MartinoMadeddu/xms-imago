//! The Image Viewer's image on the GPU, as Gaffer and Houdini's COPs show
//! theirs: the shown layer uploaded once, as floats as stored; the colour
//! transform (the image's colour space to the view chosen) OpenColorIO's
//! shader; exposure, gamma and which channel values given to it. Changing the
//! space or the view makes a shader again, not the pixels: at once, at any
//! size of image.

use std::sync::Arc;

use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use xms_image::{Image, Layer, Look, Show};

/// What to draw: the image, its layer and look, from which colour space,
/// through which view.
pub struct ImagePaint {
    pub image: Arc<Image>,
    pub layer: Layer,
    pub show: Show,
    pub look: Look,
    /// The colour space the pixels are in; empty, the scene's.
    pub from: String,
    pub view: crate::colour::View,
    /// Where the image is in the canvas (the callback's rectangle), from 0 to
    /// 1: its corner and its size. Zoomed in, past the canvas's edges.
    pub place: [f32; 4],
}

/// The pipeline for one colour space and view.
struct Display {
    key: (String, crate::colour::View),
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

/// The image on the GPU: uploaded again only for another image or layer.
struct Uploaded {
    key: (usize, String),
    group: wgpu::BindGroup,
    look: wgpu::Buffer,
}

/// What the paint callback keeps between frames.
pub struct ImageGpu {
    layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    samples: u32,
    display: Option<Display>,
    uploaded: Option<Uploaded>,
}

/// The largest side of an image the GPU takes, once installed: 0, no GPU.
static MAX_SIDE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether the GPU draws this image (else the view's own CPU path): a GPU
/// there, and the image within its largest texture.
pub fn fits(image: &Image) -> bool {
    let max = MAX_SIDE.load(std::sync::atomic::Ordering::Relaxed);
    image.width <= max && image.height <= max && image.width * image.height > 0
}

/// Put the Image Viewer's GPU state where its paint callback finds it.
pub fn install(render_state: &egui_wgpu::RenderState, samples: u32) {
    MAX_SIDE.store(render_state.device.limits().max_texture_dimension_2d as usize, std::sync::atomic::Ordering::Relaxed);
    let layout = render_state.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("image"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
        ],
    });
    render_state.renderer.write().callback_resources.insert(ImageGpu {
        layout,
        format: render_state.target_format,
        samples,
        display: None,
        uploaded: None,
    });
}

/// The look's values for the shader: gain, gamma, which channel, and where the
/// image is placed.
fn look_bytes(show: Show, look: &Look, place: [f32; 4]) -> [u8; 32] {
    let show = match show {
        Show::Colour => 0u32,
        Show::Red => 1,
        Show::Green => 2,
        Show::Blue => 3,
        Show::Alpha => 4,
        Show::Luminance => 5,
    };
    let mut b = [0u8; 32];
    b[0..4].copy_from_slice(&look.exposure.exp2().to_le_bytes());
    b[4..8].copy_from_slice(&look.gamma.to_le_bytes());
    b[8..12].copy_from_slice(&show.to_le_bytes());
    for (k, v) in place.iter().enumerate() {
        b[16 + k * 4..20 + k * 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

/// The layer as RGBA floats, as stored: a channel it lacks 0, alpha 1.
fn layer_pixels(image: &Image, layer: &Layer) -> Vec<f32> {
    let n = image.width * image.height;
    let channel = |k: usize| layer.rgba[k].and_then(|c| image.channels.get(c)).map(|c| &c.values);
    let (r, g, b, a) = (channel(0), channel(1), channel(2), channel(3));
    // One channel on its own is grey.
    let (g, b) = if g.is_none() && b.is_none() { (r, r) } else { (g, b) };
    let mut out = Vec::with_capacity(n * 4);
    for p in 0..n {
        let at = |c: Option<&Vec<f32>>, or: f32| c.and_then(|v| v.get(p)).copied().unwrap_or(or);
        out.extend([at(r, 0.0), at(g, 0.0), at(b, 0.0), at(a, 1.0)]);
    }
    out
}

impl ImageGpu {
    fn ensure_display(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, from: &str, view: &crate::colour::View) {
        let key = (from.to_string(), view.clone());
        if self.display.as_ref().is_some_and(|d| d.key == key) {
            return;
        }
        let shader = crate::viewer::gpu::colour_shader(from, view, "Image Viewer");
        let (colour_layout, group) = crate::viewer::gpu::colour_group(device, queue, &shader);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image viewer"),
            source: wgpu::ShaderSource::Wgsl(format!("{}\n{}", shader.text, include_str!("image_gpu.wgsl")).into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image viewer"),
            bind_group_layouts: &[Some(&self.layout), Some(&colour_layout)],
            immediate_size: 0,
        });
        let fs = if self.format.is_srgb() { "fs_image_srgb" } else { "fs_image" };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image viewer"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_image"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: self.samples.max(1),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        self.display = Some(Display { key, pipeline, group });
    }

    fn ensure_image(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &Arc<Image>, layer: &Layer) {
        let key = (Arc::as_ptr(image) as usize, layer.name.clone());
        if self.uploaded.as_ref().is_some_and(|u| u.key == key) {
            return;
        }
        let size = wgpu::Extent3d {
            width: image.width as u32,
            height: image.height as u32,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("image viewer image"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&layer_pixels(image, layer)),
        );
        let look = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image viewer look"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image viewer image"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: look.as_entire_binding(),
                },
            ],
        });
        self.uploaded = Some(Uploaded { key, group, look });
    }
}

impl CallbackTrait for ImagePaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = resources.get_mut::<ImageGpu>() else {
            return vec![];
        };
        gpu.ensure_display(device, queue, &self.from, &self.view);
        gpu.ensure_image(device, queue, &self.image, &self.layer);
        if let Some(u) = &gpu.uploaded {
            queue.write_buffer(&u.look, 0, &look_bytes(self.show, &self.look, self.place));
        }
        vec![]
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &CallbackResources) {
        let Some(gpu) = resources.get::<ImageGpu>() else { return };
        let (Some(d), Some(u)) = (&gpu.display, &gpu.uploaded) else { return };
        pass.set_pipeline(&d.pipeline);
        pass.set_bind_group(0, &u.group, &[]);
        pass.set_bind_group(1, &d.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_goes_up_as_rgba_with_what_it_lacks_filled_in() {
        let ch = |name: &str, v: Vec<f32>| xms_image::Channel { name: name.into(), values: v };
        let image = Image {
            width: 2,
            height: 1,
            origin: (0, 0),
            frame: (0, 0, 2, 1),
            channels: vec![ch("R", vec![0.5, 2.0]), ch("G", vec![0.25, 0.0]), ch("B", vec![0.0, 1.0]), ch("Z", vec![7.0, 8.0])],
        };
        let layers = image.layers();
        let rgb = layers.iter().find(|l| l.rgba[0].is_some()).unwrap();
        assert_eq!(layer_pixels(&image, rgb), vec![0.5, 0.25, 0.0, 1.0, 2.0, 0.0, 1.0, 1.0], "as stored, alpha 1");
        let look = Look { exposure: 1.0, gamma: 2.2, srgb: false };
        let b = look_bytes(Show::Alpha, &look, [0.25, 0.0, 0.5, 1.0]);
        assert_eq!(f32::from_le_bytes(b[0..4].try_into().unwrap()), 2.0);
        assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), 4);
        assert_eq!(f32::from_le_bytes(b[24..28].try_into().unwrap()), 0.5);
    }
}
