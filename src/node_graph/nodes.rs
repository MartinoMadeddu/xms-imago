use crate::core::anim::{self, AnimData, FrameRate};
use crate::core::geo_mesh::GeoMesh;
use crate::types::{EvalResult, IceGraphId, MergeMode, MeshData, NodeType, Placement, Prim, RetimeMode};
use glam::{EulerRot, Quat, Vec3};
use std::sync::Arc;

/// Evaluate one node given its already-resolved upstream inputs. Returns an
/// `EvalResult` — either a single merged mesh or a list of named prims.
/// `output` is the index of the output socket being asked for. Only nodes with
/// several outputs look at it.
pub fn evaluate_node_type(
    node_type: &NodeType,
    inputs: &[EvalResult],
    eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo,
    output: usize,
) -> Option<EvalResult> {
    match node_type {
        // A shape is a primitive at its Primitive Location.
        NodeType::CreateCube { size, location } => Some(shape(location, "/cube", create_cube(*size))),
        NodeType::CreateSphere {
            radius,
            segments,
            location,
        } => Some(shape(location, "/sphere", create_sphere(*radius, *segments))),
        NodeType::CreateGrid {
            rows,
            cols,
            size,
            location,
        } => Some(shape(location, "/grid", create_grid(*rows, *cols, *size))),
        // A camera: a primitive with no geometry, placed.
        NodeType::Camera {
            location,
            translate,
            rotate,
            camera,
        } => Some(EvalResult::Named(vec![Prim {
            place: Placement::One(transform_matrix(
                Vec3::from(*translate),
                Vec3::from(rotate.map(f32::to_radians)),
                Vec3::ONE,
            )),
            camera: Some(Arc::new(camera.clone())),
            ..Prim::new(
                crate::types::prim_location(location, &crate::types::camera_location()),
                MeshData::default(),
            )
        }])),

        // A light: a primitive with no geometry, placed.
        NodeType::Light {
            location,
            translate,
            rotate,
            light,
        } => Some(EvalResult::Named(vec![crate::core::light::light_prim(
            &crate::types::prim_location(location, crate::core::light::DEFAULT_LOCATION),
            light.clone(),
            Placement::One(transform_matrix(
                Vec3::from(*translate),
                Vec3::from(rotate.map(f32::to_radians)),
                Vec3::ONE,
            )),
        )])),

        // A volume: the grids of the file of the frame cooked, no geometry.
        NodeType::LoadVdb { path, location } => {
            let frame = crate::usd_scene::cook_frame().unwrap_or(1.0).round() as i64;
            let file = crate::core::image_file::path_at(path, frame);
            let volume = crate::core::volume::cached(&file).ok()?;
            Some(EvalResult::Named(vec![Prim {
                volume: Some(volume),
                place: Placement::One(glam::Mat4::IDENTITY),
                ..Prim::new(
                    crate::types::prim_location(location, crate::core::volume::DEFAULT_LOCATION),
                    MeshData::default(),
                )
            }]))
        }

        NodeType::LoadUsd {
            path,
            mode,
            location,
            prim,
            convert,
            mask,
        } => {
            if path.is_empty() {
                return None;
            }
            // `$GDIR/usd/street.usda`: its variables put in.
            let path = crate::core::vars::expand(path.trim(), &[]);
            load_stage(&path, *convert, &Default::default(), mask, *mode, location, prim).map(EvalResult::Named)
        }

        NodeType::PickPrims { pattern } => inputs.first().map(|r| match r {
            EvalResult::Named(prims) => {
                let p = crate::core::pattern::NamePattern::new(pattern);
                EvalResult::Named(
                    prims
                        .iter()
                        .map(|m| Prim {
                            picked: p.matches(&m.path),
                            ..m.clone()
                        })
                        .collect(),
                )
            }
            other => other.clone(),
        }),
        NodeType::PrunePrims { pattern, keep } => inputs.first().map(|r| match r {
            EvalResult::Named(prims) => {
                let p = crate::core::pattern::NamePattern::new(pattern);
                if p.is_empty() {
                    return r.clone();
                }
                EvalResult::Named(prims.iter().filter(|m| p.matches(&m.path) == *keep).cloned().collect())
            }
            other => other.clone(),
        }),
        NodeType::UnpackPrims => inputs.first().map(|r| EvalResult::Single(r.as_mesh())),
        // Named by its operator when cooked in a graph (`NodeGraphState::cook`).
        NodeType::Material { surface } => Some(EvalResult::Material(Arc::new(crate::core::material::Material::new(
            "Material", surface,
        )))),
        NodeType::MaterialX { graph } => Some(EvalResult::Material(Arc::new(
            crate::core::material::Material::from_graph("MaterialX", graph),
        ))),
        // In a graph, cooked by input number (`NodeGraphState::cook`). Here,
        // without the numbers: the scene first, then the materials in order.
        NodeType::MaterialBinding {
            library,
            bindings,
            options,
            ..
        } => {
            let scene = inputs.iter().find(|r| !matches!(r, EvalResult::Material(_)));
            let materials: Vec<_> = inputs
                .iter()
                .filter_map(|r| match r {
                    EvalResult::Material(m) => Some(Some(m.clone())),
                    _ => None,
                })
                .collect();
            Some(crate::core::material::bind(
                scene,
                &materials,
                library,
                &[],
                bindings,
                options,
            ))
        }
        // Render
        // The scene coming in, with its render prims added or changed.
        NodeType::RenderCommons { scene } => Some(crate::core::render::scene_settings(inputs.first(), scene)),
        NodeType::RenderSettings { renderer } => Some(crate::core::render::renderer_settings(inputs.first(), renderer)),
        NodeType::RenderVariables { vars } => Some(crate::core::render::render_variables(inputs.first(), vars)),
        // Runs when asked, from its panel: on cook, what comes in goes on.
        NodeType::RenderCommand { .. } => inputs.first().cloned(),
        // Its variables go down the wires, read from its parameters: on
        // cook, what comes in goes on.
        NodeType::SetVariable { .. } => inputs.first().cloned(),
        // Cooked by the graph they are in (`node_graph::compound`): a
        // Compound's inside, what came into a Compound Input. Here, what comes
        // in goes on.
        NodeType::Compound { .. } | NodeType::CompoundInput { .. } | NodeType::CompoundOutput { .. } => {
            inputs.first().cloned()
        }
        // Without Primitive Locations it names nothing: what comes in goes on.
        NodeType::EditPrimitive { .. } => inputs.first().cloned(),
        // Cooked by the graph, with its locations (`core::reparent`).
        NodeType::Reparent { .. } => inputs.first().cloned(),
        // Cooked by `NodeGraphState::cook` (`core::variant`).
        NodeType::Variant { .. } => inputs.first().cloned(),
        // Without Primitive Locations: on the picked primitives, or every one.
        NodeType::SetAttribute { rows } => inputs
            .first()
            .map(|r| crate::core::set_attribute::set_attributes(r, rows, &[])),

        NodeType::Transform {
            translation,
            rotation,
            scale,
            pivot,
            order,
        } => inputs.first().and_then(|r| {
            let m = transform_about(*translation, *rotation, *scale, *pivot, *order);
            match r {
                // A clip moves by its top joints; its skin follows when posed.
                EvalResult::Anim(clip) => {
                    let s = Vec3::splat(clip_scale(*scale));
                    let x = transform_about(*translation, *rotation, s, *pivot, *order);
                    anim::memo(&format!("{node_type:?}"), Some(clip), || Some(clip.moved(x))).map(EvalResult::Anim)
                }
                // Packed primitives: the picked ones move, or every one when
                // none is, each staying a primitive of its own. Placed ones
                // move their placements (the shared mesh is not copied); the
                // others their points.
                EvalResult::Named(prims) => {
                    let all = !prims.iter().any(|p| p.picked);
                    Some(EvalResult::Named(
                        prims
                            .iter()
                            .map(|p| match &p.place {
                                _ if !all && !p.picked => p.clone(),
                                Placement::InPlace => Prim {
                                    mesh: Arc::new(place_mesh(&p.mesh, &m)),
                                    geo: None,
                                    ..p.clone()
                                },
                                placed => Prim {
                                    place: placed.moved(m),
                                    ..p.clone()
                                },
                            })
                            .collect(),
                    ))
                }
                other => Some(other.map_mesh(|mesh| place_mesh(&mesh, &m))),
            }
        }),

        // Cooked by the graph (`node_graph::local`); asked on its own, what
        // comes in passes through.
        NodeType::LocalAnimation { .. } | NodeType::MergeAnimation { .. } => inputs.first().cloned(),

        NodeType::Merge { mode, .. } => {
            let first = inputs.first()?;
            if merges_primitives(*mode, first) {
                // Primitives side by side; on the same path the later input
                // is the stronger one. A plain mesh among them is a primitive.
                let mut prims = match prims_of(first) {
                    EvalResult::Named(p) => p,
                    _ => vec![],
                };
                for r in &inputs[1..] {
                    if let EvalResult::Named(more) = prims_of(r) {
                        prims = merge_prims(&prims, &more);
                    }
                }
                Some(EvalResult::Named(prims))
            } else {
                // Every input as one mesh.
                let meshes: Vec<MeshData> = inputs.iter().map(|r| r.as_mesh()).collect();
                let mut all = meshes.into_iter();
                let first = all.next()?;
                Some(EvalResult::Single(all.fold(first, |a, b| merge(&a, &b))))
            }
        }

        NodeType::Ice { id, params, .. } => {
            // What it sets on its graph's Parameter nodes, for each run.
            let params = || params.clone();
            // The first input is the scene; the others, numbered from 1, are
            // references: every prim of the scene wired there (groups too),
            // for Get Reference.
            let references: Vec<crate::ice::Reference> = inputs
                .iter()
                .skip(1)
                .map(|r| match r {
                    EvalResult::Named(prims) => crate::ice::Reference {
                        paths: crate::node_graph::scene_paths(prims),
                        meshes: reference_meshes(prims),
                    },
                    _ => Default::default(),
                })
                .collect();
            match inputs.first()? {
                // Packed primitives: the tree runs on each one (the picked
                // ones, when some are), in its own space, as ICE does on an
                // object. Each keeps its path, placement and material, and
                // whatever the tree does not write stays shared.
                EvalResult::Named(prims) => {
                    let any_picked = prims.iter().any(|p| p.picked);
                    let (mut out, mut made) = (Vec::with_capacity(prims.len()), vec![]);
                    // Paths there are: an instancer made on one of them is numbered.
                    let mut taken: std::collections::HashSet<String> = prims.iter().map(|p| p.path.clone()).collect();
                    for p in prims {
                        if any_picked && !p.picked {
                            out.push(p.clone());
                            continue;
                        }
                        let geo = p
                            .geo
                            .clone()
                            .unwrap_or_else(|| Arc::new(crate::core::geo::Geo::from_mesh(&p.mesh)));
                        crate::ice::take_instances();
                        // Reference meshes are taken into its space.
                        crate::ice::set_space(p.place.matrices().first().copied().unwrap_or(glam::Mat4::IDENTITY));
                        crate::ice::set_params(params());
                        let g = eval_subnet(*id, &geo, &references);
                        // What its Instancers made: point instancers, after the others.
                        for (i, draw) in crate::ice::take_instances() {
                            made.extend(instanced(p, &i, draw, &inputs[1..], &mut taken));
                        }
                        // The attributes stay in the geometry (see `to_mesh_lean`).
                        out.push(Prim {
                            mesh: Arc::new(g.to_mesh_lean()),
                            geo: Some(Arc::new(g)),
                            ..p.clone()
                        });
                    }
                    out.extend(made);
                    Some(EvalResult::Named(out))
                }
                other => {
                    let geo = crate::core::geo::Geo::from_mesh(&other.as_mesh());
                    crate::ice::set_params(params());
                    Some(EvalResult::Single(eval_subnet(*id, &geo, &references).to_mesh()))
                }
            }
        }

        // Socket by socket, in `NodeGraphState::eval_node_out`.
        NodeType::Placeholder { .. } => inputs.get(output).cloned(),
        NodeType::PassThrough => inputs.first().cloned(),

        // Animation
        // Clip, Geometry, Rest.
        NodeType::LoadFbx { path, take } => match output {
            1 => crate::fbx_loader::load_scene_cached(path)
                .ok()
                .filter(|s| !s.prims.is_empty())
                .map(|s| EvalResult::Named((*s.prims).clone())),
            2 => {
                let _ = take;
                let clip = crate::fbx_loader::load_fbx_cached(path, crate::fbx_loader::NO_TAKE)
                    .ok()?
                    .anim;
                anim::memo(&format!("{node_type:?}:rest"), Some(&clip), || Some(clip.bind_clip())).map(EvalResult::Anim)
            }
            _ => crate::fbx_loader::load_fbx_cached(path, *take)
                .ok()
                .map(|l| EvalResult::Anim(l.anim)),
        },

        NodeType::TestClip {
            seconds,
            fps_num,
            fps_den,
        } => anim::memo(&format!("{node_type:?}"), None, || {
            Some(anim::create_test_clip(*seconds, FrameRate::new(*fps_num, *fps_den)))
        })
        .map(EvalResult::Anim),

        NodeType::RenameJoints {
            find,
            replace,
            strip_namespace,
            prefix,
        } => anim_op(node_type, inputs, |a| {
            a.renamed(find, replace, *strip_namespace, prefix)
        }),

        NodeType::TrimClip { head, tail } => anim_op(node_type, inputs, |a| a.trimmed(*head as usize, *tail as usize)),

        NodeType::Retime { fps_num, fps_den, mode } => anim_op(node_type, inputs, |a| {
            let rate = FrameRate::new(*fps_num, *fps_den);
            match mode {
                RetimeMode::Resample => a.resampled(rate),
                RetimeMode::Reinterpret => a.reinterpreted(rate),
            }
        }),

        NodeType::SetTimecode {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame,
        } => anim_op(node_type, inputs, |a| {
            a.with_start_timecode(*hours, *minutes, *seconds, *frames, *drop_frame)
        }),

        // Batch / export
        NodeType::LoadFbxDir { dir, index, take } => {
            let files = crate::fbx_loader::list_fbx(dir);
            let path = files.get((*index as usize).min(files.len().checked_sub(1)?))?;
            crate::fbx_loader::load_fbx_cached(&path.to_string_lossy(), *take)
                .ok()
                .map(|l| EvalResult::Anim(l.anim))
        }

        NodeType::SplitSkeleton { picks } => {
            let input = inputs.first()?.as_anim()?;
            let root = split_root(input, picks.get(output)?)?;
            anim::memo(&format!("split:{root}"), Some(input), || Some(input.split(root))).map(EvalResult::Anim)
        }

        NodeType::AutoTPose {
            set_hip_height,
            hip_height,
        } => anim_op(node_type, inputs, |a| {
            a.auto_tpose(set_hip_height.then_some(*hip_height * 0.01))
        }),

        NodeType::FixPose { edits } => anim_op(node_type, inputs, |a| a.pose_fixed(edits)),

        NodeType::ProxySkin { thickness } => anim_op(node_type, inputs, |a| a.with_proxy_skin(*thickness)),

        NodeType::WriteFbx { .. } => inputs.first().cloned(),
        // Writes when asked, from the properties panel: passes its input on.
        NodeType::WriteUsd { .. } => inputs.first().cloned(),
        // The file of the frame cooked.
        // Image operators: their inputs in the rendering space, their image in
        // it too.
        t @ (NodeType::ImageMerge { .. }
        | NodeType::ColourCorrect { .. }
        | NodeType::Shuffle { .. }
        | NodeType::DeleteChannels { .. }
        | NodeType::Constant { .. }
        | NodeType::Crop { .. }
        | NodeType::Reformat { .. }
        | NodeType::Transform2D { .. }
        | NodeType::Blur { .. }
        | NodeType::Premultiply { .. }
        | NodeType::WriteImage { .. }) => image_operator(t, inputs).map(|img| EvalResult::Image(img, String::new())),
        // The file of the frame cooked, as stored, with the colour space it
        // is in: choosing another space reads nothing again.
        NodeType::LoadImage { path, colour } => {
            let frame = crate::usd_scene::cook_frame().unwrap_or(1.0).round() as i64;
            let file = crate::core::image_file::path_at(path, frame);
            let space = crate::core::image_file::colour_space(colour, &file);
            crate::core::image_file::read(&file).ok().map(|img| EvalResult::Image(img, space))
        }

        // Mocap tools
        NodeType::MirrorClip => anim_op(node_type, inputs, |a| a.mirrored()),
        NodeType::SmoothClip {
            radius,
            amount,
            translations,
        } => anim_op(node_type, inputs, |a| a.smoothed(*radius, *amount, *translations)),
        NodeType::InPlace { keep_height, to_root } => {
            anim_op(node_type, inputs, |a| a.in_place(*keep_height, *to_root))
        }
        // Read from older graphs only: `graph_io` turns it into Transform.
        NodeType::TransformClip {
            translate,
            rotate,
            scale,
        } => anim_op(node_type, inputs, |a| {
            a.transformed(Vec3::from_array(*translate), Vec3::from_array(*rotate), *scale)
        }),
        NodeType::LoopClip { blend } => anim_op(node_type, inputs, |a| a.looped(*blend)),
        NodeType::TimeWarp { speed, reverse } => anim_op(node_type, inputs, |a| a.time_warped(*speed, *reverse)),
        NodeType::PruneJoints { words } => anim_op(node_type, inputs, |a| a.pruned(words)),
        NodeType::FloorClip { height } => anim_op(node_type, inputs, |a| a.floored(*height)),
        NodeType::BlendClips { blend, align } => anim_op2(node_type, inputs, |a, b| a.blended(b, *blend, *align)),
        NodeType::Retarget => anim_op2(node_type, inputs, |a, b| a.retargeted(b).0),
        NodeType::Characterize { picks } => anim_op(node_type, inputs, |a| crate::core::human::with_picks(a, picks)),
        // With one input wired, the clip goes through as it is.
        NodeType::Reach { settings } => match inputs.len() {
            1 => inputs.first().cloned(),
            _ => {
                let mut report = None;
                let out = anim_op2(node_type, inputs, |a, b| {
                    let (c, r) = crate::core::reach::reach(a, b, settings);
                    report = Some(r);
                    c
                });
                if let (Some(EvalResult::Anim(o)), Some(r)) = (&out, report) {
                    crate::core::reach::remember_report(o, r);
                }
                out
            }
        },
        NodeType::HumanLimits { limits } => match inputs.len() {
            1 => inputs.first().cloned(),
            _ => {
                let mut report = None;
                let out = anim_op2(node_type, inputs, |moved, reference| {
                    let (c, r) = crate::core::reach::human_limits(reference, moved, limits);
                    report = Some(crate::core::reach::ReachReport {
                        human: r,
                        ..Default::default()
                    });
                    c
                });
                if let (Some(EvalResult::Anim(o)), Some(r)) = (&out, report) {
                    crate::core::reach::remember_report(o, r);
                }
                out
            }
        },
        t @ (NodeType::RigGuide { .. }
        | NodeType::BuildSkeleton { .. }
        | NodeType::HardBind
        | NodeType::Envelope { .. }
        | NodeType::RangeOfMotion { .. }
        | NodeType::DeltaMush { .. }
        | NodeType::BakeWeights { .. }
        | NodeType::RigidBind { .. }
        | NodeType::RigidSkin
        | NodeType::RigProxy) => {
            let ports: Vec<Option<EvalResult>> = inputs.iter().cloned().map(Some).collect();
            crate::core::rig::node::cook(t, &ports, 0.0)
        }
        NodeType::Capture { falloff } => {
            let EvalResult::Named(prims) = prims_of(inputs.first()?) else {
                return None;
            };
            let skeleton = inputs.get(1)?.as_anim()?;
            Some(EvalResult::Named(crate::core::skin::capture(
                &prims, skeleton, *falloff,
            )))
        }
        NodeType::Skin => {
            let EvalResult::Named(prims) = prims_of(inputs.first()?) else {
                return None;
            };
            let (rest, clip) = (inputs.get(1)?.as_anim()?, inputs.get(2)?.as_anim()?);
            // Keyed by the clip, the rest skeleton and the geometry.
            let key = format!(
                "{node_type:?}:{:p}:{:p}",
                Arc::as_ptr(rest),
                prims.first().map(|p| Arc::as_ptr(&p.mesh)).unwrap_or(std::ptr::null())
            );
            let mut report = None;
            let out = anim::memo(&key, Some(clip), || {
                let (c, r) = crate::core::skin::skin(&prims, rest, clip);
                report = Some(r);
                Some(c)
            });
            if let (Some(o), Some(r)) = (&out, report) {
                crate::core::skin::remember_report(o, r);
            }
            out.map(EvalResult::Anim)
        }

        // Ragdoll
        NodeType::LoadFbxMesh { path } => {
            let meshes = crate::fbx_loader::load_meshes_cached(path).ok()?;
            Some(EvalResult::Named(
                meshes
                    .iter()
                    .map(|(name, mesh)| Prim {
                        path: format!("/{name}"),
                        mesh: mesh.clone(),
                        picked: false,
                        material: None,
                        bindings: None,
                        look: None,
                        stage: None,
                        geo: None,
                        source: None,
                        place: Placement::InPlace,
                        purpose: Default::default(),
                        props: Default::default(),
                        above: Default::default(),
                        instancing: None,
                        instanced_by: None,
                        shading: None,
                        camera: None,
                        render: None,
                        light: None,
                        volume: None,
                    })
                    .collect(),
            ))
        }
        NodeType::Calamari { hulls, detail } => {
            anim_op(node_type, inputs, |a| crate::ragdoll::calamari(a, *hulls, *detail))
        }
        NodeType::Ragdoll { settings, limits, .. } => {
            // The clip, and the collider when one is wired. Nothing is solved
            // here: the node puts out the result of a solve when there is
            // one for exactly these inputs and settings.
            let clip = inputs.iter().find_map(|r| r.as_anim())?;
            let collider = inputs.iter().find(|r| r.as_anim().is_none()).map(|r| r.shared_mesh());
            Some(EvalResult::Anim(crate::ragdoll::output(
                clip,
                collider.as_ref(),
                settings,
                limits,
            )))
        }

        // UV
        NodeType::UvUnwrap {
            method,
            angle,
            margin,
            axis,
            tiles,
        } => inputs.first().map(|r| {
            r.map_mesh(|mut mesh| {
                mesh.uvs = (*crate::core::uv::unwrap_cached(&mesh, *method, *angle, *margin, *axis, *tiles)).clone();
                mesh
            })
        }),
        NodeType::UvTransform { offset, rotate, scale } => inputs.first().map(|r| {
            r.map_mesh(|mut mesh| {
                mesh.uvs = crate::core::uv::transform_all(&mesh, *offset, *rotate, *scale);
                mesh
            })
        }),
        NodeType::UvEdit { edits } => inputs.first().map(|r| {
            r.map_mesh(|mut mesh| {
                mesh.uvs = crate::core::uv::edit_islands(&mesh, edits);
                mesh
            })
        }),

        // Modelling
        // On packed primitives with some picked, only those are edited.
        NodeType::EditPoly { ops, .. } => inputs.first().map(|r| {
            r.map_mesh(|m| {
                let mesh = crate::core::poly::PolyMesh::from_mesh(&m);
                crate::core::poly::eval_cached(&mesh, ops, ops.len()).to_mesh()
            })
        }),
    }
}

