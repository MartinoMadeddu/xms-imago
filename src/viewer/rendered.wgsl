// Viewer > Arnold: the render over the whole picture, scaled to it while a
// render of another size is still the last one in.

struct View {
    view_proj: mat4x4<f32>,
    eye:       vec4<f32>,
    viewport:  vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var rendered: texture_2d<f32>;

@vertex
fn vs_rendered(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_rendered(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(rendered));
    let p = vec2<i32>(at.xy * size / max(view.viewport.xy, vec2<f32>(1.0)));
    let c = textureLoad(rendered, clamp(p, vec2<i32>(0), vec2<i32>(size) - 1), 0);
    // Opaque: its alpha is the render's, not the picture's.
    return vec4<f32>(c.rgb, 1.0);
}
