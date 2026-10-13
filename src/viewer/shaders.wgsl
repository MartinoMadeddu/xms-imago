// The Viewer's 3D view. Colours are linear light; the view is drawn into an
// sRGB texture, so blending happens in linear light, as Bevy drew it.

struct View {
    view_proj: mat4x4<f32>,
    eye:       vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: View;

struct MeshIn {
    @location(0) pos:    vec3<f32>,
    @location(1) nrm:    vec3<f32>,
    @location(2) m0:     vec4<f32>,
    @location(3) m1:     vec4<f32>,
    @location(4) m2:     vec4<f32>,
    @location(5) m3:     vec4<f32>,
    @location(6) colour: vec4<f32>,
    @location(7) flags:  vec4<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) nrm:    vec3<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) lit:    f32,
};

fn mesh_out(v: MeshIn) -> MeshOut {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var o: MeshOut;
    o.clip = view.view_proj * (m * vec4<f32>(v.pos, 1.0));
    o.nrm = (m * vec4<f32>(v.nrm, 0.0)).xyz;
    o.colour = v.colour;
    o.lit = v.flags.x;
    return o;
}

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    return mesh_out(v);
}

@fragment
fn fs_mesh(i: MeshOut) -> @location(0) vec4<f32> {
    if i.lit < 0.5 {
        return i.colour;
    }
    // A key light and the sky, lighting both sides alike.
    let len = length(i.nrm);
    let n = select(vec3<f32>(0.0, 1.0, 0.0), i.nrm / len, len > 1e-8);
    let key = abs(dot(n, normalize(vec3<f32>(0.4, 0.8, 0.45))));
    let sky = 0.5 + 0.5 * abs(n.y);
    return vec4<f32>(i.colour.rgb * (0.22 + 0.18 * sky + 0.6 * key), i.colour.a);
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

@vertex
fn vs_line(v: LineIn) -> LineOut {
    var o: LineOut;
    o.clip = view.view_proj * vec4<f32>(v.pos, 1.0);
    o.colour = v.colour;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.colour;
}
