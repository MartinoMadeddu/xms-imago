use glam::Vec3;

// ============================================================================
// SHARED IDs
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeId(pub usize);

/// Primitive paths dragged out of the Scene Explorer (the selected rows, or
/// the one row dragged): dropped on an operator, its Prims Location tab or
/// its "+ Add Location", they join the operator's Primitive Locations.
#[derive(Clone, Debug)]
pub struct DraggedPrims(pub Vec<String>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IceGraphId(pub usize);

// Scene object ID — one per visible object in the scene explorer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneObjectId(pub usize);

// ============================================================================
// OUTER NODE TYPES
// ============================================================================

/// Vec3 stored as a plain array in saved graphs.
mod vec3_array {
    use glam::Vec3;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &Vec3, s: S) -> Result<S::Ok, S::Error> {
        v.to_array().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec3, D::Error> {
        <[f32; 3]>::deserialize(d).map(Vec3::from_array)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum NodeType {
    /// The shapes. `location` is the Primitive Location: the path of the
    /// primitive the shape is, in the scene (`/cube`, `/set/props/box1`).
    /// Its parents are plain Xforms. Graphs saved before it existed get the
    /// default.
    CreateCube   { size: f32, #[serde(default = "cube_location")] location: String },
    CreateSphere { radius: f32, segments: u32, #[serde(default = "sphere_location")] location: String },
    CreateGrid   { rows: u32, cols: u32, size: f32, #[serde(default = "grid_location")] location: String },
    LoadUsd      { path: String },
    /// Write what the network changed in its stages as a USD override layer.
    WriteUsd     { path: String },
    /// Mark packed primitives whose path matches a pattern as picked.
    PickPrims    { pattern: String },
    /// Remove packed primitives whose path matches a pattern, or keep only those.
    PrunePrims   { pattern: String, keep: bool },
    /// One value per primitive, set on the prims its Prims Location tab
    /// names: a constant primvar (Object) or the prim's own attribute or
    /// metadata (Property), one per row. See `core::props`.
    SetAttribute { rows: Vec<crate::core::props::SetRow> },
    /// Merge packed primitives into one mesh.
    UnpackPrims,
    /// Moves whatever comes in: a mesh, packed primitives (the picked ones,
    /// or every one), or a clip. A clip moves by its top joints, so a skinned
    /// mesh follows its skeleton once and is not moved a second time.
    /// `rotation` in radians, applied X then Y then Z.
    Transform    {
        #[serde(with = "vec3_array")] translation: Vec3,
        #[serde(with = "vec3_array")] rotation:    Vec3,
        #[serde(with = "vec3_array")] scale:       Vec3,
    },
    /// Two inputs as one. What it does follows `mode`: see `MergeMode`.
    /// Graphs saved before the mode existed get Auto (`graph_io`).
    Merge        { #[serde(default)] mode: MergeMode },
    /// An ICE node: an ICE graph run on what comes in. Saved as "Subnet" by
    /// earlier versions.
    #[serde(alias = "Subnet")]
    Ice          { id: IceGraphId, name: String },
    // The Output node is gone: the view flag says what the scene is. Graphs
    // saved with one have it taken out when read (`graph_io`).

    // ── Animation ────────────────────────────────────────────────────────────
    // Each of these outputs a whole clip. Time is not a parameter: the
    // timeline samples the clip of the selected node.
    LoadFbx      { path: String, take: u32 },
    TestClip     { seconds: f32, fps_num: u32, fps_den: u32 },
    RenameJoints { find: String, replace: String, strip_namespace: bool, prefix: String },
    /// Frames removed from the head and the tail of the incoming clip.
    TrimClip     { head: u32, tail: u32 },
    Retime       { fps_num: u32, fps_den: u32, mode: RetimeMode },
    SetTimecode  { hours: u32, minutes: u32, seconds: u32, frames: u32, drop_frame: bool },

    // ── Batch / export ───────────────────────────────────────────────────────
    /// One FBX out of a folder, chosen by index into the sorted file list.
    LoadFbxDir   { dir: String, index: u32, take: u32 },
    /// One output per entry: the picked joint and everything below it.
    SplitSkeleton { picks: Vec<SplitPick> },
    /// Single-frame neutral pose. `hip_height` is in centimetres.
    AutoTPose    { set_hip_height: bool, hip_height: f32 },
    /// Manual corrections on top of a pose.
    FixPose      { edits: Vec<crate::core::anim::PoseEdit> },
    /// Spheres and cylinders bound to the skeleton.
    ProxySkin    { thickness: f32 },
    /// Passes the clip through. Writing happens from the properties panel.
    /// `mesh`: also write the skinned mesh, skin and bind pose. Off, the
    /// file holds the skeleton and its motion only, which is what an engine
    /// imports onto a skeleton it already has. Graphs saved before the
    /// switch existed wrote the mesh, and still do.
    WriteFbx     { path: String, #[serde(default = "yes")] mesh: bool },

    // ── Mocap tools ──────────────────────────────────────────────────────────
    /// Swap left and right.
    MirrorClip,
    /// Gaussian filter over time. `radius` in frames.
    SmoothClip   { radius: u32, amount: f32, translations: bool },
    /// Hold the hips over their starting point.
    InPlace      { keep_height: bool, to_root: bool },
    /// Graphs saved before Transform took clips. Read, then turned into a
    /// Transform node (`graph_io`).
    TransformClip { translate: [f32; 3], rotate: [f32; 3], scale: f32 },
    /// First input followed by the second, cross-faded over `blend` frames.
    BlendClips   { blend: u32, align: bool },
    /// Ease the end into the start so the clip cycles.
    LoopClip     { blend: u32 },
    /// Motion of the first input on the skeleton of the second.
    Retarget,
    /// Says which joint is which part of a human, where the names do not.
    /// Slots not picked are read from the names. Body Collide and Retarget
    /// go by what the clip carries.
    Characterize { picks: crate::core::human::Picks },
    /// Hands and feet of the first input sent where the second input (the
    /// motion before a retarget) has them, scaled; then the human pass.
    Reach        { settings: crate::core::reach::ReachSettings },
    /// The Body Collide node's human pass on its own: the first input put
    /// back within human limits against the second (the clip before it was moved).
    HumanLimits  { limits: crate::core::human_ik::Limits },
    /// A node that does not exist yet: named inputs and outputs, each input
    /// handed to the output of the same number unchanged, and a description
    /// of what it should do. Saved with the graph, to show how a new node
    /// should work.
    Placeholder  { inputs: Vec<String>, outputs: Vec<String>, #[serde(default)] description: String },
    /// Skin weights by proximity: each point of the geometry weighted to the
    /// nearest bones of the skeleton, in the skeleton's first frame.
    Capture      { falloff: f32 },
    /// Geometry with skin weights, the skeleton it was weighted in, and an
    /// animation: the animation with that skin, joints matched by name.
    Skin,
    TimeWarp     { speed: f32, reverse: bool },
    /// Remove joints whose name contains one of the comma-separated words.
    PruneJoints  { words: String },
    /// Put the lowest point of the clip at `height` (metres).
    FloorClip    { height: f32 },

    // ── Ragdoll ──────────────────────────────────────────────────────────────
    /// Every mesh of an FBX file, as packed primitives: a set to collide with.
    LoadFbxMesh  { path: String },
    /// Graphs saved before Body Collide showed its own bodies. Read, then
    /// folded into the Body Collide node before it (`graph_io`).
    Calamari     { hulls: bool, detail: u32 },
    /// Body Collide: keeps the character out of a collider mesh and out of
    /// itself. Solving is started from the properties panel. Saved under its
    /// first name, so older graphs open. `view` is what the viewport draws
    /// of the character, not what the node puts out.
    Ragdoll      { settings: crate::ragdoll::Settings, #[serde(default)] view: BodyView,
                   /// The human pass on the result: not part of the solve.
                   #[serde(default)] limits: crate::core::human_ik::Limits },

    // ── UV ───────────────────────────────────────────────────────────────────
    /// Make texture coordinates. `angle` (degrees) limits how far a chart's
    /// normals may spread; `margin` is the gap between charts.
    UvUnwrap     { method: crate::core::uv::UvMethod, angle: f32, margin: f32, axis: usize,
                   /// UDIM tiles to spread the elements over. One: everything in the unit square.
                   #[serde(default = "one_tile")] tiles: u32 },
    /// Move, turn and scale the whole UV layout.
    UvTransform  { offset: [f32; 2], rotate: f32, scale: [f32; 2] },
    /// Move, turn and scale single UV islands.
    UvEdit       { edits: Vec<crate::core::uv::IslandEdit> },

    // ── Modelling ────────────────────────────────────────────────────────────
    /// Polygon modelling in one node: an ordered list of operations, each
    /// with its own selection, like the history of an Edit Poly modifier.
    EditPoly {
        ops:     Vec<crate::core::poly::PolyOp>,
        /// Selection being built for the next operation.
        pending: crate::core::poly::PolySelection,
        /// Operation whose selection is being edited in the viewport, which
        /// then shows the mesh as it enters that operation. None: `pending`.
        edit:    Option<usize>,
        /// Collapse every earlier operation when a new one is added.
        #[serde(default)]
        auto_collapse: bool,
    },
}

/// What the viewport draws of a character that Body Collide works on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BodyView {
    /// Its skin, as the clip carries it.
    #[default]
    Skin,
    /// The skin cut into one rigid piece per body.
    Pieces,
    /// The convex hull of each body, as the solver collides it.
    Hulls,
}

/// What one output of the Split node keeps.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SplitPick {
    /// Nth character found in the clip. Survives name changes between files.
    Character(u32),
    /// Joint with this exact name.
    Joint(String),
}

pub const DEFAULT_WRITE_PATH: &str = "{dir}/split/{file}_{char}.fbx";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RetimeMode {
    /// Keep duration, interpolate new samples.
    Resample,
    /// Keep samples, relabel the rate (changes speed).
    Reinterpret,
}

/// How a Merge joins its inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MergeMode {
    /// By what comes into the first input: primitives are merged as
    /// primitives, a plain mesh or a clip is flattened.
    #[default]
    Auto,
    /// Primitives side by side, each keeping its path. On the same path the
    /// second input is the stronger one.
    Primitives,
    /// Everything as one plain mesh, as Unpack would make it.
    Flatten,
}

/// A Primitive Location as typed, made a valid USD path: segments split on
/// `/` (empty ones dropped), each a USD identifier (letters, digits and
/// `_`, not starting with a digit; anything else becomes `_`). Nothing
/// usable gives `fallback`.
pub fn prim_location(text: &str, fallback: &str) -> String {
    let segments: Vec<String> = text.split('/').map(str::trim).filter(|s| !s.is_empty()).map(|seg| {
        let mut id: String = seg.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
        if id.starts_with(|c: char| c.is_ascii_digit()) { id.insert(0, '_'); }
        id
    }).collect();
    if segments.is_empty() { fallback.to_string() } else { format!("/{}", segments.join("/")) }
}

pub fn cube_location()   -> String { "/cube".into() }
pub fn sphere_location() -> String { "/sphere".into() }
pub fn grid_location()   -> String { "/grid".into() }

impl NodeType {
    /// A cube at its default Primitive Location.
    pub fn cube(size: f32) -> Self { NodeType::CreateCube { size, location: cube_location() } }
    /// A sphere at its default Primitive Location.
    pub fn sphere(radius: f32, segments: u32) -> Self { NodeType::CreateSphere { radius, segments, location: sphere_location() } }
    /// A grid at its default Primitive Location.
    pub fn grid(rows: u32, cols: u32, size: f32) -> Self { NodeType::CreateGrid { rows, cols, size, location: grid_location() } }
    /// A Merge that goes by its first input.
    pub fn merge() -> Self { NodeType::Merge { mode: MergeMode::Auto } }

    /// Nodes that create animation data with no input.
    pub fn is_anim_generator(&self) -> bool {
        matches!(self, NodeType::LoadFbx { .. } | NodeType::LoadFbxDir { .. } | NodeType::TestClip { .. })
    }

    /// Nodes a clip goes through: the clip nodes, and Transform.
    pub fn passes_clips(&self) -> bool { self.is_anim() || matches!(self, NodeType::Transform { .. } | NodeType::Placeholder { .. }) }

    /// Nodes whose output is a clip.
    pub fn is_anim(&self) -> bool {
        self.is_anim_generator() || matches!(self,
            NodeType::RenameJoints { .. } | NodeType::TrimClip { .. } | NodeType::Retime { .. }
            | NodeType::SetTimecode { .. } | NodeType::SplitSkeleton { .. } | NodeType::AutoTPose { .. }
            | NodeType::FixPose { .. } | NodeType::ProxySkin { .. } | NodeType::WriteFbx { .. }
            | NodeType::MirrorClip | NodeType::SmoothClip { .. } | NodeType::InPlace { .. }
            | NodeType::TransformClip { .. } | NodeType::BlendClips { .. } | NodeType::LoopClip { .. }
            | NodeType::Retarget | NodeType::Characterize { .. } | NodeType::Reach { .. } | NodeType::HumanLimits { .. } | NodeType::Skin | NodeType::TimeWarp { .. } | NodeType::PruneJoints { .. }
            | NodeType::FloorClip { .. } | NodeType::Calamari { .. } | NodeType::Ragdoll { .. })
    }
}

pub fn node_type_icon(t: &NodeType) -> &'static str {
    match t {
        NodeType::CreateCube { .. }    => "◼",
        NodeType::CreateSphere { .. }  => "⚪",
        NodeType::CreateGrid { .. }    => "⊞",
        NodeType::LoadUsd { .. }       => "📂",
        NodeType::WriteUsd { .. }      => "📤",
        NodeType::PickPrims { .. }     => "👆",
        NodeType::PrunePrims { .. }    => "🚫",
        NodeType::SetAttribute { .. }  => "🏷",
        NodeType::UnpackPrims          => "📦",
        NodeType::Transform { .. }     => "⟲",
        NodeType::Merge { .. }         => "➕",
        NodeType::Ice { .. }        => "▣",
        NodeType::Placeholder { .. }   => "📝",
        NodeType::LoadFbx { .. }       => "🎬",
        NodeType::TestClip { .. }      => "🚶",
        NodeType::RenameJoints { .. }  => "✏",
        NodeType::TrimClip { .. }      => "✂",
        NodeType::Retime { .. }        => "⏱",
        NodeType::SetTimecode { .. }   => "🕐",
        NodeType::LoadFbxDir { .. }    => "📁",
        NodeType::SplitSkeleton { .. } => "Ψ",
        NodeType::AutoTPose { .. }     => "✚",
        NodeType::FixPose { .. }       => "🔧",
        NodeType::ProxySkin { .. }     => "⬟",
        NodeType::WriteFbx { .. }      => "💾",
        NodeType::EditPoly { .. }      => "🔨",
        NodeType::MirrorClip           => "↔",
        NodeType::SmoothClip { .. }    => "〰",
        NodeType::InPlace { .. }       => "📍",
        NodeType::TransformClip { .. } => "🔃",
        NodeType::BlendClips { .. }    => "🔀",
        NodeType::LoopClip { .. }      => "🔁",
        NodeType::Retarget             => "👥",
        NodeType::Characterize { .. }  => "👤",
        NodeType::Reach { .. }         => "👣",
        NodeType::HumanLimits { .. }   => "💪",
        NodeType::Capture { .. }       => "🔗",
        NodeType::Skin                 => "👕",
        NodeType::TimeWarp { .. }      => "⏩",
        NodeType::PruneJoints { .. }   => "🌿",
        NodeType::FloorClip { .. }     => "⬇",
        NodeType::LoadFbxMesh { .. }   => "📥",
        NodeType::Calamari { .. }      => "✂",
        NodeType::Ragdoll { .. }       => "💥",
        NodeType::UvUnwrap { .. }      => "🗺",
        NodeType::UvTransform { .. }   => "📌",
        NodeType::UvEdit { .. }        => "✋",
    }
}

pub fn node_type_label(t: &NodeType) -> &'static str {
    match t {
        NodeType::CreateCube { .. }    => "Cube",
        NodeType::CreateSphere { .. }  => "Sphere",
        NodeType::CreateGrid { .. }    => "Grid",
        NodeType::LoadUsd { .. }       => "Load USD",
        NodeType::WriteUsd { .. }      => "Write USD",
        NodeType::PickPrims { .. }     => "Pick Primitives",
        NodeType::PrunePrims { .. }    => "Prune Primitives",
        NodeType::SetAttribute { .. }  => "Set Attribute",
        NodeType::UnpackPrims          => "Unpack",
        NodeType::Transform { .. }     => "Transform",
        NodeType::Merge { .. }         => "Merge",
        NodeType::Ice { .. }        => "ICE",
        NodeType::Placeholder { .. }   => "Placeholder",
        NodeType::LoadFbx { .. }       => "Load FBX",
        NodeType::TestClip { .. }      => "Test Clip",
        NodeType::RenameJoints { .. }  => "Rename Joints",
        NodeType::TrimClip { .. }      => "Trim",
        NodeType::Retime { .. }        => "Retime",
        NodeType::SetTimecode { .. }   => "Set Timecode",
        NodeType::LoadFbxDir { .. }    => "Load FBX Folder",
        NodeType::SplitSkeleton { .. } => "Split Characters",
        NodeType::AutoTPose { .. }     => "Auto T-Pose",
        NodeType::FixPose { .. }       => "Fix Pose",
        NodeType::ProxySkin { .. }     => "Proxy Skin",
        NodeType::WriteFbx { .. }      => "Write FBX",
        NodeType::EditPoly { .. }      => "Edit Poly",
        NodeType::MirrorClip           => "Mirror",
        NodeType::SmoothClip { .. }    => "Smooth",
        NodeType::InPlace { .. }       => "In Place",
        NodeType::TransformClip { .. } => "Transform Clip",
        NodeType::BlendClips { .. }    => "Blend",
        NodeType::LoopClip { .. }      => "Loop",
        NodeType::Retarget             => "Retarget",
        NodeType::Characterize { .. }  => "Characterize",
        NodeType::Reach { .. }         => "Reach",
        NodeType::HumanLimits { .. }   => "Human Limits",
        NodeType::Capture { .. }       => "Capture",
        NodeType::Skin                 => "Skin",
        NodeType::TimeWarp { .. }      => "Time Warp",
        NodeType::PruneJoints { .. }   => "Prune Joints",
        NodeType::FloorClip { .. }     => "Floor",
        NodeType::LoadFbxMesh { .. }   => "Load FBX Mesh",
        NodeType::Calamari { .. }      => "Calamari",
        NodeType::Ragdoll { .. }       => "Body Collide",
        NodeType::UvUnwrap { .. }      => "UV Unwrap",
        NodeType::UvTransform { .. }   => "UV Transform",
        NodeType::UvEdit { .. }        => "UV Edit",
    }
}

// ============================================================================
// SUBNET NODE TYPES
// ============================================================================

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum IceGraphNodeType {
    SubInput,
    SubOutput,
    AddVec3,
    SubtractVec3,
    MultiplyVec3 { scalar: f32 },
    CrossProduct,
    Normalize,
    DotProduct,
    LerpVec3 { t: f32 },
    ConstVec3  { value: Vec3 },
    ConstFloat { value: f32  },
    ConstInt   { value: i32  },

    ScatterPoints { count: u32, seed: u32 },
    GetTemplate,
    CopyToPoints,
}

pub fn subnet_node_label(t: &IceGraphNodeType) -> &'static str {
    match t {
        IceGraphNodeType::SubInput            => "Graph Input",
        IceGraphNodeType::SubOutput           => "Graph Output",
        IceGraphNodeType::AddVec3             => "Add",
        IceGraphNodeType::SubtractVec3        => "Subtract",
        IceGraphNodeType::MultiplyVec3 { .. } => "Multiply",
        IceGraphNodeType::CrossProduct        => "Cross Product",
        IceGraphNodeType::Normalize           => "Normalize",
        IceGraphNodeType::DotProduct          => "Dot Product",
        IceGraphNodeType::LerpVec3 { .. }     => "Blend",
        IceGraphNodeType::ConstVec3 { .. }    => "Vector",
        IceGraphNodeType::ConstFloat { .. }   => "Number",
        IceGraphNodeType::ConstInt { .. }     => "Integer",
        IceGraphNodeType::ScatterPoints { .. } => "Scatter Points",
        IceGraphNodeType::GetTemplate          => "Get Template",
        IceGraphNodeType::CopyToPoints         => "Copy to Points",
    }
}

pub fn subnet_node_icon(t: &IceGraphNodeType) -> &'static str {
    match t {
        IceGraphNodeType::SubInput            => "▶",
        IceGraphNodeType::SubOutput           => "◀",
        IceGraphNodeType::AddVec3             => "+",
        IceGraphNodeType::SubtractVec3        => "-",
        IceGraphNodeType::MultiplyVec3 { .. } => "×",
        IceGraphNodeType::CrossProduct        => "×",
        IceGraphNodeType::Normalize           => "|v|",
        IceGraphNodeType::DotProduct          => "·",
        IceGraphNodeType::LerpVec3 { .. }     => "≈",
        IceGraphNodeType::ConstVec3 { .. }    => "→v",
        IceGraphNodeType::ConstFloat { .. }   => "→f",
        IceGraphNodeType::ConstInt { .. }     => "→i",
        IceGraphNodeType::ScatterPoints { .. } => "✳",
        IceGraphNodeType::GetTemplate          => "📄",
        IceGraphNodeType::CopyToPoints         => "📦",
    }
}

// ============================================================================
// SUBNET SOCKET VALUE
// ============================================================================

#[derive(Clone, Debug)]
pub enum IceGraphValue {
    Mesh(MeshData),
    Vec3(Vec3),
    Float(f32),
    Int(i32),
}

impl IceGraphValue {
    pub fn as_mesh(&self)  -> Option<&MeshData> { if let IceGraphValue::Mesh(m)  = self { Some(m)  } else { None } }
    pub fn as_vec3(&self)  -> Option<Vec3>       { if let IceGraphValue::Vec3(v)  = self { Some(*v) } else { None } }
    pub fn as_float(&self) -> Option<f32>        { if let IceGraphValue::Float(f) = self { Some(*f) } else { None } }
    pub fn as_int(&self)   -> Option<i32>        { if let IceGraphValue::Int(i)   = self { Some(*i) } else { None } }
}

// ============================================================================
// MESH DATA
// ============================================================================

// A single named primvar channel. The outer Vec is per-element
/// (vertex, face, or facevarying), the inner Vec is the tuple width (1, 2, 3, 4).
#[derive(Clone, Debug, Default)]
pub struct PrimVar {
    pub name:        String,
    pub interp:      PrimVarInterp,
    pub values:      Vec<Vec<f32>>,   // values[elem_idx][component]
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PrimVarInterp {
    #[default]
    Vertex,       // one value per vertex/point
    Uniform,      // one value per face
    FaceVarying,  // one value per face-vertex
    Constant,     // one value for the whole prim
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices:   Vec<[f32; 3]>,
    pub indices:    Vec<u32>,
    pub points:     Vec<[f32; 3]>,   // scatter / point-cloud points
    pub normals:    Vec<[f32; 3]>,   // per-vertex normals (may be empty)
    pub primvars:   Vec<PrimVar>,    // arbitrary extra channels
    /// Original face-vertex counts before triangulation (for face count display)
    pub face_count: usize,
    /// Polygons as vertex loops, when the mesh has them. `indices` then holds
    /// their triangulation. Empty for meshes that are only triangles.
    pub polys:      Vec<Vec<u32>>,
    /// Texture coordinates, one pair per triangle corner in the order of
    /// `indices`. Empty when the mesh has none.
    pub uvs:        Vec<[f32; 2]>,
    /// Curves: their control points end to end, and how many each one has.
    pub curve_points: Vec<[f32; 3]>,
    pub curve_counts: Vec<u32>,
    /// How the curves pass through their control points (USD BasisCurves).
    pub curve_basis:  CurveBasis,
    pub curve_wrap:   CurveWrap,
    /// Width of each of `points`, or one for them all. Empty, or 0 for a
    /// point: drawn at a size of the viewport's choosing.
    pub widths:       Vec<f32>,
}

/// How curves pass through their control points (USD `basis`, with `type`
/// linear as `Linear`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurveBasis {
    #[default]
    Linear,
    Bezier,
    Bspline,
    CatmullRom,
}

/// How curves end (USD `wrap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurveWrap {
    #[default]
    Nonperiodic,
    Periodic,
    Pinned,
}

impl MeshData {
    pub fn from_triangles(vertices: Vec<[f32; 3]>, indices: Vec<u32>) -> Self {
        let face_count = indices.len() / 3;
        Self { vertices, indices, face_count, ..Default::default() }
    }

    /// Mesh from polygons of any size. Triangulates them for drawing and
    /// keeps the polygons for modelling.
    pub fn from_polys(vertices: Vec<[f32; 3]>, polys: Vec<Vec<u32>>) -> Self {
        let mut indices = Vec::new();
        for poly in &polys {
            for i in 1..poly.len().saturating_sub(1) {
                indices.extend([poly[0], poly[i], poly[i + 1]]);
            }
        }
        let mut m = Self { vertices, indices, face_count: polys.len(), polys, ..Default::default() };
        m.compute_normals();
        m
    }

    /// The mesh's polygons: the stored ones, or its triangles.
    pub fn polygons(&self) -> Vec<Vec<u32>> {
        if !self.polys.is_empty() { return self.polys.clone(); }
        self.indices.chunks_exact(3).map(|t| t.to_vec()).collect()
    }

    /// Positions, normals and triangle indices for drawing. A mesh with
    /// polygons is drawn with hard edges where neighbouring polygons meet at
    /// more than 40 degrees and smooth shading elsewhere, so a box looks like
    /// a box and a sphere like a sphere.
    pub fn render_buffers(&self) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        use glam::Vec3;
        if self.polys.is_empty() {
            let normals = if self.normals.len() == self.vertices.len() {
                self.normals.clone()
            } else {
                self.vertices.iter().map(|_| [0.0f32, 1.0, 0.0]).collect()
            };
            return (self.vertices.clone(), normals, self.indices.clone());
        }
        let v = |i: u32| Vec3::from_array(self.vertices[i as usize]);
        let face_n: Vec<Vec3> = self.polys.iter().map(|poly| {
            let mut n = Vec3::ZERO;
            for i in 0..poly.len() { n += v(poly[i]).cross(v(poly[(i + 1) % poly.len()])); }
            n
        }).collect();
        let mut around: Vec<Vec<u32>> = vec![vec![]; self.vertices.len()];
        for (p, poly) in self.polys.iter().enumerate() {
            for i in poly { around[*i as usize].push(p as u32); }
        }
        let limit = 40f32.to_radians().cos();
        let (mut pos, mut nrm, mut idx) = (vec![], vec![], vec![]);
        for (p, poly) in self.polys.iter().enumerate() {
            let own  = face_n[p].normalize_or_zero();
            let base = pos.len() as u32;
            for i in poly {
                // Area-weighted average over the neighbours within the angle limit.
                let n: Vec3 = around[*i as usize].iter()
                    .map(|q| face_n[*q as usize])
                    .filter(|q| q.normalize_or_zero().dot(own) >= limit)
                    .sum();
                pos.push(self.vertices[*i as usize]);
                nrm.push(if n.length_squared() > 0.0 { n.normalize().to_array() } else { own.to_array() });
            }
            for i in 1..poly.len().saturating_sub(1) {
                idx.extend([base, base + i as u32, base + i as u32 + 1]);
            }
        }
        (pos, nrm, idx)
    }

    /// Compute flat (per-triangle) normals and store them as a Vertex primvar.
    /// Called by generators after building geometry.
    pub fn compute_normals(&mut self) {
        use glam::Vec3;
        let n = self.vertices.len();
        let mut normals = vec![Vec3::ZERO; n];
        let mut counts  = vec![0u32; n];

        for tri in self.indices.chunks(3) {
            if tri.len() < 3 { continue; }
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            if a >= n || b >= n || c >= n { continue; }
            let va = Vec3::from(self.vertices[a]);
            let vb = Vec3::from(self.vertices[b]);
            let vc = Vec3::from(self.vertices[c]);
            let face_n = (vb - va).cross(vc - va);
            normals[a] += face_n; counts[a] += 1;
            normals[b] += face_n; counts[b] += 1;
            normals[c] += face_n; counts[c] += 1;
        }

        self.normals = normals.iter().zip(&counts).map(|(n, &c)| {
            // A vertex used only by triangles with no area has no direction of its own.
            if c > 0 && n.length_squared() > 0.0 { n.normalize().to_array() } else { [0.0, 1.0, 0.0] }
        }).collect();

        // Also store as a primvar so the inspector can show it
        self.primvars.retain(|p| p.name != "N");
        self.primvars.push(PrimVar {
            name:   "N".into(),
            interp: PrimVarInterp::Vertex,
            values: self.normals.iter()
                .map(|n| vec![n[0], n[1], n[2]])
                .collect(),
        });
    }

    /// Total number of faces (triangles after triangulation).
    pub fn num_faces(&self) -> usize { self.face_count }

    /// Total number of face-varying elements (3 per triangle).
    pub fn num_face_varying(&self) -> usize { self.indices.len() }

    /// Every point, numbered as the Primitive Inspector lists them: polygon
    /// vertices, then curve control points, then free points.
    pub fn point_count(&self) -> usize {
        self.vertices.len() + self.curve_points.len() + self.points.len()
    }

    /// Polygon corners: one per point of each polygon, as `Geo` stores them
    /// (`Topology::Mesh::indices`), not the corners of the triangles drawn.
    pub fn corner_count(&self) -> usize {
        if self.polys.is_empty() { self.indices.len() } else { self.polys.iter().map(|p| p.len()).sum() }
    }

    /// The point of each polygon corner, polygon after polygon.
    pub fn corner_points(&self) -> Vec<u32> {
        if self.polys.is_empty() { self.indices.clone() } else { self.polys.iter().flatten().copied().collect() }
    }

    /// The UV of each polygon corner, in the order of `corner_points`, read
    /// back from `uvs` (one per triangle corner, each polygon a fan from its
    /// first corner, as `from_polys` and `Geo::to_mesh` triangulate). `None`
    /// when the mesh has no UVs or they do not follow that triangulation.
    pub fn corner_uvs(&self) -> Option<Vec<[f32; 2]>> {
        if self.uvs.is_empty() || self.uvs.len() != self.indices.len() { return None; }
        if self.polys.is_empty() { return Some(self.uvs.clone()); }
        let tris: usize = self.polys.iter().map(|p| p.len().saturating_sub(2)).sum();
        if tris * 3 != self.uvs.len() { return None; }
        let mut out = Vec::with_capacity(self.corner_count());
        let mut t = 0usize; // first triangle of the polygon
        for p in &self.polys {
            let n = p.len();
            if n < 3 { out.extend(std::iter::repeat([0.0; 2]).take(n)); continue; }
            // Corner 0 starts every triangle; corner k (1..n-1) is the second
            // corner of triangle k-1, and the last is the third of the last.
            out.push(self.uvs[t * 3]);
            for k in 1..n - 1 { out.push(self.uvs[(t + k - 1) * 3 + 1]); }
            out.push(self.uvs[(t + n - 3) * 3 + 2]);
            t += n - 2;
        }
        Some(out)
    }

    /// The segments of the curves' control hulls: each pair of control points
    /// that follow each other along a curve, and the last back to the first
    /// on a periodic curve. Indices are into `curve_points`.
    pub fn curve_segments(&self) -> Vec<[u32; 2]> {
        let closed = self.curve_wrap == CurveWrap::Periodic;
        let mut out = Vec::with_capacity(self.curve_points.len());
        let mut at = 0u32;
        for &n in &self.curve_counts {
            for k in 1..n { out.push([at + k - 1, at + k]); }
            if closed && n > 2 { out.push([at + n - 1, at]); }
            at += n;
        }
        out
    }
}

// ============================================================================
// EVAL RESULT
// ============================================================================

#[derive(Clone, Debug)]
pub struct Prim {
    pub path:     String,
    /// Shared: a packed primitive is passed along without being copied.
    pub mesh:     std::sync::Arc<MeshData>,
    /// Chosen by a Pick Primitives node. Nodes that change a mesh then work
    /// on the picked primitives and pass the others through untouched.
    pub picked:   bool,
    /// Path of the material bound to it in the file it came from.
    pub material: Option<String>,
    /// That material, as far as the viewport can show it.
    pub look:     Option<std::sync::Arc<Look>>,
    /// The hierarchy of the stage it came from, shared with every other
    /// primitive of that stage. The scene explorer lists the stage from it.
    pub stage:    Option<std::sync::Arc<crate::usd_scene::StageTree>>,
    /// The geometry `mesh` is a view of, in its own space with every
    /// attribute. `None` once a node rebuilds the mesh outside it (merging
    /// several primitives into one, for instance).
    pub geo:      Option<std::sync::Arc<crate::core::geo::Geo>>,
    /// The primitive as it was loaded, shared by everything made from it:
    /// what its edits are measured against (see `crate::edits`). `None` for
    /// primitives that did not come from a composed stage.
    pub source:   Option<std::sync::Arc<Source>>,
    /// Where `mesh` is drawn: as it is, or as copies of a shared mesh.
    pub place:    Placement,
    /// Its USD purpose, which the scene explorer's purpose toggles filter by.
    pub purpose:  Purpose,
    /// Properties set on it by Set Attribute (attributes and metadata by
    /// name: `visibility`, `kind`, `doubleSided`…), written by Write USD.
    /// Empty as loaded. Shared, as the mesh is.
    pub props:    std::sync::Arc<std::collections::BTreeMap<String, crate::core::props::Value>>,
    /// Values Set Attribute set on the prims Imago made above it (the
    /// Xforms of its Primitive Location), by path. Every primitive below
    /// such a prim carries them; Write USD writes them once, on the Xform.
    pub above:    std::sync::Arc<std::collections::BTreeMap<String, crate::core::props::Opinions>>,
    /// A point instancer: its points are its instances, and these are its
    /// prototypes' paths by `protoIndex`. It is not drawn, nor merged into a
    /// mesh: its prototypes are drawn at its points. `None` for the rest.
    pub instancing: Option<std::sync::Arc<Vec<String>>>,
    /// A point instancer's prototype mesh: its placements are made from the
    /// instancer's points that use its prototype (`core::instancing::sync`),
    /// so what moves the points moves the copies. `None` for the rest.
    pub instanced_by: Option<std::sync::Arc<InstancedBy>>,
}

/// Which instancer a prototype mesh is drawn by, and what its placements
/// were last made from.
#[derive(Clone, Debug)]
pub struct InstancedBy {
    /// The instancer's path.
    pub instancer: String,
    /// The `protoIndex` of the mesh's prototype.
    pub index:     u32,
    /// The mesh's transform inside its prototype.
    pub inside:    glam::Mat4,
    /// Instances not drawn (`invisibleIds`).
    pub hidden:    std::sync::Arc<Vec<i64>>,
    /// The instancer's points and placement the placements were made from.
    pub geo:       std::sync::Arc<crate::core::geo::Geo>,
    pub place:     Placement,
}

/// A packed primitive as it was loaded from its stage.
#[derive(Clone, Debug)]
pub struct Source {
    pub geo:      std::sync::Arc<crate::core::geo::Geo>,
    pub place:    Placement,
    pub material: Option<String>,
    pub purpose:  Purpose,
}

/// A packed primitive's USD purpose: what kind of drawing it is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Purpose {
    /// No purpose: always drawn.
    #[default]
    Default,
    /// Final geometry. `has_proxy`: its asset also carries proxy geometry,
    /// drawn in its place while proxies are shown.
    Render { has_proxy: bool },
    /// A light stand-in for render geometry.
    Proxy,
    /// A helper, never rendered.
    Guide,
}

/// The purposes the viewport can show or hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PurposeKind { Render, Proxy, Guide }

/// Where a packed primitive's mesh is drawn.
///
/// Instanced primitives share one mesh in its own space and differ only in
/// where they are placed: a native USD instance is one copy, a point
/// instancer's prototype is many. Moving them moves the placements, not the
/// points; a node that changes the mesh itself works on the placed copies.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Placement {
    /// The mesh is already where it is drawn.
    #[default]
    InPlace,
    /// One copy of a shared mesh.
    One(glam::Mat4),
    /// Many copies of a shared mesh.
    Many(std::sync::Arc<[glam::Mat4]>),
}