/// Joint index a Split output refers to in this clip.
pub fn split_root(clip: &AnimData, pick: &crate::types::SplitPick) -> Option<usize> {
    match pick {
        crate::types::SplitPick::Character(i) => clip.character_roots().get(*i as usize).copied(),
        crate::types::SplitPick::Joint(name) => clip.joints.iter().position(|j| j.name == *name),
    }
}

/// Run a clip operator on the first input. Returns nothing when the input is
/// not animation. The result is memoised on the node parameters and the input.
fn anim_op(node_type: &NodeType, inputs: &[EvalResult], op: impl FnOnce(&AnimData) -> AnimData) -> Option<EvalResult> {
    let input: &Arc<AnimData> = inputs.first()?.as_anim()?;
    anim::memo(&format!("{node_type:?}"), Some(input), || Some(op(input))).map(EvalResult::Anim)
}

/// A shape as a primitive at `location` (see `prim_location`).
fn shape(location: &str, fallback: &str, mesh: MeshData) -> EvalResult {
    EvalResult::Named(vec![Prim::new(crate::types::prim_location(location, fallback), mesh)])
}

/// Whether a Merge in `mode` merges primitives, given its first input. Auto
/// goes by that input: primitives merge as primitives, a plain mesh or a clip
/// is flattened.
pub fn merges_primitives(mode: MergeMode, first: &EvalResult) -> bool {
    match mode {
        MergeMode::Auto => matches!(first, EvalResult::Named(_)),
        MergeMode::Primitives => true,
        MergeMode::Flatten => false,
    }
}

