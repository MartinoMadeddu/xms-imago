// The Image Viewer's image: its shown layer as floats, as stored, through the
// look (exposure, which channel) and `imago_display`, OpenColorIO's transform
// from the image's colour space to the view chosen (put before this text),
// then the gamma. On an sRGB target, which encodes what it is given, the
// colours are decoded first.

@group(0) @binding(0) var image: texture_2d<f32>;

struct Look {
    // 2^exposure.
    gain: f32,
    gamma: f32,
    // 0 colour, 1 red, 2 green, 3 blue, 4 alpha, 5 luminance.
    show: u32,
    pad: u32,
    // Where the image is in the canvas, from 0 to 1: its corner, its size.
    place: vec4<f32>,
};
@group(0) @binding(1) var<uniform> look: Look;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle over the canvas (the callback's viewport); the image is placed
// in it by `look.place`, so it may be zoomed past the canvas's edges.
@vertex
fn vs_image(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: Out;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn shown(uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(image));
    let texel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - vec2<f32>(1.0)));
    let c = textureLoad(image, texel, 0);
    // Alpha is shown as it is: it is not light.
    if (look.show == 4u) {
        return vec3<f32>(clamp(c.a, 0.0, 1.0));
    }
    var rgb = c.rgb;
    if (look.show == 1u) { rgb = vec3<f32>(c.r); }
    if (look.show == 2u) { rgb = vec3<f32>(c.g); }
    if (look.show == 3u) { rgb = vec3<f32>(c.b); }
    if (look.show == 5u) { rgb = vec3<f32>(dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722))); }
    var d = max(imago_display(vec4<f32>(rgb * look.gain, 1.0)).rgb, vec3<f32>(0.0));
    if (look.gamma > 0.0 && abs(look.gamma - 1.0) > 1e-6) {
        d = pow(d, vec3<f32>(1.0 / look.gamma));
    }
    return clamp(d, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Where in the image a point of the canvas is; outside it, nothing is drawn
// (the canvas's grey stays).
fn in_image(uv: vec2<f32>) -> vec2<f32> {
    let t = (uv - look.place.xy) / look.place.zw;
    if (any(t < vec2<f32>(0.0)) || any(t >= vec2<f32>(1.0))) {
        discard;
    }
    return t;
}

@fragment
fn fs_image(i: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(shown(in_image(i.uv)), 1.0);
}

@fragment
fn fs_image_srgb(i: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(decode(shown(in_image(i.uv))), 1.0);
}