impl Placement {
    pub fn is_placed(&self) -> bool { !matches!(self, Placement::InPlace) }

    /// How many times the mesh is drawn.
    pub fn copies(&self) -> usize {
        match self { Placement::Many(m) => m.len(), _ => 1 }
    }

    /// The placements, empty when the mesh is in place.
    pub fn matrices(&self) -> &[glam::Mat4] {
        match self { Placement::InPlace => &[], Placement::One(m) => std::slice::from_ref(m), Placement::Many(m) => &m[..] }
    }

    /// Every copy moved by `m` afterwards.
    pub fn moved(&self, m: glam::Mat4) -> Placement {
        match self {
            Placement::InPlace => Placement::InPlace,
            Placement::One(x) => Placement::One(m * *x),
            Placement::Many(xs) => Placement::Many(xs.iter().map(|x| m * *x).collect()),
        }
    }

    /// The same placement, without comparing every matrix of a large set.
    pub fn same(&self, other: &Placement) -> bool {
        match (self, other) {
            (Placement::InPlace, Placement::InPlace) => true,
            (Placement::One(a), Placement::One(b)) => a == b,
            (Placement::Many(a), Placement::Many(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// How a surface looks in the viewport.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub name:      String,
    pub color:     [f32; 3],
    pub roughness: f32,
    pub metallic:  f32,
    pub opacity:   f32,
    /// Texture files, as full paths.
    pub color_map:    Option<std::path::PathBuf>,
    pub emissive_map: Option<std::path::PathBuf>,
    /// The colour texture's alpha cuts the surface out.
    pub cutout:    bool,
}

impl Prim {
    pub fn new(path: String, mesh: MeshData) -> Self {
        Self { path, mesh: std::sync::Arc::new(mesh), picked: false, material: None, look: None, stage: None, geo: None, source: None, place: Placement::InPlace, purpose: Purpose::Default, props: Default::default(), above: Default::default(), instancing: None, instanced_by: None }
    }

    /// The mesh where it is drawn: as it is when in place, else every copy
    /// placed and merged. Borrowed when nothing has to be made.
    pub fn world(&self) -> std::borrow::Cow<'_, MeshData> {
        use crate::node_graph::nodes::{merge_all, place_mesh};
        match &self.place {
            Placement::InPlace => std::borrow::Cow::Borrowed(&*self.mesh),
            Placement::One(m) => std::borrow::Cow::Owned(place_mesh(&self.mesh, m)),
            Placement::Many(ms) => {
                let copies: Vec<MeshData> = ms.iter().map(|m| place_mesh(&self.mesh, m)).collect();
                let parts: Vec<&MeshData> = copies.iter().collect();
                std::borrow::Cow::Owned(merge_all(&parts))
            }
        }
    }
}

/// One picked primitive, placed once or in place (or a lone primitive
/// with no pick), through a mesh operation: the operation sees it where it
/// is drawn (as the modelling tools do), and the
/// result goes back into its own space and geometry. Points the operation
/// left alone keep their exact values, so an operation on UVs alone leaves
/// the points shared with the source.
fn edit_in_place(p: &Prim, op: impl FnOnce(MeshData) -> MeshData) -> Prim {
    use crate::node_graph::nodes::place_mesh;
    let at = match p.place { Placement::One(at) => Some(at), Placement::InPlace => None, Placement::Many(_) => return p.clone() };
    let world = p.world().into_owned();
    let edited = op(world.clone());
    let mut local = match at { Some(at) => place_mesh(&edited, &at.inverse()), None => edited.clone() };
    if local.vertices.len() == p.mesh.vertices.len() {
        for (i, v) in local.vertices.iter_mut().enumerate() {
            if edited.vertices[i] == world.vertices[i] { *v = p.mesh.vertices[i]; }
        }
    }
    match &p.geo {
        Some(geo) => {
            let geo = std::sync::Arc::new(geo.with_mesh_edits(&p.mesh, &local));
            Prim { mesh: std::sync::Arc::new(geo.to_mesh()), geo: Some(geo), ..p.clone() }
        }
        None => Prim { mesh: std::sync::Arc::new(local), ..p.clone() },
    }
}

/// The world meshes of some packed primitives, merged into one.
fn merge_world(prims: &[&Prim]) -> MeshData {
    // A point instancer's points are where its prototypes are drawn, not geometry.
    let worlds: Vec<std::borrow::Cow<'_, MeshData>> = prims.iter().filter(|p| p.instancing.is_none()).map(|p| p.world()).collect();
    let parts: Vec<&MeshData> = worlds.iter().map(|w| &**w).collect();
    crate::node_graph::nodes::merge_all(&parts)
}

#[derive(Clone, Debug)]
pub enum EvalResult {
    Single(MeshData),
    Named(Vec<Prim>),
    /// Skeleton + clip. Shared, so passing it along the graph is cheap.
    Anim(std::sync::Arc<crate::core::anim::AnimData>),
}

impl EvalResult {
    pub fn into_mesh(self) -> MeshData {
        match self {
            EvalResult::Single(m) => m,
            EvalResult::Named(prims) => merge_world(&prims.iter().collect::<Vec<_>>()),
            EvalResult::Anim(_) => MeshData::default(),
        }
    }

    /// Each primitive where it is drawn, kept apart. Unlike `into_mesh`, curves
    /// of different kinds keep their control points instead of becoming
    /// polylines, so this is what the Primitive Inspector lists.
    pub fn into_parts(self) -> Vec<MeshData> {
        match self {
            EvalResult::Single(m) => vec![m],
            EvalResult::Named(prims) => prims.iter().map(|p| p.world().into_owned()).collect(),
            EvalResult::Anim(_) => vec![],
        }
    }

    pub fn as_anim(&self) -> Option<&std::sync::Arc<crate::core::anim::AnimData>> {
        if let EvalResult::Anim(a) = self { Some(a) } else { None }
    }

    pub fn as_mesh(&self) -> MeshData {
        self.clone().into_mesh()
    }

    /// Everything as one mesh, shared. A single packed primitive is handed
    /// over as it is. Several are merged once and the merge is kept while
    /// the same primitives keep arriving, so a heavy model is not copied
    /// each time the graph is evaluated.
    pub fn shared_mesh(&self) -> std::sync::Arc<MeshData> {
        use std::sync::{Arc, Mutex};
        type Kept = (Vec<(Arc<MeshData>, Placement)>, Arc<MeshData>);
        static MERGED: Mutex<Vec<Kept>> = Mutex::new(Vec::new());
        match self {
            EvalResult::Named(prims) if prims.len() == 1 && !prims[0].place.is_placed() => prims[0].mesh.clone(),
            EvalResult::Named(prims) => {
                let mut kept = MERGED.lock().unwrap();
                let same = |parts: &Vec<(Arc<MeshData>, Placement)>| parts.len() == prims.len()
                    && parts.iter().zip(prims).all(|((m, pl), b)| Arc::ptr_eq(m, &b.mesh) && pl.same(&b.place));
                if let Some((_, m)) = kept.iter().find(|(parts, _)| same(parts)) {
                    return m.clone();
                }
                let merged = Arc::new(merge_world(&prims.iter().collect::<Vec<_>>()));
                if kept.len() >= 2 { kept.remove(0); }
                kept.push((prims.iter().map(|p| (p.mesh.clone(), p.place.clone())).collect(), merged.clone()));
                merged
            }
            other => Arc::new(other.as_mesh()),
        }
    }

    /// True when this is packed primitives with at least one picked.
    pub fn has_picked(&self) -> bool {
        matches!(self, EvalResult::Named(prims) if prims.iter().any(|p| p.picked))
    }

    /// The mesh a modifying node works on: the picked primitives as one
    /// mesh when some are picked, everything as one mesh otherwise.
    pub fn work_mesh(&self) -> MeshData {
        match self {
            EvalResult::Named(prims) if prims.iter().any(|p| p.picked) => {
                merge_world(&prims.iter().filter(|p| p.picked).collect::<Vec<_>>())
            }
            other => other.as_mesh(),
        }
    }

    /// Run a mesh operation. On packed primitives with some picked, only
    /// those go through it, and the rest pass through as they are.
    ///
    /// One picked primitive placed once (every prim of a composed stage) is
    /// edited where it is drawn and stays itself: the result goes back into
    /// its own space under the same placement, and into its geometry, where
    /// only the columns the operation changed are replaced. Several picked
    /// come out merged as one primitive, still picked, in the place of the
    /// first of them. With none picked, a lone primitive is edited where it
    /// is drawn and stays itself; several are merged and go through as one
    /// mesh.
    pub fn map_mesh(&self, op: impl FnOnce(MeshData) -> MeshData) -> EvalResult {
        match self {
            EvalResult::Named(prims) if prims.iter().filter(|p| p.picked).count() == 1
                && prims.iter().any(|p| p.picked && matches!(p.place, Placement::One(_))) =>
            {
                let mut op = Some(op);
                EvalResult::Named(prims.iter().map(|p| {
                    if p.picked {
                        if let Some(f) = op.take() { return edit_in_place(p, f); }
                    }
                    p.clone()
                }).collect())
            }
            EvalResult::Named(prims) if prims.iter().any(|p| p.picked) => {
                let result = op(self.work_mesh());
                let mut out: Vec<Prim> = Vec::with_capacity(prims.len());
                let mut result = Some(result);
                for p in prims {
                    if !p.picked { out.push(p.clone()); continue; }
                    if let Some(mesh) = result.take() {
                        out.push(Prim { path: p.path.clone(), mesh: std::sync::Arc::new(mesh), picked: true, material: p.material.clone(), look: p.look.clone(), stage: p.stage.clone(), geo: None, source: p.source.clone(), place: Placement::InPlace, purpose: p.purpose, props: p.props.clone(), above: p.above.clone(), instancing: p.instancing.clone(), instanced_by: p.instanced_by.clone() });
                    }
                }
                EvalResult::Named(out)
            }
            // One primitive, none picked (a shape, a file with one mesh):
            // it is edited where it is drawn and stays itself, path and all.
            EvalResult::Named(prims) if prims.len() == 1 && !prims[0].picked && !matches!(prims[0].place, Placement::Many(_)) =>
                EvalResult::Named(vec![edit_in_place(&prims[0], op)]),
            other => EvalResult::Single(op(other.as_mesh())),
        }
    }

    /// Paths of the packed primitives, in order.
    pub fn prim_paths(&self) -> Vec<String> {
        match self { EvalResult::Named(prims) => prims.iter().map(|p| p.path.clone()).collect(), _ => vec![] }
    }
}

// ============================================================================
// PRIMITIVE INSPECTOR STATE
// ============================================================================

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PrimInspectorTab {
    #[default]
    Vertex,
    Edge,
    /// One row per polygon. Per-polygon primvars sit here too.
    Uniform,
    FaceVarying,
    Constant,
    /// The selected prims themselves: their properties, by name, with where
    /// each comes from.
    Properties,
    /// Joints of a clip, at the playhead.
    Joint,
    /// Bones of a clip: each runs from one joint to another.
    Bone,
}

/// One tab of the inspector, ready to draw.
#[derive(Clone, Debug, Default)]
pub struct InspectorTable {
    pub rows:   usize,
    /// Text column before the numbers (joint names), with its heading.
    pub labels: Option<(String, Vec<String>)>,
    /// Column group name and its values per row. A group wider than one
    /// is shown as name.X, name.Y, name.Z, unless it is an array.
    pub cols:   Vec<(String, Vec<Vec<f32>>)>,
    /// Columns whose values are arrays of whole numbers of any length (the
    /// points of a face or an edge), each shown in one cell as `[0, 3, 2, 1]`,
    /// with the length in characters of their widest cell.
    pub arrays: Vec<(String, usize)>,
}

/// An array of whole numbers as one cell shows it: `[0, 3, 2, 1]`.
pub fn array_text(values: &[f32]) -> String {
    let items: Vec<String> = values.iter().map(|v| format!("{}", *v as i64)).collect();
    format!("[{}]", items.join(", "))
}

#[derive(Default)]
pub struct PrimInspectorState {
    pub active_tab:   PrimInspectorTab,
    pub row_offset:   usize,

    // Primitives or clip of the scene at the view flag (the selected row's
    // part of it), cooked once per graph revision and selection.
    cached_parts:   Option<std::sync::Arc<Vec<MeshData>>>,
    cached_clip:    Option<std::sync::Arc<crate::core::anim::AnimData>>,
    cached_node_id: Option<NodeId>,
    cached_paths:   Vec<String>,
    cached_rev:     Option<u64>,
    /// The Properties tab's rows.
    cached_props:   std::sync::Arc<Vec<crate::prim_inspector::PropRow>>,
    /// Number of edges of the cached primitives: polygon edges and curve segments.
    pub edge_count: usize,
    // The open tab, built once per cached data (and per frame for a clip).
    table_key: Option<(PrimInspectorTab, usize)>,
    table:     InspectorTable,
}

impl PrimInspectorState {
    /// Whether what is held is for this operator, these selected paths and
    /// this revision of the graph.
    pub fn is_cache_valid(&self, node_id: Option<NodeId>, paths: &[String], revision: u64) -> bool {
        self.cached_node_id == node_id && self.cached_paths == paths && self.cached_rev == Some(revision)
    }

    pub fn update_cache(
        &mut self,
        node_id:  Option<NodeId>,
        paths:    &[String],
        parts:    Vec<MeshData>,
        clip:     Option<std::sync::Arc<crate::core::anim::AnimData>>,
        props:    Vec<crate::prim_inspector::PropRow>,
        revision: u64,
    ) {
        self.cached_props   = std::sync::Arc::new(props);
        self.edge_count = parts.iter()
            .map(|m| crate::core::poly::PolyMesh::from_mesh(m).edges().len() + m.curve_segments().len())
            .sum();
        self.cached_parts   = Some(std::sync::Arc::new(parts)).filter(|p| !p.is_empty());
        self.cached_clip    = clip;
        self.cached_node_id = node_id;
        self.cached_paths   = paths.to_vec();
        self.cached_rev     = Some(revision);
        self.table_key      = None;
        self.table          = InspectorTable::default();
    }

    /// The primitives shown, each where it is drawn.
    pub fn cached_parts(&self) -> Option<std::sync::Arc<Vec<MeshData>>> { self.cached_parts.clone() }
    pub fn cached_clip(&self) -> Option<std::sync::Arc<crate::core::anim::AnimData>> { self.cached_clip.clone() }
    /// The Properties tab's rows.
    pub fn cached_props(&self) -> std::sync::Arc<Vec<crate::prim_inspector::PropRow>> { self.cached_props.clone() }

    /// True when the stored table is this tab at this frame.
    pub fn table_is_for(&self, tab: &PrimInspectorTab, frame: usize) -> bool {
        self.table_key.as_ref() == Some(&(tab.clone(), frame))
    }

    pub fn set_table(&mut self, tab: PrimInspectorTab, frame: usize, table: InspectorTable) {
        self.table_key = Some((tab, frame));
        self.table     = table;
    }

    /// Lend the table out for drawing; hand it back with `put_table`.
    pub fn take_table(&mut self) -> InspectorTable { std::mem::take(&mut self.table) }
    pub fn put_table(&mut self, table: InspectorTable) { self.table = table; }

    /// Clear cache when tab changes
    pub fn set_active_tab(&mut self, tab: PrimInspectorTab) {
        if self.active_tab != tab {
            self.active_tab = tab;
            self.row_offset = 0;
        }
    }
}

// ============================================================================
// SCENE HIERARCHY
// ============================================================================

/// Pick a display icon based on the prim name and whether it has children.
/// Matches common USD naming conventions (Geom, pCylinder, pSphere, etc.).
fn prim_icon(name: &str, has_children: bool) -> &'static str {
    let lower = name.to_lowercase();
    if has_children {
        if lower.contains("geom")                            { return "🔷"; }
        if lower.contains("xform")                            { return "⟲"; }
        if lower.contains("look") || lower.contains("mat")  { return "🎨"; }
        return "📁";  // generic xform / group
    }
    // Leaf mesh — guess from name
    if lower.contains("cylinder") || lower.contains("tube") { return "🔩"; }
    if lower.contains("sphere")   || lower.contains("ball") { return "🔵"; }
    if lower.contains("cube")     || lower.contains("box")  { return "⬛"; }
    if lower.contains("plane")    || lower.contains("grid") { return "⬜"; }
    if lower.contains("cone")                               { return "🔺"; }
    "🔹"  // generic mesh leaf
}

/// Icon for a prim of a composed stage, by its type.
fn stage_icon(n: &crate::usd_scene::StageNode, has_children: bool) -> &'static str {
    match n.type_name.as_str() {
        "Mesh" | "BasisCurves" | "NurbsCurves" | "HermiteCurves" | "Points" => prim_icon(&n.name, false),
        "Material" => "🎨",
        "Camera" => "🎥",
        "Skeleton" | "SkelRoot" => "🦴",
        t if t.ends_with("Light") => "💡",
        _ => prim_icon(&n.name, has_children),
    }
}

#[derive(Clone, Debug)]
pub struct SceneObject {
    pub id:           SceneObjectId,
    pub name:         String,
    pub icon:         &'static str,
    pub depth:        usize,
    pub has_children: bool,
    pub expanded:     bool,
    /// What the row's expansion is remembered by: unique, so two rows of the
    /// same name do not open together. A prim of a composed stage uses its
    /// path, shared with the viewport, which draws by it.
    pub key:          String,
    /// The prim path of a row of a composed stage, whatever its type. These
    /// rows decide what the viewport draws as geometry or as a box.
    pub stage_path:   Option<String>,
    /// The path of the prim the row stands for, a group or a mesh, of a
    /// stage or made in Imago. Selecting the row shows what is at or below
    /// it in the Primitive Inspector. None for joints and a plain mesh.
    pub path:         Option<String>,
    /// Its USD type (`Xform`, `Mesh`, `PointInstancer`…), shown in the
    /// Scene Explorer's type column. Empty for rows that are not prims.
    pub type_name:    String,
}

#[derive(Default)]
pub struct SceneHierarchy {
    pub objects:            Vec<SceneObject>,
    /// The paths of the selected rows, prims or groups, in the order they
    /// were selected. The Primitive Inspector shows what is at or below
    /// them; nothing selected, the whole scene. Dragged out, they all go.
    selected_paths:         Vec<String>,
    /// The row a Shift+click selects from.
    anchor:                 Option<String>,
    next_id:                usize,
    /// Rows opened or closed, by key. A row not in here takes its default:
    /// prims of a composed stage start closed, everything else open.
    expanded:               std::collections::HashMap<String, bool>,
    /// Prims of a composed stage drawn as geometry, with everything below
    /// them, however far the explorer is opened.
    geometry:               std::collections::HashSet<String>,
    /// Every composed stage drawn as geometry: no boxes at all.
    pub show_all_geometry:  bool,
    /// Purposes drawn, stored so that the default is guide off, proxy on
    /// and render on.
    show_guide:             bool,
    hide_proxy:             bool,
    hide_render:            bool,
    /// Counts changes to what the viewport should draw (expansion, geometry
    /// toggles). The viewport redraws when it moves.
    pub display_revision:   u64,
}

impl SceneHierarchy {
    fn next_id(&mut self) -> SceneObjectId {
        let id = SceneObjectId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Whether a prim of a composed stage is opened in the explorer.
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.get(path).copied().unwrap_or(false)
    }