/// Two lists of primitives as one. A primitive of `second` on a path that
/// `first` has already is the stronger opinion: it takes that primitive's
/// place, as an `over` on the same path does in USD. What it leaves unsaid (no
/// material, no stage, no purpose) is read from the weaker one, so a shape put
/// on the path of a prim loaded from a file becomes an edit of that prim, and
/// is written as one.
///
/// Later in a list is stronger: a layer sublayers its stages in the order of
/// the last primitive of each (`usd_write`). So one of another stage taking a
/// primitive's place goes last, as its stage is the stronger.
pub fn merge_prims(first: &[Prim], second: &[Prim]) -> Vec<Prim> {
    let mut out: Vec<Prim> = first.to_vec();
    for p in second {
        match out.iter().position(|q| q.path == p.path) {
            Some(i) => {
                let stronger = over(&out[i], p);
                let other_stage = match (&out[i].stage, &p.stage) {
                    (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                    _ => false,
                };
                if other_stage {
                    out.remove(i);
                    out.push(stronger);
                } else {
                    out[i] = stronger;
                }
            }
            None => out.push(p.clone()),
        }
    }
    out
}

/// `strong` over `weak`, on the same path.
fn over(weak: &Prim, strong: &Prim) -> Prim {
    let from_stage = strong.stage.is_some();
    let has_material = strong.material.is_some();
    Prim {
        material: if has_material {
            strong.material.clone()
        } else {
            weak.material.clone()
        },
        look: if has_material {
            strong.look.clone()
        } else {
            weak.look.clone()
        },
        stage: if from_stage {
            strong.stage.clone()
        } else {
            weak.stage.clone()
        },
        source: if from_stage {
            strong.source.clone()
        } else {
            weak.source.clone()
        },
        purpose: if strong.purpose == crate::types::Purpose::Default {
            weak.purpose
        } else {
            strong.purpose
        },
        // Properties set on both: the stronger value of each.
        props: if weak.props.is_empty() {
            strong.props.clone()
        } else if strong.props.is_empty() {
            weak.props.clone()
        } else {
            let mut both = (*weak.props).clone();
            both.extend(strong.props.iter().map(|(k, v)| (k.clone(), v.clone())));
            Arc::new(both)
        },
        // Values on the Xforms above: the stronger where both set one.
        above: if weak.above.is_empty() {
            strong.above.clone()
        } else if strong.above.is_empty() {
            weak.above.clone()
        } else {
            let mut both = (*weak.above).clone();
            for (path, o) in strong.above.iter() {
                both.entry(path.clone()).or_default().merge(o);
            }
            Arc::new(both)
        },
        ..strong.clone()
    }
}

/// The primitives of the stage at `path`, read as Load USD reads it (see
/// `stage_prims`), with `variants` chosen over the file's (Variant), at the
/// frame being cooked when the stage changes over time (its time code there:
/// `usd_scene::cook_time`). One per mesh prim,
/// shared with the cached stage. `None` with nothing to show: no geometry,
/// instancer, camera or light.
pub fn load_stage(
    path: &str,
    convert: bool,
    variants: &crate::usd_scene::Variants,
    mask: &[String],
    mode: crate::usd_scene::LoadMode,
    location: &str,
    prim: &str,
) -> Option<Vec<Prim>> {
    let scene = crate::usd_scene::load_cached_with(path, convert, variants, mask).ok()?;
    let scene = match crate::usd_scene::cook_time(scene.tree.time_codes_per_second.or(scene.time.map(|t| t.2))) {
        Some(t) => crate::usd_scene::at_frame(&scene, t),
        None => scene,
    };
    if scene.meshes.is_empty() && scene.instancers.is_empty() && scene.cameras.is_empty() && scene.lights.is_empty() {
        return None;
    }
    stage_prims(&scene, mode, location, prim)
}

/// A loaded stage as primitives, as Load USD gives them: sublayered at the
/// file's own paths, or, referenced or payloaded, the referenced prim and what
/// is below it at `location`, every path rebased there (bindings and prototypes
/// too) and the stage's tree with them. `None` when a reference finds no prim
/// to bring in.
pub fn stage_prims(
    scene: &crate::usd_scene::UsdScene,
    mode: crate::usd_scene::LoadMode,
    location: &str,
    prim: &str,
) -> Option<Vec<Prim>> {
    use crate::usd_scene::{LoadMode, StageArc};
    let arc = match mode {
        LoadMode::Sublayer => None,
        LoadMode::Reference | LoadMode::Payload => Some(StageArc {
            payload: mode == LoadMode::Payload,
            prim: crate::usd_scene::referenced_prim(scene, prim)?,
            at: crate::types::prim_location(location, &crate::types::reference_location()),
        }),
    };
    // Where a path of the file is in the scene; what an arc leaves out is not.
    let place = |p: &str| match &arc {
        None => Some(p.to_string()),
        Some(a) => a.place(p),
    };
    // A path pointing elsewhere in the file (a binding outside the referenced
    // prim) stays as it is.
    let point = |p: &String| place(p).unwrap_or_else(|| p.clone());
    let tree = match &arc {
        None => scene.tree.clone(),
        Some(a) => Arc::new(crate::usd_scene::rebased(&scene.tree, a)),
    };
    let looks = scene.looks();
    // The meshes point instancers draw at their points.
    let members: std::collections::HashMap<&str, (&crate::usd_scene::UsdInstancer, u32, glam::Mat4)> = scene
        .instancers
        .iter()
        .flat_map(|i| {
            i.members
                .iter()
                .map(move |(path, k, inside)| (path.as_str(), (i, *k, *inside)))
        })
        .collect();
    let mut prims: Vec<Prim> = scene
        .meshes
        .iter()
        .filter_map(|m| {
            let path = place(&m.path)?;
            let material = m.material.as_ref().map(point);
            Some(Prim {
                path,
                mesh: m.mesh.clone(),
                picked: false,
                look: m.material.as_ref().and_then(|p| looks.get(p).cloned()),
                stage: Some(tree.clone()),
                geo: m.geo.clone(),
                // As loaded: what its edits are measured against.
                source: m.geo.clone().map(|geo| {
                    Arc::new(crate::types::Source {
                        geo,
                        place: m.place.clone(),
                        material: material.clone(),
                        purpose: m.purpose,
                    })
                }),
                material,
                bindings: None,
                place: m.place.clone(),
                purpose: m.purpose,
                props: Default::default(),
                above: Default::default(),
                instancing: None,
                instanced_by: members.get(m.path.as_str()).map(|(i, index, inside)| {
                    Arc::new(crate::types::InstancedBy {
                        instancer: point(&i.path),
                        index: *index,
                        inside: *inside,
                        hidden: i.hidden.clone(),
                        geo: i.geo.clone(),
                        place: i.place.clone(),
                    })
                }),
                shading: None,
                camera: None,
                render: None,
                light: None,
                volume: None,
            })
        })
        .collect();
    // Each point instancer, a points primitive of its own. One nested in
    // another's prototype is placed by that one's points, as a mesh is.
    let by_path: std::collections::HashMap<&str, &crate::usd_scene::UsdInstancer> =
        scene.instancers.iter().map(|i| (i.path.as_str(), i)).collect();
    prims.extend(scene.instancers.iter().filter_map(|i| {
        Some(Prim {
            path: place(&i.path)?,
            mesh: i.mesh.clone(),
            picked: false,
            material: None,
            bindings: None,
            look: None,
            stage: Some(tree.clone()),
            geo: Some(i.geo.clone()),
            source: Some(Arc::new(crate::types::Source {
                geo: i.geo.clone(),
                place: i.place.clone(),
                material: None,
                purpose: i.purpose,
            })),
            place: i.place.clone(),
            purpose: i.purpose,
            props: Default::default(),
            above: Default::default(),
            instancing: Some(match &arc {
                None => i.prototypes.clone(),
                Some(_) => Arc::new(i.prototypes.iter().map(point).collect()),
            }),
            instanced_by: i.by.as_ref().and_then(|(outer, index, inside)| {
                by_path.get(outer.as_str()).map(|o| {
                    Arc::new(crate::types::InstancedBy {
                        instancer: point(&o.path),
                        index: *index,
                        inside: *inside,
                        hidden: o.hidden.clone(),
                        geo: o.geo.clone(),
                        place: o.place.clone(),
                    })
                })
            }),
            shading: None,
            camera: None,
            render: None,
            light: None,
            volume: None,
        })
    }));
    // Each camera, a primitive with no geometry. Its geometry is the same
    // empty one as when loaded, so only its placement can be an edit.
    let empty = no_geometry();
    prims.extend(scene.cameras.iter().filter_map(|c| {
        Some(Prim {
            path: place(&c.path)?,
            stage: Some(tree.clone()),
            geo: Some(empty.clone()),
            source: Some(Arc::new(crate::types::Source {
                geo: empty.clone(),
                place: c.place.clone(),
                material: None,
                purpose: c.purpose,
            })),
            place: c.place.clone(),
            purpose: c.purpose,
            camera: Some(Arc::new(c.camera.clone())),
            ..Prim::new(String::new(), MeshData::default())
        })
    }));
    // Each light, as each camera: a primitive with no geometry, placed.
    prims.extend(scene.lights.iter().filter_map(|l| {
        Some(Prim {
            path: place(&l.path)?,
            stage: Some(tree.clone()),
            geo: Some(empty.clone()),
            source: Some(Arc::new(crate::types::Source {
                geo: empty.clone(),
                place: l.place.clone(),
                material: None,
                purpose: l.purpose,
            })),
            place: l.place.clone(),
            purpose: l.purpose,
            light: Some(Arc::new(l.light.clone())),
            ..Prim::new(String::new(), MeshData::default())
        })
    }));
    Some(prims)
}

/// The geometry of a primitive that has none (a camera): one, shared.
pub fn no_geometry() -> Arc<crate::core::geo::Geo> {
    static EMPTY: std::sync::OnceLock<Arc<crate::core::geo::Geo>> = std::sync::OnceLock::new();
    EMPTY
        .get_or_init(|| Arc::new(crate::core::geo::Geo::from_points(vec![])))
        .clone()
}

/// Packed primitives from whatever comes in: a mesh is one primitive.
fn prims_of(r: &EvalResult) -> EvalResult {
    match r {
        EvalResult::Named(p) => EvalResult::Named(p.clone()),
        EvalResult::Single(m) => EvalResult::Named(vec![Prim::new("/mesh".into(), m.clone())]),
        EvalResult::Anim(_) | EvalResult::Material(_) | EvalResult::Image(..) => EvalResult::Named(vec![]),
    }
}

/// A clip operator with two clip inputs. Needs both wired.
fn anim_op2(
    node_type: &NodeType,
    inputs: &[EvalResult],
    op: impl FnOnce(&AnimData, &AnimData) -> AnimData,
) -> Option<EvalResult> {
    let a: &Arc<AnimData> = inputs.first()?.as_anim()?;
    let b: &Arc<AnimData> = inputs.get(1)?.as_anim()?;
    // The second input is part of the key by its address.
    anim::memo(&format!("{node_type:?}:{:p}", Arc::as_ptr(b)), Some(a), || {
        Some(op(a, b))
    })
    .map(EvalResult::Anim)
}

// Generators

pub fn create_cube(size: f32) -> MeshData {
    let s = size / 2.0;
    // Six quads, counter-clockwise seen from outside.
    MeshData::from_polys(
        vec![
            [-s, -s, -s],
            [s, -s, -s],
            [s, s, -s],
            [-s, s, -s],
            [-s, -s, s],
            [s, -s, s],
            [s, s, s],
            [-s, s, s],
        ],
        vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7], // -Z, +Z
            vec![0, 1, 5, 4],
            vec![3, 7, 6, 2], // -Y, +Y
            vec![0, 4, 7, 3],
            vec![1, 2, 6, 5], // -X, +X
        ],
    )
}

