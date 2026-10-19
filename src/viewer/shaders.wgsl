// The Viewer's 3D view. Colours are linear light; the view is drawn into an
// sRGB texture, so blending happens in linear light, as Bevy drew it.

struct View {
    view_proj: mat4x4<f32>,
    eye:       vec4<f32>,
    // The picture's size in pixels (x, y).
    viewport:  vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: View;

// A material's textures; a white pixel where there is none.
@group(1) @binding(0) var colour_map:   texture_2d<f32>;
@group(1) @binding(1) var emissive_map: texture_2d<f32>;
@group(1) @binding(2) var map_sampler:  sampler;
// Where each map is read from: UVs scaled (xy) then offset (zw). An
// ordinary texture is read as it is; a UDIM texture's tiles lie side by
// side in one image, 1001 at the bottom left.
struct MapPlace {
    colour:   vec4<f32>,
    emissive: vec4<f32>,
};
@group(1) @binding(3) var<uniform> map_place: MapPlace;

struct MeshIn {
    @location(0) pos:    vec3<f32>,
    @location(1) nrm:    vec3<f32>,
    @location(2) m0:     vec4<f32>,
    @location(3) m1:     vec4<f32>,
    @location(4) m2:     vec4<f32>,
    @location(5) m3:     vec4<f32>,
    @location(6) colour: vec4<f32>,
    // x: lit, y: colour map, z: emissive map, w: cut out by the colour map's alpha.
    @location(7) flags:  vec4<f32>,
    @location(8) uv:     vec2<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) nrm:    vec3<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) flags:  vec4<f32>,
    @location(3) uv:     vec2<f32>,
};

fn mesh_out(v: MeshIn) -> MeshOut {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var o: MeshOut;
    o.clip = view.view_proj * (m * vec4<f32>(v.pos, 1.0));
    o.nrm = (m * vec4<f32>(v.nrm, 0.0)).xyz;
    o.colour = v.colour;
    o.flags = v.flags;
    o.uv = v.uv;
    return o;
}

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    return mesh_out(v);
}

@fragment
fn fs_mesh(i: MeshOut) -> @location(0) vec4<f32> {
    // Textures are read before any branch: sampling needs every pixel of a
    // quad to take the same path.
    let colour_uv = i.uv * map_place.colour.xy + map_place.colour.zw;
    let emissive_uv = i.uv * map_place.emissive.xy + map_place.emissive.zw;
    let tex = mix(vec4<f32>(1.0), textureSample(colour_map, map_sampler, colour_uv), i.flags.y);
    let glow = textureSample(emissive_map, map_sampler, emissive_uv).rgb * i.flags.z;
    let base = i.colour * tex;
    if i.flags.w > 0.5 && base.a < 0.5 {
        discard;
    }
    if i.flags.x < 0.5 {
        return base;
    }
    // A key light and the sky, lighting both sides alike.
    let len = length(i.nrm);
    let n = select(vec3<f32>(0.0, 1.0, 0.0), i.nrm / len, len > 1e-8);
    let key = abs(dot(n, normalize(vec3<f32>(0.4, 0.8, 0.45))));
    let sky = 0.5 + 0.5 * abs(n.y);
    return vec4<f32>(base.rgb * (0.22 + 0.18 * sky + 0.6 * key) + glow, base.a);
}

// Polygon edges: a mesh's edges drawn at each of its copies, as the surfaces
// are. A little towards the camera, in proportion to the distance, so an
// edge wins over the surface it lies on.
@vertex
fn vs_edge(v: MeshIn) -> MeshOut {
    var o = mesh_out(v);
    o.clip.z = o.clip.z * 1.002;
    return o;
}

struct LineIn {
    @location(0) pos:    vec3<f32>,
    @location(1) colour: vec4<f32>,
};

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
};

fn line_out(v: LineIn) -> LineOut {
    var o: LineOut;
    o.clip = view.view_proj * vec4<f32>(v.pos, 1.0);
    o.colour = v.colour;
    return o;
}

@vertex
fn vs_line(v: LineIn) -> LineOut {
    return line_out(v);
}

// A line on its surface (Edit Poly's wireframe): a little towards the
// camera, in proportion to the distance.
@vertex
fn vs_line_raised(v: LineIn) -> LineOut {
    var o = line_out(v);
    o.clip.z = o.clip.z * 1.002;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.colour;
}

// Edit Poly's vertices: a round dot of a fixed size in pixels, facing the
// camera, hidden by the surfaces in front of it. Each dot is a quad of two
// triangles made from its corner number.
struct PointIn {
    @builtin(vertex_index) corner: u32,
    @location(0) pos:    vec3<f32>,
    @location(1) colour: vec4<f32>,
    // Radius, in pixels.
    @location(2) size:   f32,
};

struct PointOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) uv:     vec2<f32>,
};

@vertex
fn vs_point(v: PointIn) -> PointOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let c = corners[v.corner];
    var o: PointOut;
    o.clip = view.view_proj * vec4<f32>(v.pos, 1.0);
    // On its surface, a little towards the camera.
    o.clip.z = o.clip.z * 1.004;
    o.clip = vec4<f32>(o.clip.xy + c * v.size * 2.0 / view.viewport.xy * o.clip.w, o.clip.zw);
    o.colour = v.colour;
    o.uv = c;
    return o;
}

@fragment
fn fs_point(i: PointOut) -> @location(0) vec4<f32> {
    let d = length(i.uv);
    if d > 1.0 {
        discard;
    }
    // A dark rim, so a dot reads on any surface.
    if d > 0.7 {
        return vec4<f32>(0.01, 0.01, 0.012, 0.85);
    }
    return i.colour;
}