    /// Whether a prim of a composed stage is drawn as geometry.
    pub fn shows_geometry(&self, path: &str) -> bool {
        self.geometry.contains(path)
    }

    /// Open or close a prim of a composed stage by its path.
    pub fn set_expanded(&mut self, path: &str, open: bool) {
        self.expanded.insert(path.to_string(), open);
        for o in self.objects.iter_mut().filter(|o| o.key == path) { o.expanded = open; }
        self.display_revision += 1;
    }

    /// Open or close a row.
    pub fn toggle_expanded(&mut self, id: SceneObjectId) {
        let Some(obj) = self.objects.iter_mut().find(|o| o.id == id) else { return };
        obj.expanded = !obj.expanded;
        self.expanded.insert(obj.key.clone(), obj.expanded);
        if obj.stage_path.is_some() { self.display_revision += 1; }
    }

    /// Draw a prim of a composed stage as geometry, or as a box again.
    pub fn toggle_geometry(&mut self, path: &str) {
        if !self.geometry.remove(path) { self.geometry.insert(path.to_string()); }
        self.display_revision += 1;
    }

    pub fn purpose_shown(&self, kind: PurposeKind) -> bool {
        match kind {
            PurposeKind::Guide  => self.show_guide,
            PurposeKind::Proxy  => !self.hide_proxy,
            PurposeKind::Render => !self.hide_render,
        }
    }