pub fn create_sphere(radius: f32, segments: u32) -> MeshData {
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let mut quads = Vec::new();
    for lat in 0..=segments {
        let theta = lat as f32 * std::f32::consts::PI / segments as f32;
        let (st, ct) = (theta.sin(), theta.cos());
        for lon in 0..=segments {
            let phi = lon as f32 * 2.0 * std::f32::consts::PI / segments as f32;
            verts.push([phi.cos() * st * radius, ct * radius, phi.sin() * st * radius]);
        }
    }
    for lat in 0..segments {
        for lon in 0..segments {
            let f = lat * (segments + 1) + lon;
            let s = f + segments + 1;
            // Counter-clockwise seen from outside, like the cube and the grid.
            idx.extend_from_slice(&[f, f + 1, s, s, f + 1, s + 1]);
            quads.push(vec![f, f + 1, s + 1, s]);
        }
    }
    let mut m = MeshData::from_triangles(verts, idx);
    m.face_count = quads.len();
    m.polys = quads;
    // On a sphere the normal is the direction from the centre. This also
    // covers the poles, where the triangles have no area.
    m.normals = m
        .vertices
        .iter()
        .map(|v| Vec3::from_array(*v).normalize_or(Vec3::Y).to_array())
        .collect();
    m
}

pub fn create_grid(rows: u32, cols: u32, size: f32) -> MeshData {
    let mut verts = Vec::new();
    let mut quads = Vec::new();
    let rc = rows + 1;
    let cc = cols + 1;
    let cw = size / cols as f32;
    let ch = size / rows as f32;
    let ox = -size / 2.0;
    let oz = -size / 2.0;
    for r in 0..rc {
        for c in 0..cc {
            verts.push([ox + c as f32 * cw, 0.0, oz + r as f32 * ch]);
        }
    }
    for r in 0..rows {
        for c in 0..cols {
            let tl = r * cc + c;
            let tr = tl + 1;
            let bl = tl + cc;
            let br = bl + 1;
            quads.push(vec![tl, bl, br, tr]);
        }
    }
    MeshData::from_polys(verts, quads)
}

