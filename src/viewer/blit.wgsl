// The Viewer's finished picture, into the interface. The picture is linear
// light in the scene's colour space (a float texture); `imago_display`, the
// display transform of the view chosen (OpenColorIO's, put before this
// text), gives the colours the display is sent. On an sRGB target, which
// encodes what it is given, they are decoded first.

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

fn shown(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSample(picture, picture_sampler, uv);
    return imago_display(vec4<f32>(c.rgb, 1.0)).rgb;
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_blit(i: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(shown(i.uv), 1.0);
}

@fragment
fn fs_blit_srgb(i: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(decode(shown(i.uv)), 1.0);
}