    pub fn set_purpose_shown(&mut self, kind: PurposeKind, on: bool) {
        if self.purpose_shown(kind) == on { return; }
        match kind {
            PurposeKind::Guide  => self.show_guide = on,
            PurposeKind::Proxy  => self.hide_proxy = !on,
            PurposeKind::Render => self.hide_render = !on,
        }
        self.display_revision += 1;
    }

    /// Whether the viewport draws a primitive of this purpose. Without a
    /// purpose it always is. Render geometry whose asset has a proxy gives
    /// way to the proxy while proxies are shown.
    pub fn draws_purpose(&self, purpose: Purpose) -> bool {
        match purpose {
            Purpose::Default => true,
            Purpose::Guide => self.show_guide,
            Purpose::Proxy => !self.hide_proxy,
            Purpose::Render { has_proxy } => !self.hide_render && !(has_proxy && !self.hide_proxy),
        }
    }

    pub fn set_show_all_geometry(&mut self, on: bool) {
        if self.show_all_geometry != on {
            self.show_all_geometry = on;
            self.display_revision += 1;
        }
    }

    /// One composed stage, as its hierarchy. A mesh is listed while its
    /// primitive is still coming in (Prune Primitives removes it); a group is
    /// listed while something below it is, or when it never held a mesh (a
    /// scope of materials, a camera rig). `listed` collects the mesh paths
    /// shown here.
    fn push_stage(
        &mut self,
        tree:       &crate::usd_scene::StageTree,
        present:    &std::collections::HashSet<&str>,
        listed:     &mut std::collections::HashSet<String>,
    ) {
        let nodes = &tree.nodes;
        // Prims that become packed geometry: meshes, curves, points.
        let is_mesh = |i: usize| matches!(nodes[i].type_name.as_str(), "Mesh" | "BasisCurves" | "NurbsCurves" | "HermiteCurves" | "Points");
        // Which prims hold meshes below them, and which hold ones still present.
        let (mut holds, mut holds_present) = (vec![false; nodes.len()], vec![false; nodes.len()]);
        let mut parents: Vec<usize> = vec![];
        for i in 0..nodes.len() {
            while parents.last().is_some_and(|&p| nodes[p].depth >= nodes[i].depth) { parents.pop(); }
            if is_mesh(i) {
                let here = present.contains(nodes[i].path.as_str());
                for &p in &parents { holds[p] = true; holds_present[p] |= here; }
            }
            parents.push(i);
        }
        let shown: Vec<bool> = (0..nodes.len()).map(|i| {
            if is_mesh(i) { present.contains(nodes[i].path.as_str()) } else { holds_present[i] || !holds[i] }
        }).collect();
        let rows: Vec<usize> = (0..nodes.len()).filter(|&i| shown[i]).collect();
        for (k, &i) in rows.iter().enumerate() {
            let n = &nodes[i];
            let has_children = rows.get(k + 1).is_some_and(|&j| nodes[j].depth > n.depth);
            // Listed here, so not listed again by its path below: geometry,
            // and a point instancer (a primitive of its own).
            listed.insert(n.path.clone());
            let id = self.next_id();
            let expanded = self.is_expanded(&n.path);
            self.objects.push(SceneObject {
                id,
                name:         n.name.clone(),
                icon:         stage_icon(n, has_children),
                depth:        n.depth,
                has_children,
                expanded,
                key:          n.path.clone(),
                stage_path:   Some(n.path.clone()),
                path:         Some(n.path.clone()),
                type_name:    n.type_name.clone(),
            });
        }
    }