// Operators

/// The matrix of a Transform node: scale, then rotation X, Y, Z (radians), then
/// translation.
pub fn transform_matrix(t: Vec3, r: Vec3, s: Vec3) -> glam::Mat4 {
    glam::Mat4::from_scale_rotation_translation(s, Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z), t)
}

/// The matrix of a Transform operator: scale, then rotation in `order`
/// (radians), both about `pivot`, then translation; USD's translate, pivot,
/// rotate, scale and the pivot back.
pub fn transform_about(t: Vec3, r: Vec3, s: Vec3, pivot: Vec3, order: crate::types::RotationOrder) -> glam::Mat4 {
    glam::Mat4::from_translation(t + pivot)
        * glam::Mat4::from_quat(order.quat(r))
        * glam::Mat4::from_scale(s)
        * glam::Mat4::from_translation(-pivot)
}

/// A skeleton scales the same on every axis: a clip takes the X scale.
pub fn clip_scale(s: Vec3) -> f32 {
    if s.x.abs() > 1e-6 {
        s.x
    } else {
        1e-6
    }
}

pub fn transform(mesh: &MeshData, t: Vec3, r: Vec3, s: Vec3) -> MeshData {
    let rot = Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z);
    let mut m = MeshData {
        vertices: mesh
            .vertices
            .iter()
            .map(|v| (rot * (Vec3::from_array(*v) * s) + t).to_array())
            .collect(),
        indices: mesh.indices.clone(),
        points: mesh
            .points
            .iter()
            .map(|p| (rot * (Vec3::from_array(*p) * s) + t).to_array())
            .collect(),
        polys: mesh.polys.clone(),
        face_count: mesh.face_count,
        uvs: mesh.uvs.clone(),
        curve_points: mesh
            .curve_points
            .iter()
            .map(|p| (rot * (Vec3::from_array(*p) * s) + t).to_array())
            .collect(),
        curve_counts: mesh.curve_counts.clone(),
        curve_basis: mesh.curve_basis,
        curve_wrap: mesh.curve_wrap,
        widths: mesh.widths.clone(),
        ..Default::default()
    };
    // Recompute normals after transform so they stay correct
    if !mesh.normals.is_empty() {
        m.compute_normals();
    }
    m
}

/// A mesh moved by a matrix: an instanced copy made into a mesh of its own. A
/// mirroring matrix turns the faces round, so they keep facing out.
pub fn place_mesh(mesh: &MeshData, m: &glam::Mat4) -> MeshData {
    let mut out = MeshData {
        vertices: mesh
            .vertices
            .iter()
            .map(|v| m.transform_point3(Vec3::from_array(*v)).to_array())
            .collect(),
        indices: mesh.indices.clone(),
        points: mesh
            .points
            .iter()
            .map(|p| m.transform_point3(Vec3::from_array(*p)).to_array())
            .collect(),
        polys: mesh.polys.clone(),
        face_count: mesh.face_count,
        uvs: mesh.uvs.clone(),
        curve_points: mesh
            .curve_points
            .iter()
            .map(|p| m.transform_point3(Vec3::from_array(*p)).to_array())
            .collect(),
        curve_counts: mesh.curve_counts.clone(),
        curve_basis: mesh.curve_basis,
        curve_wrap: mesh.curve_wrap,
        widths: mesh.widths.clone(),
        // What each element carries does not move with it: the same values,
        // so the Primitive Inspector lists them where the prim is drawn.
        primvars: mesh.primvars.clone(),
        ..Default::default()
    };
    if m.determinant() < 0.0 {
        for tri in out.indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
        if out.uvs.len() == out.indices.len() {
            for tri in out.uvs.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
        }
        for poly in out.polys.iter_mut() {
            poly.reverse();
        }
        // Corner values follow their polygon's corners round.
        let sizes: Vec<usize> = out.polys.iter().map(|p| p.len()).collect();
        let corners: usize = sizes.iter().sum();
        for pv in out
            .primvars
            .iter_mut()
            .filter(|p| p.interp == crate::types::PrimVarInterp::FaceVarying && p.values.len() == corners)
        {
            let mut at = 0;
            for n in &sizes {
                pv.values[at..at + n].reverse();
                at += n;
            }
        }
    }
    if !mesh.normals.is_empty() {
        out.compute_normals();
    }
    out
}

/// Any number of meshes as one, in a single pass.
pub fn merge_all(parts: &[&MeshData]) -> MeshData {
    match parts {
        [] => return MeshData::default(),
        [one] => return (*one).clone(),
        _ => {}
    }
    let mut m = MeshData::default();
    m.vertices.reserve(parts.iter().map(|p| p.vertices.len()).sum());
    m.indices.reserve(parts.iter().map(|p| p.indices.len()).sum());
    let any_polys = parts.iter().any(|p| !p.polys.is_empty());
    let any_uvs = parts.iter().any(|p| !p.uvs.is_empty());
    let all_normals = parts.iter().all(|p| p.normals.len() == p.vertices.len());
    // Curves of one kind stay as they are; of several, they become polylines.
    let mut kinds = parts
        .iter()
        .filter(|p| !p.curve_counts.is_empty())
        .map(|p| (p.curve_basis, p.curve_wrap));
    let first_kind = kinds.next();
    let one_kind = kinds.all(|k| Some(k) == first_kind);
    if let (Some((basis, wrap)), true) = (first_kind, one_kind) {
        m.curve_basis = basis;
        m.curve_wrap = wrap;
    }
    let any_widths = parts.iter().any(|p| !p.widths.is_empty());
    for p in parts {
        if !p.curve_counts.is_empty() {
            if one_kind {
                m.curve_points.extend_from_slice(&p.curve_points);
                m.curve_counts.extend_from_slice(&p.curve_counts);
            } else {
                let (points, counts) = crate::strands::as_polylines(p);
                m.curve_points.extend(points);
                m.curve_counts.extend(counts);
            }
        }
        if any_widths {
            // One per point; a part without widths gets 0, the default size.
            let w = |i: usize| p.widths.get(i).or(p.widths.first()).copied().unwrap_or(0.0);
            m.widths.extend((0..p.points.len()).map(w));
        }
        let off = m.vertices.len() as u32;
        m.vertices.extend_from_slice(&p.vertices);
        m.indices.extend(p.indices.iter().map(|i| i + off));
        m.points.extend_from_slice(&p.points);
        if any_polys {
            if p.polys.is_empty() {
                m.polys.extend(
                    p.indices
                        .chunks_exact(3)
                        .map(|t| t.iter().map(|i| i + off).collect::<Vec<u32>>()),
                );
            } else {
                m.polys.extend(
                    p.polys
                        .iter()
                        .map(|poly| poly.iter().map(|i| i + off).collect::<Vec<u32>>()),
                );
            }
        }
        if any_uvs {
            if p.uvs.len() == p.indices.len() {
                m.uvs.extend_from_slice(&p.uvs);
            } else {
                m.uvs.extend(std::iter::repeat([0.0; 2]).take(p.indices.len()));
            }
        }
        if all_normals {
            m.normals.extend_from_slice(&p.normals);
        }
    }
    m.face_count = if m.polys.is_empty() {
        m.indices.len() / 3
    } else {
        m.polys.len()
    };
    if !all_normals {
        m.compute_normals();
    }
    m
}

pub fn merge(a: &MeshData, b: &MeshData) -> MeshData {
    let mut verts = a.vertices.clone();
    let mut idx = a.indices.clone();
    let off = verts.len() as u32;
    verts.extend(&b.vertices);
    idx.extend(b.indices.iter().map(|i| i + off));
    let mut pts = a.points.clone();
    pts.extend(&b.points);
    // Polygons survive when either side has them.
    let mut polys = vec![];
    if !a.polys.is_empty() || !b.polys.is_empty() {
        polys = a.polygons();
        polys.extend(
            b.polygons()
                .into_iter()
                .map(|p| p.into_iter().map(|i| i + off).collect::<Vec<u32>>()),
        );
    }
    let mut m = MeshData {
        vertices: verts,
        indices: idx,
        points: pts,
        ..Default::default()
    };
    m.face_count = if polys.is_empty() {
        m.indices.len() / 3
    } else {
        polys.len()
    };
    m.polys = polys;
    // UVs survive when either side has them; the other side gets zeros.
    if !a.uvs.is_empty() || !b.uvs.is_empty() {
        let side = |x: &MeshData| {
            if x.uvs.len() == x.indices.len() {
                x.uvs.clone()
            } else {
                vec![[0.0; 2]; x.indices.len()]
            }
        };
        m.uvs = side(a);
        m.uvs.extend(side(b));
    }
    m.compute_normals();
    m
}

#[cfg(test)]
mod packed_tests {
    use super::*;
    use crate::types::{IceGraphId, NodeType};

