// The Viewer's finished picture, into the interface. The picture is linear
// light (an sRGB texture, read back as linear); the interface's target is
// not sRGB, so it is encoded on the way.

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle over the whole rectangle.
@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: Out;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_blit_encode(i: Out) -> @location(0) vec4<f32> {
    let c = textureSample(picture, picture_sampler, i.uv);
    return vec4<f32>(encode(c.rgb), 1.0);
}

@fragment
fn fs_blit_linear(i: Out) -> @location(0) vec4<f32> {
    let c = textureSample(picture, picture_sampler, i.uv);
    return vec4<f32>(c.rgb, 1.0);
}