    /// Rebuild from the scene at the view flag (or what reaches Output): the
    /// operator it is read from, its name and what it puts out. The rows are
    /// the scene's own tree, as USD has it: no row for the operator. Only a
    /// plain mesh, which has no path, is listed as one row by the operator's
    /// name. A selected row stays selected while its path is in the scene;
    /// once it is gone nothing is selected, and the Primitive Inspector shows
    /// the whole scene again.
    pub fn rebuild_scene(&mut self, scene: Option<(NodeId, String, EvalResult)>) {
        let plain = matches!(scene, Some((_, _, EvalResult::Single(_))));
        self.rebuild(scene.into_iter().collect());
        if !plain && self.objects.first().is_some_and(|o| o.key.starts_with("node:")) {
            self.objects.remove(0);
            for o in self.objects.iter_mut() { o.depth = o.depth.saturating_sub(1); }
        }
        // A selected path stays selected while it is in the scene.
        let present: std::collections::HashSet<&str> = self.objects.iter().filter_map(|o| o.path.as_deref()).collect();
        let kept: Vec<String> = self.selected_paths.iter().filter(|p| present.contains(p.as_str())).cloned().collect();
        if self.anchor.as_deref().is_some_and(|a| !present.contains(a)) { self.anchor = None; }
        self.selected_paths = kept;
    }