    fn pass(_: IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]) -> crate::core::geo::Geo {
        m.clone()
    }
    fn run(node: NodeType, input: &EvalResult) -> EvalResult {
        evaluate_node_type(&node, std::slice::from_ref(input), &pass, 0).unwrap()
    }
    /// Three unit cubes side by side, as packed primitives.
    fn packed() -> EvalResult {
        EvalResult::Named(
            (0..3)
                .map(|i| {
                    let mesh = transform(
                        &create_cube(1.0),
                        glam::Vec3::new(i as f32 * 5.0, 0.0, 0.0),
                        glam::Vec3::ZERO,
                        glam::Vec3::ONE,
                    );
                    Prim::new(format!("/car/{}", ["body", "wheel_L", "wheel_R"][i]), mesh)
                })
                .collect(),
        )
    }
    fn prims(r: &EvalResult) -> &[Prim] {
        match r {
            EvalResult::Named(p) => p,
            _ => panic!("not packed"),
        }
    }
    fn max_y(m: &MeshData) -> f32 {
        m.vertices.iter().map(|v| v[1]).fold(f32::MIN, f32::max)
    }

    /// Transform turns in its order, about its pivot; the angles read back
    /// from a rotation in any order make it again.
    #[test]
    fn transform_turns_in_its_order_about_its_pivot() {
        use crate::types::RotationOrder;
        use std::f32::consts::FRAC_PI_2;
        let near = |a: Vec3, b: Vec3| (a - b).length() < 1e-5;
        // A quarter turn about Y, about (1, 0, 0): that point stays, the
        // origin goes round it.
        let m = transform_about(Vec3::ZERO, Vec3::Y * FRAC_PI_2, Vec3::ONE, Vec3::X, RotationOrder::XYZ);
        assert!(near(m.transform_point3(Vec3::X), Vec3::X));
        assert!(near(m.transform_point3(Vec3::ZERO), Vec3::new(1.0, 0.0, 1.0)));
        // X then Y, or Y then X: up goes different ways.
        let r = Vec3::new(FRAC_PI_2, FRAC_PI_2, 0.0);
        let up = |o| transform_about(Vec3::ZERO, r, Vec3::ONE, Vec3::ZERO, o).transform_vector3(Vec3::Y);
        assert!(near(up(RotationOrder::XYZ), Vec3::X), "{}", up(RotationOrder::XYZ));
        assert!(near(up(RotationOrder::ZYX), Vec3::Z), "{}", up(RotationOrder::ZYX));
        // Graphs saved before the order: as glam's XYZ, which they used.
        let before = RotationOrder::before_orders().quat(Vec3::new(0.3, 0.5, 0.7));
        assert!(before.abs_diff_eq(Quat::from_euler(EulerRot::XYZ, 0.3, 0.5, 0.7), 1e-6));
        for o in RotationOrder::ALL {
            let r = Vec3::new(0.3, -0.5, 0.7);
            assert!(near(o.angles(o.quat(r)), r), "{o:?}");
        }
    }

    /// Prototypes from one Get Reference are that input's prims: another
    /// input with a prim at the same path is not instanced.
    #[test]
    fn prototypes_come_from_their_own_reference_input() {
        let cube = |size: f32| EvalResult::Named(vec![Prim::new("/tree".into(), create_cube(size))]);
        let references = [cube(1.0), cube(3.0)];
        let made = xms_ice::Instances {
            path: "scatter".into(),
            prototypes: vec!["/tree".into()],
            prototypes_input: Some(2),
            proto_indices: vec![0],
            positions: vec![[0.0; 3]],
            orientations: vec![[0.0, 0.0, 0.0, 1.0]],
            scales: vec![[1.0; 3]],
        };
        let on = Prim::new("/ground".into(), create_cube(4.0));
        let mut taken = Default::default();
        let out = instanced(&on, &made, Default::default(), &references, &mut taken);
        let protos: Vec<&Prim> = out.iter().filter(|p| p.instanced_by.is_some()).collect();
        assert_eq!(protos.len(), 1);
        assert_eq!(max_y(&protos[0].mesh), 1.5, "the second input's tree");
        // Unknown (paths from two inputs, put together): every input's.
        let made = xms_ice::Instances {
            prototypes_input: None,
            ..made
        };
        let out = instanced(&on, &made, Default::default(), &references, &mut Default::default());
        assert_eq!(out.iter().filter(|p| p.instanced_by.is_some()).count(), 2);
    }

    #[test]
    fn transform_moves_instances_without_copying_their_mesh() {
        use glam::Mat4;
        let shared = Arc::new(create_cube(1.0));
        let copy = |x: f32| Prim {
            mesh: shared.clone(),
            place: Placement::One(Mat4::from_translation(Vec3::new(x, 0.0, 0.0))),
            ..Prim::new(format!("/inst_{x}"), MeshData::default())
        };
        let input = EvalResult::Named(vec![copy(0.0), copy(10.0)]);
        let out = run(
            NodeType::Transform {
                translation: Vec3::Y * 2.0,
                rotation: Vec3::ZERO,
                scale: Vec3::ONE,
                pivot: Default::default(),
                order: Default::default(),
            },
            &input,
        );
        // The placements moved, the mesh is still the shared one.
        assert!(prims(&out).iter().all(|p| Arc::ptr_eq(&p.mesh, &shared)));
        // And the result is what moving the merged copies gives.
        let expected = transform(&input.as_mesh(), Vec3::Y * 2.0, Vec3::ZERO, Vec3::ONE);
        let got = out.as_mesh();
        assert_eq!(got.vertices.len(), expected.vertices.len());
        assert!(got
            .vertices
            .iter()
            .zip(&expected.vertices)
            .all(|(a, b)| (Vec3::from_array(*a) - Vec3::from_array(*b)).length() < 1e-5));
    }

    #[test]
    fn a_mirrored_copy_keeps_its_faces_facing_out() {
        let cube = create_cube(1.0);
        let mirrored = place_mesh(&cube, &glam::Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)));
        // Mirroring and turning the faces round gives the same normals, mirrored.
        let n = |m: &MeshData, t: usize| {
            let p = |k: usize| Vec3::from_array(m.vertices[m.indices[t * 3 + k] as usize]);
            (p(1) - p(0)).cross(p(2) - p(0)).normalize()
        };
        for t in 0..cube.indices.len() / 3 {
            let (a, b) = (n(&cube, t), n(&mirrored, t));
            assert!((Vec3::new(-a.x, a.y, a.z) - b).length() < 1e-5);
        }
    }

    #[test]
    fn pick_marks_by_path_pattern_and_shares_the_meshes() {
        let input = packed();
        let out = run(
            NodeType::PickPrims {
                pattern: "wheel_L$".into(),
            },
            &input,
        );
        let picked: Vec<bool> = prims(&out).iter().map(|p| p.picked).collect();
        assert_eq!(picked, vec![false, true, false]);
        assert!(out.has_picked() && !input.has_picked());
        // Nothing is copied on the way through.
        assert!(prims(&out)
            .iter()
            .zip(prims(&input))
            .all(|(a, b)| std::sync::Arc::ptr_eq(&a.mesh, &b.mesh)));
        assert_eq!(out.work_mesh().vertices.len(), create_cube(1.0).vertices.len());
    }

    #[test]
    fn a_mesh_node_after_a_pick_changes_only_the_picked() {
        let input = packed();
        let picked = run(
            NodeType::PickPrims {
                pattern: "wheel".into(),
            },
            &input,
        );
        let lift = NodeType::Transform {
            translation: glam::Vec3::new(0.0, 10.0, 0.0),
            rotation: glam::Vec3::ZERO,
            scale: glam::Vec3::ONE,
            pivot: Default::default(),
            order: Default::default(),
        };
        let out = run(lift.clone(), &picked);
        // The two wheels move, each staying a primitive of its own.
        let p = prims(&out);
        assert_eq!(
            p.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(),
            vec!["/car/body", "/car/wheel_L", "/car/wheel_R"]
        );
        assert!(
            std::sync::Arc::ptr_eq(&p[0].mesh, &prims(&input)[0].mesh),
            "the body is passed through as it is"
        );
        assert!(
            max_y(&p[0].mesh) < 1.0
                && max_y(&p[1].mesh) > 10.0
                && max_y(&p[2].mesh) > 10.0
                && p[1].picked
                && p[2].picked
        );
        // Without a pick Transform moves every primitive and keeps them apart.
        let all = run(lift, &input);
        assert_eq!(prims(&all).len(), prims(&input).len());
        assert!(prims(&all).iter().all(|p| p.mesh.vertices.iter().all(|v| v[1] > 9.0)));
    }

    #[test]
    fn a_placed_primitive_is_edited_in_its_own_space() {
        use crate::core::geo::{Context, Geo, POINTS, ST};
        use glam::Mat4;
        // A quad with UVs, placed ten units up, as a prim of a composed stage is.
        let mut geo = Geo::from_polygons(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            vec![4],
            vec![0, 1, 2, 3],
        );
        geo.set(
            Context::Corner,
            ST,
            crate::core::geo::Attr::new(
                crate::core::geo::Column::Vec2(Arc::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])),
                crate::core::geo::Role::TexCoord,
            ),
        );
        let geo = Arc::new(geo);
        let at = Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0));
        let prim = Prim {
            mesh: Arc::new(geo.to_mesh()),
            geo: Some(geo.clone()),
            place: Placement::One(at),
            picked: true,
            ..Prim::new("/set/card".into(), MeshData::default())
        };
        let input = EvalResult::Named(vec![prim]);

        // UVs only: still placed, points still the very same column.
        let out = run(
            NodeType::UvTransform {
                offset: [0.5, 0.0],
                rotate: 0.0,
                scale: [1.0, 1.0],
            },
            &input,
        );
        let p = &prims(&out)[0];
        assert!(matches!(p.place, Placement::One(m) if m == at));
        let g = p.geo.as_ref().expect("geometry kept");
        assert!(g
            .attr(Context::Point, POINTS)
            .unwrap()
            .same(geo.attr(Context::Point, POINTS).unwrap()));
        assert!(!g
            .attr(Context::Corner, ST)
            .unwrap()
            .same(geo.attr(Context::Corner, ST).unwrap()));
        assert!(
            p.mesh.vertices.iter().all(|v| v[1] <= 1.0),
            "the view stays in its own space"
        );

        // A picked Transform moves the placement: the geometry stays shared.
        let moved = run(
            NodeType::Transform {
                translation: Vec3::X,
                rotation: Vec3::ZERO,
                scale: Vec3::ONE,
                pivot: Default::default(),
                order: Default::default(),
            },
            &input,
        );
        let p = &prims(&moved)[0];
        assert!(Arc::ptr_eq(p.geo.as_ref().unwrap(), &geo));
        assert!(
            matches!(p.place, Placement::One(m) if (m.w_axis.x - 1.0).abs() < 1e-6 && (m.w_axis.y - 10.0).abs() < 1e-6)
        );
    }

    #[test]
    fn edit_poly_after_a_pick_edits_the_picked_primitive() {
        let picked = run(NodeType::PickPrims { pattern: "body".into() }, &packed());
        let out = run(
            NodeType::EditPoly {
                ops: vec![],
                pending: Default::default(),
                edit: None,
                auto_collapse: false,
            },
            &picked,
        );
        assert_eq!(prims(&out).len(), 3);
        assert!(prims(&out)[0].picked && prims(&out)[0].mesh.vertices.len() >= 8);
    }

    #[test]
    fn prune_removes_or_keeps_and_unpack_merges() {
        let input = packed();
        let without = run(
            NodeType::PrunePrims {
                pattern: "wheel".into(),
                keep: false,
            },
            &input,
        );
        assert_eq!(prims(&without).len(), 1);
        let only = run(
            NodeType::PrunePrims {
                pattern: "wheel".into(),
                keep: true,
            },
            &input,
        );
        assert_eq!(prims(&only).len(), 2);
        // An empty pattern leaves everything.
        assert_eq!(
            prims(&run(
                NodeType::PrunePrims {
                    pattern: " ".into(),
                    keep: true
                },
                &input
            ))
            .len(),
            3
        );
        let one = run(NodeType::UnpackPrims, &input);
        assert!(matches!(&one, EvalResult::Single(m) if m.vertices.len() == 3 * create_cube(1.0).vertices.len()));
    }

    #[test]
    fn merging_many_at_once_matches_merging_in_pairs() {
        let parts: Vec<MeshData> = prims(&packed()).iter().map(|p| (*p.mesh).clone()).collect();
        let refs: Vec<&MeshData> = parts.iter().collect();
        let all = merge_all(&refs);
        let pairs = merge(&merge(&parts[0], &parts[1]), &parts[2]);
        assert_eq!(
            (all.vertices.len(), all.indices.len(), all.polygons().len()),
            (pairs.vertices.len(), pairs.indices.len(), pairs.polygons().len())
        );
        assert_eq!(all.indices, pairs.indices);
        assert_eq!(all.normals.len(), all.vertices.len());
    }

    fn eval(node: NodeType, inputs: &[EvalResult]) -> EvalResult {
        evaluate_node_type(&node, inputs, &pass, 0).unwrap()
    }

    #[test]
    fn a_shape_is_a_primitive_at_its_location() {
        let cube = eval(NodeType::cube(1.0), &[]);
        assert_eq!(cube.prim_paths(), vec!["/cube"]);
        let typed = eval(
            NodeType::CreateSphere {
                radius: 1.0,
                segments: 8,
                location: " root//asset/1 ball ".into(),
            },
            &[],
        );
        assert_eq!(typed.prim_paths(), vec!["/root/asset/_1_ball"]);
        let empty = eval(
            NodeType::CreateGrid {
                rows: 2,
                cols: 2,
                size: 1.0,
                location: " / ".into(),
            },
            &[],
        );
        assert_eq!(empty.prim_paths(), vec!["/grid"], "nothing usable: the default");
    }

    #[test]
    fn a_lone_primitive_stays_itself_through_a_mesh_operator() {
        let cube = eval(NodeType::cube(1.0), &[]);
        let out = run(
            NodeType::EditPoly {
                ops: vec![],
                pending: Default::default(),
                edit: None,
                auto_collapse: false,
            },
            &cube,
        );
        assert_eq!(out.prim_paths(), vec!["/cube"]);
        let lifted = run(
            NodeType::UvTransform {
                offset: [0.5, 0.0],
                rotate: 0.0,
                scale: [1.0, 1.0],
            },
            &out,
        );
        assert_eq!(lifted.prim_paths(), vec!["/cube"]);
        assert!(prims(&lifted)[0].mesh.vertices.len() >= 8);
    }

    #[test]
    fn merge_goes_by_its_first_input() {
        let auto = || NodeType::merge();
        let sphere = eval(NodeType::sphere(0.5, 8), &[]);
        let cube = eval(NodeType::cube(1.0), &[]);
        // Primitives first: primitives out, each with its path.
        let both = eval(auto(), &[cube.clone(), sphere.clone()]);
        assert_eq!(both.prim_paths(), vec!["/cube", "/sphere"]);
        // A plain mesh first: one mesh, as before.
        let flat = eval(auto(), &[EvalResult::Single(create_cube(1.0)), sphere.clone()]);
        assert!(
            matches!(&flat, EvalResult::Single(m) if m.vertices.len() == create_cube(1.0).vertices.len() + create_sphere(0.5, 8).vertices.len())
        );
        // The mode overrides the input.
        assert!(matches!(
            eval(
                NodeType::Merge {
                    mode: MergeMode::Flatten,
                    inputs: 2
                },
                &[cube.clone(), sphere.clone()]
            ),
            EvalResult::Single(_)
        ));
        let forced = eval(
            NodeType::Merge {
                mode: MergeMode::Primitives,
                inputs: 2,
            },
            &[EvalResult::Single(create_cube(1.0)), sphere],
        );
        assert_eq!(forced.prim_paths(), vec!["/mesh", "/sphere"]);
    }

    #[test]
    fn on_the_same_path_the_second_input_is_stronger() {
        let small = eval(NodeType::cube(1.0), &[]);
        let big = eval(NodeType::cube(4.0), &[]);
        let out = eval(NodeType::merge(), &[small, big]);
        assert_eq!(out.prim_paths(), vec!["/cube"]);
        assert!(max_y(&prims(&out)[0].mesh) > 1.9, "the second cube wins");

        // A shape over a prim of a file: it takes the file prim's place and
        // keeps its stage, source and material, so it is written as an edit.
        let mut loaded = Prim::new("/car/body".into(), create_cube(1.0));
        loaded.material = Some("/looks/red".into());
        loaded.stage = Some(Arc::new(Default::default()));
        let body = EvalResult::Named(vec![loaded.clone(), prims(&packed())[1].clone()]);
        let shape = eval(
            NodeType::CreateCube {
                size: 3.0,
                location: "/car/body".into(),
            },
            &[],
        );
        let out = eval(NodeType::merge(), &[body, shape]);
        assert_eq!(
            out.prim_paths(),
            vec!["/car/body", "/car/wheel_L"],
            "the first input's order is kept"
        );
        let p = &prims(&out)[0];
        assert!(max_y(&p.mesh) > 1.4 && p.material.as_deref() == Some("/looks/red"));
        assert!(Arc::ptr_eq(p.stage.as_ref().unwrap(), loaded.stage.as_ref().unwrap()));
    }

    /// A primitive of another stage taking one's place goes last: its stage is
    /// the stronger, as the layer written sublayers it (the last primitive of
    /// each stage says how strong it is).
    #[test]
    fn a_prim_of_another_stage_on_the_same_path_goes_last() {
        let on = |path: &str, stage: &Arc<crate::usd_scene::StageTree>| Prim {
            stage: Some(stage.clone()),
            ..Prim::new(path.into(), create_cube(1.0))
        };
        let (a, b) = (Arc::new(Default::default()), Arc::new(Default::default()));
        let out = merge_prims(&[on("/x", &a), on("/z", &a)], &[on("/x", &b)]);
        let paths: Vec<&str> = out.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["/z", "/x"]);
        assert!(Arc::ptr_eq(out[1].stage.as_ref().unwrap(), &b));
        // Of the same stage (an edit of it): in its place.
        let out = merge_prims(&[on("/x", &a), on("/z", &a)], &[on("/x", &a)]);
        assert_eq!(out[0].path, "/x");
    }
}