    /// The selected rows' paths: the parts of the scene the Primitive
    /// Inspector shows (all of it when none), and what "+ Add Location" adds.
    pub fn selected_paths(&self) -> &[String] { &self.selected_paths }

    /// Whether a row is selected.
    pub fn is_selected(&self, path: Option<&str>) -> bool {
        path.is_some_and(|p| self.selected_paths.iter().any(|s| s == p))
    }

    /// A click on a row with a path. Alone: that row only (clicked again,
    /// nothing). With Ctrl (Cmd on macOS): the row added or taken out. With
    /// Shift: every row with a path from the last one clicked to this one,
    /// as listed. `listed`: the paths of the rows in view, in order.
    pub fn click(&mut self, path: &str, toggle: bool, range: bool, listed: &[&str]) {
        if range {
            if let Some(a) = self.anchor.clone() {
                let (i, j) = (listed.iter().position(|p| *p == a), listed.iter().position(|p| *p == path));
                if let (Some(i), Some(j)) = (i, j) {
                    let (lo, hi) = (i.min(j), i.max(j));
                    for p in &listed[lo..=hi] {
                        if !self.is_selected(Some(p)) { self.selected_paths.push(p.to_string()); }
                    }
                    return;
                }
            }
        }
        if toggle {
            if self.is_selected(Some(path)) { self.selected_paths.retain(|p| p != path); }
            else { self.selected_paths.push(path.to_string()); }
        } else if self.selected_paths.len() == 1 && self.selected_paths[0] == path {
            self.selected_paths.clear();
        } else {
            self.selected_paths = vec![path.to_string()];
        }
        self.anchor = Some(path.to_string());
    }