#[cfg(test)]
mod place_tests {
    use super::place_mesh;
    use crate::types::{MeshData, PrimVar, PrimVarInterp};

    /// A placed primitive keeps what its elements carry: an attribute ICE wrote
    /// shows in the Primitive Inspector wherever the prim is placed.
    #[test]
    fn placing_keeps_the_primvars() {
        let mut m = MeshData::default();
        m.vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        m.polys = vec![vec![0, 1, 2]];
        m.indices = vec![0, 1, 2];
        m.face_count = 1;
        m.primvars.push(PrimVar {
            name: "foo".into(),
            interp: PrimVarInterp::Vertex,
            values: vec![vec![1.0], vec![2.0], vec![3.0]],
        });
        m.primvars.push(PrimVar {
            name: "c".into(),
            interp: PrimVarInterp::FaceVarying,
            values: vec![vec![1.0], vec![2.0], vec![3.0]],
        });
        let moved = place_mesh(&m, &glam::Mat4::from_translation(glam::Vec3::X));
        let rows = |m: &MeshData| {
            m.primvars
                .iter()
                .map(|p| (p.name.clone(), p.interp.clone(), p.values.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&moved), rows(&m));
        // Mirrored: the polygon turns round, and its corner values with it.
        let mirrored = place_mesh(&m, &glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)));
        assert_eq!(mirrored.primvars[0].values, m.primvars[0].values);
        assert_eq!(mirrored.primvars[1].values, vec![vec![3.0], vec![2.0], vec![1.0]]);
    }
}