    /// Select nothing: the Primitive Inspector shows the whole scene.
    pub fn clear_selection(&mut self) {
        self.selected_paths.clear();
        self.anchor = None;
    }

    /// Rebuild from the evaluated node graph results.
    /// Expansion is kept by each row's key, so it survives graph changes.
    /// Children are ALWAYS emitted — the UI handles hiding them.
    pub fn rebuild(&mut self, entries: Vec<(NodeId, String, EvalResult)>) {
        self.objects.clear();

        for (node_id, node_name, result) in entries {
            let node_key = format!("node:{}", node_id.0);
            let node_open = *self.expanded.get(&node_key).unwrap_or(&true);
            match result {
                EvalResult::Single(_) => {
                    let id = self.next_id();
                    self.objects.push(SceneObject {
                        id,
                        name:         node_name.clone(),
                        icon:         "🔹",
                        depth:        0,
                        has_children: false,
                        expanded:     true,
                        key:          node_key,
                        stage_path:   None,
                        path:         None,
                        type_name:    String::new(),
                    });
                }

                EvalResult::Anim(anim) => {
                    // Skeleton: one row per joint, indented by hierarchy depth.
                    let id = self.next_id();
                    self.objects.push(SceneObject {
                        id,
                        name:         node_name.clone(),
                        icon:         "🎬",
                        depth:        0,
                        has_children: !anim.joints.is_empty(),
                        expanded:     node_open,
                        key:          node_key,
                        stage_path:   None,
                        path:         None,
                        type_name:    String::new(),
                    });
                    let depths = anim.joint_depths();
                    let mut is_parent = vec![false; anim.joints.len()];
                    for j in &anim.joints {
                        if let Some(p) = j.parent { is_parent[p] = true; }
                    }
                    // Depth-first order so children sit under their parent.
                    let mut kids: Vec<Vec<usize>> = vec![vec![]; anim.joints.len()];
                    let mut todo: Vec<usize> = vec![];
                    for (i, j) in anim.joints.iter().enumerate().rev() {
                        match j.parent { Some(p) => kids[p].push(i), None => todo.push(i) }
                    }
                    while let Some(i) = todo.pop() {
                        let name = anim.joints[i].name.clone();
                        let key  = format!("joint:{}:{name}", node_id.0);
                        let exp  = *self.expanded.get(&key).unwrap_or(&true);
                        let id   = self.next_id();
                        self.objects.push(SceneObject {
                            id,
                            name,
                            icon:         if is_parent[i] { "●" } else { "○" },
                            depth:        depths[i] + 1,
                            has_children: is_parent[i],
                            expanded:     exp,
                            key,
                            stage_path:   None,
                            path:         None,
                            type_name:    String::new(),
                        });
                        todo.extend(kids[i].iter().copied());
                    }
                }

                EvalResult::Named(prims) => {
                    let group_id = self.next_id();
                    self.objects.push(SceneObject {
                        id:           group_id,
                        name:         node_name.clone(),
                        icon:         "📂",
                        depth:        0,
                        has_children: !prims.is_empty(),
                        expanded:     node_open,
                        key:          node_key,
                        stage_path:   None,
                        path:         None,
                        type_name:    String::new(),
                    });

                    // Primitives from a composed stage are listed in the stage's
                    // own hierarchy, with its groups, materials, cameras and
                    // lights. Each stage once, however many primitives share it.
                    let present: std::collections::HashSet<&str> = prims.iter().map(|p| p.path.as_str()).collect();
                    let mut stages: Vec<&std::sync::Arc<crate::usd_scene::StageTree>> = vec![];
                    for p in &prims {
                        if let Some(tree) = &p.stage {
                            if !tree.nodes.is_empty() && !stages.iter().any(|s| std::sync::Arc::ptr_eq(s, tree)) { stages.push(tree); }
                        }
                    }
                    let mut listed: std::collections::HashSet<String> = std::collections::HashSet::new();
                    for tree in stages {
                        self.push_stage(tree, &present, &mut listed);
                    }

                    // Everything else by the segments of its path.
                    let mut seen_parents: std::collections::HashSet<String> =
                        std::collections::HashSet::new();

                    for prim in prims.iter().filter(|p| !listed.contains(&p.path)) {
                        let segs: Vec<&str> = prim.path
                            .trim_start_matches('/')
                            .split('/')
                            .collect();

                        // Every parent on the path, as an Xform row:
                        // `/root/asset/cube1` lists root, asset,
                        // then cube1 under them.
                        for seg_depth in 0..segs.len().saturating_sub(1) {
                            let parent_key = segs[..=seg_depth].join("/");
                            if seen_parents.insert(parent_key.clone()) {
                                let key = format!("seg:{parent_key}");
                                let exp = *self.expanded.get(&key).unwrap_or(&true);
                                let id = self.next_id();
                                self.objects.push(SceneObject {
                                    id,
                                    name:         segs[seg_depth].to_string(),
                                    icon:         prim_icon(segs[seg_depth], true),
                                    depth:        seg_depth + 1,
                                    has_children: true,
                                    expanded:     exp,
                                    key,
                                    stage_path:   None,
                                    path:         Some(format!("/{parent_key}")),
                                    type_name:    "Xform".into(),
                                });
                            }
                        }

                        // Leaf mesh entry — store the full prim path
                        let leaf_depth = segs.len().max(1);
                        let leaf_name  = segs.last().unwrap_or(&"mesh").to_string();
                        let id = self.next_id();
                        self.objects.push(SceneObject {
                            id,
                            name:         leaf_name.clone(),
                            icon:         prim_icon(&leaf_name, false),
                            depth:        leaf_depth,
                            has_children: false,
                            expanded:     true,
                            key:          format!("seg:{}", prim.path),
                            stage_path:   None,
                            path:         Some(prim.path.clone()),
                            type_name:    match prim.geo.as_ref().map(|g| &g.topology) {
                                _ if prim.instancing.is_some() => "PointInstancer".into(),
                                Some(crate::core::geo::Topology::Curves { .. }) => "BasisCurves".into(),
                                Some(crate::core::geo::Topology::Points) => "Points".into(),
                                _ => "Mesh".into(),
                            },
                        });
                    }
                }
            }
        }
    }
}
fn yes() -> bool { true }

fn one_tile() -> u32 { 1 }

#[cfg(test)]
mod purpose_tests {
    use super::*;