/// The point instancer an Instancer made on `on`, as Imago keeps one (see
/// `core::instancing`): a points prim at its path, one point per instance with
/// `protoIndex`, `orient`, `scale` and `id`, placed as `on` is (the positions
/// are in its space); then its prototypes, every prim of each (the prim at the
/// prototype's path and the geometry below it, from the reference inputs) under
/// `<instancer>/Prototypes/<name>`, each keeping its place within the prototype
/// and drawn at its prototype's instances, sharing its geometry and material
/// (nothing is copied). Write USD writes it as a `PointInstancer`. A relative
/// path is beside `on` (not below it: USD has no prims under a mesh); a path
/// that is `taken` is numbered. The meshes among a reference input's prims, for
/// ICE to search: each where it is in the world, once per copy of an instanced
/// one. A mesh with no `Geo` is converted only if a graph asks for it.
fn reference_meshes(prims: &[Prim]) -> Vec<xms_ice::RefMesh> {
    prims
        .iter()
        .filter(|p| p.is_geometry() && (p.geo.is_some() || !p.mesh.indices.is_empty()))
        .flat_map(|p| {
            let geo = match &p.geo {
                Some(g) => xms_ice::RefGeo::ready(g.clone()),
                None => {
                    let m = p.mesh.clone();
                    xms_ice::RefGeo::later(move || crate::core::geo::Geo::from_mesh(&m))
                }
            };
            let worlds: Vec<glam::Mat4> = if p.place.is_placed() {
                p.place.matrices().to_vec()
            } else {
                vec![glam::Mat4::IDENTITY]
            };
            worlds.into_iter().map(move |world| xms_ice::RefMesh {
                path: p.path.clone(),
                world,
                geo: geo.clone(),
            })
        })
        .collect()
}

fn instanced(
    on: &Prim,
    made: &xms_ice::Instances,
    draw: crate::types::DrawMode,
    references: &[EvalResult],
    taken: &mut std::collections::HashSet<String>,
) -> Vec<Prim> {
    use crate::core::geo::{Attr, Column, Context, Geo, Role};
    use crate::types::InstancedBy;
    use glam::Mat4;
    let wanted = if made.path.starts_with('/') {
        made.path.trim_end_matches('/').to_string()
    } else {
        let parent = on.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        format!("{parent}/{}", made.path.trim_matches('/'))
    };
    let mut path = wanted.clone();
    let mut k = 1;
    while !taken.insert(path.clone()) {
        path = format!("{wanted}_{k}");
        k += 1;
    }

    let n = made.positions.len();
    let mut geo = Geo::from_points(made.positions.clone());
    for (name, column) in [
        ("protoIndex", Column::Int(Arc::new(made.proto_indices.clone()))),
        ("orient", Column::Quat(Arc::new(made.orientations.clone()))),
        ("scale", Column::Vec3(Arc::new(made.scales.clone()))),
        ("id", Column::Int(Arc::new((0..n as i32).collect()))),
    ] {
        geo.set(Context::Point, name, Attr::new(column, Role::None));
    }
    let geo = Arc::new(geo);
    // Each prototype under the instancer, by its leaf's name; numbered
    // where two share one.
    let mut roots: Vec<String> = vec![];
    for proto in &made.prototypes {
        let leaf = proto
            .rsplit('/')
            .next()
            .filter(|l| !l.is_empty())
            .unwrap_or("prototype");
        let mut root = format!("{path}/Prototypes/{leaf}");
        let mut k = 1;
        while roots.contains(&root) {
            root = format!("{path}/Prototypes/{leaf}_{k}");
            k += 1;
        }
        roots.push(root);
    }
    let place = on.place.clone();
    let mut out = vec![Prim {
        geo: Some(geo.clone()),
        place: place.clone(),
        instancing: Some(Arc::new(roots.clone())),
        ..Prim::new(path.clone(), geo.to_mesh())
    }];
    let below = |proto: &str, p: &str| p == proto || p.strip_prefix(proto).is_some_and(|rest| rest.starts_with('/'));
    // A draw mode chosen: on each prototype's root, as USD has it (a model,
    // its mode applied), which Write USD writes there.
    let drawn = |above: &Arc<std::collections::BTreeMap<String, crate::core::properties::Opinions>>, root: &str| {
        if draw == crate::types::DrawMode::Default {
            return above.clone();
        }
        use crate::core::properties::Value;
        let mut map = (**above).clone();
        let o = map.entry(root.to_string()).or_default();
        o.props.insert("kind".into(), Value::Token("component".into()));
        o.props.insert("model:applyDrawMode".into(), Value::Bool(true));
        o.props
            .insert("model:drawMode".into(), Value::Token(draw.token().into()));
        Arc::new(map)
    };
    // On one reference input (from 1), when the prototypes came from one.
    let searched = references
        .iter()
        .enumerate()
        .filter(|(i, _)| made.prototypes_input.is_none_or(|n| n == i + 1))
        .map(|(_, r)| r);
    for (k, proto) in made.prototypes.iter().enumerate() {
        for r in searched.clone() {
            let EvalResult::Named(prims) = r else { continue };
            for q in prims.iter().filter(|q| below(proto, &q.path) && q.is_geometry()) {
                let inside = match &q.place {
                    Placement::One(m) => *m,
                    Placement::InPlace => Mat4::IDENTITY,
                    // A prototype that is itself instanced: not yet.
                    Placement::Many(_) => continue,
                };
                if q.instancing.is_some() {
                    continue;
                }
                let by = InstancedBy {
                    instancer: path.clone(),
                    index: k as u32,
                    inside,
                    hidden: Arc::new(vec![]),
                    geo: geo.clone(),
                    place: place.clone(),
                };
                let copies = crate::core::instancing::copies(&geo, &place, &by);
                out.push(Prim {
                    path: format!("{}{}", roots[k], &q.path[proto.len()..]),
                    place: Placement::Many(copies.into()),
                    instanced_by: Some(Arc::new(by)),
                    // The reference's opinions above it were on its own paths.
                    above: drawn(&Arc::default(), &roots[k]),
                    picked: false,
                    stage: None,
                    source: None,
                    instancing: None,
                    ..q.clone()
                });
            }
        }
    }
    out
}

/// An image operator's image (`core::image_ops`) from its inputs: each image
/// that comes in brought into the rendering space first.
fn image_operator(t: &NodeType, inputs: &[EvalResult]) -> Option<Arc<crate::core::image_ops::Image>> {
    use crate::core::image_ops as ops;
    let images: Vec<Arc<xms_image::Image>> = inputs
        .iter()
        .filter_map(|r| match r {
            EvalResult::Image(img, space) => ops::working(img, space).ok(),
            _ => None,
        })
        .collect();
    let first = images.first();
    // It writes when asked (its Write button); its image passes through.
    if let NodeType::WriteImage { .. } = t {
        return first.cloned();
    }
    Some(Arc::new(match t {
        NodeType::Constant { colour, width, height } => ops::constant(*colour, *width as usize, *height as usize),
        NodeType::ImageMerge { op, mix, bounds, .. } => ops::merge(&images, *op, *mix, *bounds)?,
        NodeType::ColourCorrect { grade, layers } => ops::colour_correct(first?, grade, layers),
        NodeType::Shuffle { sources, to } => {
            // The second image, wired into "Second" (inputs come compacted).
            let second = (inputs.len() > 1).then(|| images.get(1)).flatten();
            ops::shuffle(first?, second.map(|i| i.as_ref()), sources, to)
        }
        NodeType::DeleteChannels { pattern, keep } => ops::delete_channels(first?, pattern, *keep),
        NodeType::Crop { x, y, width, height, frame } => ops::crop(first?, (*x, *y, *width as usize, *height as usize), *frame),
        NodeType::Reformat { width, height, fit, filter } => ops::reformat(first?, *width as usize, *height as usize, *fit, *filter),
        NodeType::Transform2D { translate, rotate, scale, pivot, filter } => ops::transform(first?, *translate, *rotate, *scale, *pivot, *filter),
        NodeType::Blur { size, gaussian } => ops::blur(first?, *size, *gaussian),
        NodeType::Premultiply { undo } => ops::premultiply(first?, *undo),
        _ => return None,
    }))
}


#[cfg(test)]
mod image_operator_tests {
    use super::*;
    use crate::core::image_ops as ops;

    fn flat(c: [f32; 4]) -> EvalResult {
        EvalResult::Image(Arc::new(ops::constant(c, 2, 2)), String::new())
    }

    fn red(r: &EvalResult) -> f32 {
        let EvalResult::Image(img, _) = r else { panic!("an image") };
        img.channels.iter().find(|c| c.name == "R").unwrap().values[0]
    }

    #[test]
    fn image_operators_cook_from_their_inputs() {
        let cc = NodeType::ColourCorrect { grade: ops::Grade { exposure: 1.0, ..Default::default() }, layers: String::new() };
        let pass = |_: IceGraphId, g: &crate::core::geo::Geo, _: &[crate::ice::Reference]| g.clone();
        let out = evaluate_node_type(&cc, &[flat([0.25, 0.0, 0.0, 1.0])], &pass, 0).unwrap();
        assert_eq!(red(&out), 0.5);
        let merge = NodeType::ImageMerge { inputs: 2, op: ops::MergeOp::Plus, mix: 1.0, bounds: ops::Bounds::Union };
        let out = evaluate_node_type(&merge, &[flat([0.25, 0.0, 0.0, 1.0]), flat([0.5, 0.0, 0.0, 1.0])], &pass, 0).unwrap();
        assert_eq!(red(&out), 0.75);
        // Nothing wired in: nothing out.
        assert!(evaluate_node_type(&cc, &[], &pass, 0).is_none());
        // Write Image passes its image through, the same pixels.
        let write = NodeType::WriteImage { path: String::new(), colour: String::new(), frames: Default::default(), start: 1, end: 1 };
        let input = flat([0.3, 0.0, 0.0, 1.0]);
        let out = evaluate_node_type(&write, std::slice::from_ref(&input), &pass, 0).unwrap();
        let (EvalResult::Image(a, _), EvalResult::Image(b, _)) = (&input, &out) else { panic!() };
        assert!(Arc::ptr_eq(a, b));
    }
}