    #[test]
    fn the_purpose_toggles_decide_what_is_drawn() {
        let mut h = SceneHierarchy::default();
        let (render, paired, proxy, guide) = (Purpose::Render { has_proxy: false }, Purpose::Render { has_proxy: true }, Purpose::Proxy, Purpose::Guide);
        // Defaults: guide off, proxy on, render on, a proxied render mesh gives way.
        assert!(h.draws_purpose(Purpose::Default) && h.draws_purpose(render) && h.draws_purpose(proxy));
        assert!(!h.draws_purpose(paired) && !h.draws_purpose(guide));
        // Proxy off: the render mesh comes back.
        h.set_purpose_shown(PurposeKind::Proxy, false);
        assert!(h.draws_purpose(paired) && !h.draws_purpose(proxy));
        // Render off: no render geometry at all; unpurposed still drawn.
        h.set_purpose_shown(PurposeKind::Render, false);
        assert!(!h.draws_purpose(render) && !h.draws_purpose(paired) && h.draws_purpose(Purpose::Default));
        h.set_purpose_shown(PurposeKind::Guide, true);
        assert!(h.draws_purpose(guide));
    }
}

#[cfg(test)]
mod location_tests {
    use super::*;

    #[test]
    fn a_primitive_location_becomes_a_usd_path() {
        assert_eq!(prim_location("/cube", "/x"), "/cube");
        assert_eq!(prim_location("root/asset/cube1/", "/x"), "/root/asset/cube1");
        assert_eq!(prim_location(" /set//props/ my box ", "/x"), "/set/props/my_box");
        assert_eq!(prim_location("/2nd/box-a", "/x"), "/_2nd/box_a");
        assert_eq!(prim_location("", "/cube"), "/cube");
        assert_eq!(prim_location("///", "/cube"), "/cube");
    }

    #[test]
    fn the_scene_explorer_lists_every_parent_on_the_path() {
        let mut h = SceneHierarchy::default();
        let cube = Prim::new("/root/asset/cube1".into(), MeshData::default());
        h.rebuild(vec![(NodeId(1), "Cube".into(), EvalResult::Named(vec![cube]))]);
        let rows: Vec<(&str, usize)> = h.objects.iter().map(|o| (o.name.as_str(), o.depth)).collect();
        assert_eq!(rows, vec![("Cube", 0), ("root", 1), ("asset", 2), ("cube1", 3)]);
        assert_eq!(h.objects[3].path.as_deref(), Some("/root/asset/cube1"));
    }

    #[test]
    fn the_scene_explorer_lists_the_scene_not_the_operator() {
        let mut h = SceneHierarchy::default();
        let prims = |paths: &[&str]| EvalResult::Named(paths.iter().map(|p| Prim::new(p.to_string(), MeshData::default())).collect());
        h.rebuild_scene(Some((NodeId(4), "Merge".into(), prims(&["/root/asset/cube1", "/sphere"]))));
        let rows: Vec<(&str, usize)> = h.objects.iter().map(|o| (o.name.as_str(), o.depth)).collect();
        assert_eq!(rows, vec![("root", 0), ("asset", 1), ("cube1", 2), ("sphere", 0)]);
        assert_eq!(h.objects[1].path.as_deref(), Some("/root/asset"), "a group row has its path too");

        // A selection stays while its path is in the scene, and goes with it.
        h.click("/sphere", false, false, &[]);
        h.click("/root/asset/cube1", true, false, &[]);
        h.rebuild_scene(Some((NodeId(4), "Merge".into(), prims(&["/root/asset/cube1", "/sphere"]))));
        assert_eq!(h.selected_paths(), &["/sphere".to_string(), "/root/asset/cube1".to_string()]);
        h.rebuild_scene(Some((NodeId(4), "Merge".into(), prims(&["/root/asset/cube1"]))));
        assert_eq!(h.selected_paths(), &["/root/asset/cube1".to_string()]);

        // A plain mesh has no tree: one row, by the operator's name.
        h.rebuild_scene(Some((NodeId(5), "Unpack".into(), EvalResult::Single(MeshData::default()))));
        assert_eq!(h.objects.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), vec!["Unpack"]);
        // No scene at all: no rows.
        h.rebuild_scene(None);
        assert!(h.objects.is_empty());
    }

    #[test]
    fn rows_are_selected_one_toggled_or_a_range() {
        let mut h = SceneHierarchy::default();
        let listed = ["/a", "/b", "/c", "/d"];
        h.click("/b", false, false, &listed);
        assert_eq!(h.selected_paths(), &["/b".to_string()]);
        h.click("/d", false, true, &listed);
        assert_eq!(h.selected_paths().len(), 3, "Shift: /b to /d");
        h.click("/c", true, false, &listed);
        assert_eq!(h.selected_paths(), &["/b".to_string(), "/d".to_string()], "Ctrl takes one out");
        h.click("/a", false, false, &listed);
        assert_eq!(h.selected_paths(), &["/a".to_string()], "a plain click: that one only");
        h.click("/a", false, false, &listed);
        assert!(h.selected_paths().is_empty(), "clicked again: nothing");
    }
}
