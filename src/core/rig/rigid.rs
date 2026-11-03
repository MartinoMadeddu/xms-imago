//! Rigid parts: a machine's pieces bound whole to one joint each (Rigid
//! Bind), moved whole by their joint (Rigid Skin), and simple shapes made for
//! a skeleton that has no mesh of its own (Rig Proxy).
//!
//! Rigid Skin moves a primitive bound to one joint by moving its placement,
//! so its geometry, materials and textures stay as they are and nothing is
//! copied: a 460,000-triangle car is moved by 43 matrices a frame. A
//! primitive whose points weigh on several joints is skinned point by point.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Mat4, Quat, Vec3};

use super::bind::{hard, joint_names, read_weights, segments, write_weights};
use super::Role;
use crate::core::anim::AnimData;
use crate::core::geo_mesh::GeoMesh;
use crate::types::{MeshData, Placement, Prim};

/// Primitives whose paths match `prims` follow `joint`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RigidRow {
    /// Words or regular expressions found in the paths, comma-separated.
    pub prims: String,
    /// A joint's name, or a part's (`wheel_L0`): its wheel if it has one,
    /// else its first joint.
    pub joint: String,
}

/// The joint a row names.
pub fn resolve(sk: &AnimData, name: &str) -> Option<usize> {
    let name = name.trim();
    if let Some(j) = sk.joints.iter().position(|j| j.name == name) {
        return Some(j);
    }
    let rig = sk.rig.as_deref()?;
    let of_part: Vec<usize> = (0..rig.joints.len()).filter(|i| rig.joints[*i].part == name).collect();
    of_part
        .iter()
        .copied()
        .find(|i| rig.joints[*i].role == Role::Wheel)
        .or_else(|| of_part.first().copied())
}

/// The deforming joint whose bone is nearest `p`.
fn nearest(sk: &AnimData, p: Vec3) -> usize {
    fn distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
        let ab = b - a;
        let t = if ab.length_squared() < 1e-12 {
            0.0
        } else {
            ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
        };
        p.distance(a + ab * t)
    }
    segments(sk)
        .into_iter()
        .enumerate()
        .filter_map(|(j, s)| s.map(|(a, b)| (j, distance(p, a, b))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
        .unwrap_or(0)
}

/// Rigid Bind: each primitive to one joint, every point weight one: the
/// first row whose primitives it is, else the joint whose bone is nearest
/// the middle of its bounds.
pub fn rigid_bind(prims: &[Prim], sk: &AnimData, rows: &[RigidRow]) -> Vec<Prim> {
    let names = joint_names(sk);
    let patterns: Vec<(crate::core::pattern::NamePattern, Option<usize>)> = rows
        .iter()
        .filter(|r| !r.prims.trim().is_empty())
        .map(|r| (crate::core::pattern::NamePattern::new(&r.prims), resolve(sk, &r.joint)))
        .collect();
    prims
        .iter()
        .map(|p| {
            if !p.is_geometry() || p.mesh.vertices.is_empty() {
                return p.clone();
            }
            let j = patterns
                .iter()
                .find(|(pat, _)| pat.matches(&p.path))
                .and_then(|x| x.1)
                .or_else(|| crate::viewport::bounds::world_bounds(p).map(|(lo, hi)| nearest(sk, (lo + hi) * 0.5)))
                .unwrap_or(0);
            let n = p.geo.as_ref().map(|g| g.point_count()).unwrap_or(p.mesh.vertices.len());
            write_weights(p, &vec![hard(j as u32); n], &names)
        })
        .collect()
}

/// Each joint of `clip` as one of `rest` by name: where it is in the clip's
/// frame against where it rests.
fn moves(rest: &AnimData, clip: &AnimData, frame: usize) -> Vec<Mat4> {
    let rw = rest.world_pose(0);
    let cw = clip.world_pose(frame);
    let by: HashMap<&str, usize> = clip
        .joints
        .iter()
        .enumerate()
        .map(|(i, j)| (j.name.as_str(), i))
        .collect();
    rest.joints
        .iter()
        .enumerate()
        .map(|(j, jt)| match by.get(jt.name.as_str()) {
            Some(c) => cw[*c] * rw[j].inverse(),
            None => Mat4::IDENTITY,
        })
        .collect()
}

/// Rigid Skin: the bound primitives at `clip`'s frame `frame`: one bound
/// whole to a joint moved by its placement, others point by point.
pub fn rigid_skin(prims: &[Prim], rest: &AnimData, clip: &AnimData, frame: usize) -> Vec<Prim> {
    let m = moves(rest, clip, frame);
    prims
        .iter()
        .map(|p| {
            let Some(w) = read_weights(p, rest) else {
                return p.clone();
            };
            let first = w.first().map(|x| x[0].0).unwrap_or(0);
            let whole = w.iter().all(|x| x[0].0 == first && x[0].1 > 0.999);
            if whole {
                let mv = m.get(first as usize).copied().unwrap_or(Mat4::IDENTITY);
                let place = match &p.place {
                    Placement::InPlace => Placement::One(mv),
                    Placement::One(x) => Placement::One(mv * *x),
                    Placement::Many(xs) => Placement::Many(xs.iter().map(|x| mv * *x).collect::<Vec<_>>().into()),
                };
                return Prim { place, ..p.clone() };
            }
            let Some(s) = super::surface::Surface::of(p) else {
                return p.clone();
            };
            let moved: Vec<Vec3> = s
                .points
                .iter()
                .zip(&w)
                .map(|(q, ws)| {
                    ws.iter()
                        .filter(|x| x.1 > 0.0)
                        .map(|(j, x)| {
                            m.get(*j as usize)
                                .copied()
                                .unwrap_or(Mat4::IDENTITY)
                                .transform_point3(*q)
                                * *x
                        })
                        .sum()
                })
                .collect();
            super::mush::with_points(p, &moved)
        })
        .collect()
}

/// A mesh being made: triangles, in the world.
#[derive(Default)]
struct Shape {
    v: Vec<[f32; 3]>,
    i: Vec<u32>,
}

impl Shape {
    /// A box: `at` places a unit cube from -0.5 to 0.5.
    fn cube(&mut self, at: Mat4) {
        let b = self.v.len() as u32;
        for k in 0..8 {
            let c = Vec3::new(
                if k & 1 == 0 { -0.5 } else { 0.5 },
                if k & 2 == 0 { -0.5 } else { 0.5 },
                if k & 4 == 0 { -0.5 } else { 0.5 },
            );
            self.v.push(at.transform_point3(c).to_array());
        }
        for f in [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ] {
            self.i
                .extend([b + f[0], b + f[1], b + f[2], b + f[0], b + f[2], b + f[3]]);
        }
        if at.determinant() < 0.0 {
            let from = self.i.len() - 36;
            for t in self.i[from..].chunks_exact_mut(3) {
                t.swap(1, 2);
            }
        }
    }

    /// A cylinder along X from 0 to 1, radius 1, placed by `at`.
    fn cylinder(&mut self, at: Mat4, sides: u32) {
        let b = self.v.len() as u32;
        for x in [0.0f32, 1.0] {
            self.v.push(at.transform_point3(Vec3::new(x, 0.0, 0.0)).to_array());
            for k in 0..sides {
                let a = k as f32 / sides as f32 * std::f32::consts::TAU;
                self.v
                    .push(at.transform_point3(Vec3::new(x, a.cos(), a.sin())).to_array());
            }
        }
        let (c0, c1) = (b, b + sides + 1);
        for k in 0..sides {
            let (p, q) = (k + 1, (k + 1) % sides + 1);
            self.i.extend([c0, b + q, b + p]);
            self.i.extend([c1, c1 + p, c1 + q]);
            self.i.extend([b + p, b + q, c1 + q, b + p, c1 + q, c1 + p]);
        }
    }

    fn mesh(self) -> MeshData {
        let mut m = MeshData {
            vertices: self.v,
            indices: self.i,
            ..Default::default()
        };
        m.face_count = m.indices.len() / 3;
        m.compute_normals();
        m
    }
}

/// A proxy's colour, by what its joint is, so the parts read apart.
fn look(role: Role) -> crate::types::Look {
    let (name, color, metallic) = match role {
        Role::Body => ("body", [0.42, 0.5, 0.6], 0.0),
        Role::Wheel | Role::TreadLink(_) => ("rubber", [0.16, 0.16, 0.17], 0.0),
        Role::Rotor | Role::Blade(_) => ("rotor", [0.85, 0.5, 0.18], 0.0),
        Role::Hinge => ("hinge", [0.86, 0.78, 0.4], 0.0),
        Role::Gear => ("brass", [0.78, 0.6, 0.3], 0.8),
        Role::PistonBase => ("cylinder", [0.6, 0.62, 0.65], 0.8),
        Role::PistonRod => ("rod", [0.85, 0.86, 0.88], 0.9),
        _ => ("bone", [0.86, 0.8, 0.68], 0.0),
    };
    crate::types::Look {
        name: format!("proxy_{name}"),
        color,
        roughness: 0.5,
        metallic,
        opacity: 1.0,
        color_map: None,
        emissive_map: None,
        cutout: false,
    }
}

/// A frame at `p` with axes `r` (columns), scaled by `s` along them.
fn frame(r: Mat3, p: Vec3, s: Vec3) -> Mat4 {
    Mat4::from_cols(
        (r.x_axis * s.x).extend(0.0),
        (r.y_axis * s.y).extend(0.0),
        (r.z_axis * s.z).extend(0.0),
        p.extend(1.0),
    )
}

/// Rig Proxy: simple shapes for a built skeleton, each its own primitive
/// (`/proxy/<joint>`), bound whole to its joint: a box for a body, a disc
/// for a wheel (with a bar across, to see it turn), blades, a plate for a
/// hinge, a cylinder and a rod for a piston, a toothed disc for a gear, a
/// block for a tread link, a box along each other bone.
pub fn rig_proxy(sk: &AnimData) -> Vec<Prim> {
    let Some(rig) = sk.rig.as_deref().filter(|r| r.joints.len() == sk.joints.len()) else {
        return vec![];
    };
    let w = sk.world_pose(0);
    let names = joint_names(sk);
    let pos: Vec<Vec3> = w.iter().map(|m| m.w_axis.truncate()).collect();
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for (p, j) in pos.iter().zip(&rig.joints) {
        if j.deform {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    let size = (hi - lo).max(Vec3::splat(1e-3));
    let wheel = rig
        .joints
        .iter()
        .filter(|j| j.role == Role::Wheel)
        .map(|j| j.radius)
        .fold(0.0f32, f32::max);
    let has_blades = |part: &str| {
        rig.joints
            .iter()
            .any(|j| j.part == part && matches!(j.role, Role::Blade(_)))
    };
    let mut out = vec![];
    for (i, j) in rig.joints.iter().enumerate() {
        if !j.deform {
            continue;
        }
        let rot = w[i].to_scale_rotation_translation().1 * j.canon;
        let c = Mat3::from_quat(rot);
        let p = pos[i];
        let len = p.distance(j.tip).max(size.max_element() * 0.01);
        let r = j.radius.max(len * 0.05);
        let mut s = Shape::default();
        match j.role {
            Role::Body => {
                // Its box, from its front, side and top locators; without
                // them, the machine's extent.
                let half = if j.size.min_element() > 0.0 {
                    j.size
                } else {
                    Vec3::new(size.x * 0.42, (wheel * 0.9).max(size.y * 0.3), size.z * 0.48)
                };
                s.cube(Mat4::from_scale_rotation_translation(half * 2.0, Quat::IDENTITY, p));
            }
            Role::Wheel => {
                let width = r * 0.5;
                s.cylinder(frame(c, p - c.x_axis * width * 0.5, Vec3::new(width, r, r)), 20);
                s.cube(frame(
                    c,
                    p + c.x_axis * width * 0.55,
                    Vec3::new(width * 0.1, r * 1.9, r * 0.25),
                ));
            }
            Role::Rotor if has_blades(&j.part) => {
                s.cylinder(
                    frame(c, p - c.x_axis * r * 0.05, Vec3::new(r * 0.12, r * 0.06, r * 0.06)),
                    12,
                );
            }
            Role::Rotor => {
                s.cylinder(frame(c, p, Vec3::new(r * 0.35, r, r)), 16);
            }
            Role::Blade(_) => {
                s.cube(frame(c, p + c.x_axis * r * 0.5, Vec3::new(r, r * 0.02, r * 0.09)));
            }
            Role::Hinge => {
                // From the pivot to the axis locator along the hinge line.
                let width = if j.radius > 0.0 { j.radius } else { len * 0.5 };
                s.cube(frame(
                    c,
                    p + c.x_axis * len * 0.5 + c.z_axis * width * 0.5,
                    Vec3::new(len, len * 0.06, width),
                ));
            }
            Role::PistonBase => {
                s.cylinder(frame(c, p, Vec3::new(len * 0.6, len * 0.07, len * 0.07)), 12);
            }
            Role::PistonRod => {
                s.cylinder(
                    frame(
                        c,
                        p - c.x_axis * len * 0.65,
                        Vec3::new(len * 0.65, len * 0.035, len * 0.035),
                    ),
                    10,
                );
                s.cube(frame(c, p, Vec3::splat(len * 0.1)));
            }
            Role::Gear => {
                let width = r * 0.25;
                s.cylinder(
                    frame(c, p - c.x_axis * width * 0.5, Vec3::new(width, r * 0.9, r * 0.9)),
                    24,
                );
                let teeth = 16;
                for k in 0..teeth {
                    let a = k as f32 / teeth as f32 * std::f32::consts::TAU;
                    let turn = Mat3::from_axis_angle(c.x_axis, a);
                    let out = turn * c.y_axis;
                    s.cube(frame(
                        turn * c,
                        p + out * r * 0.95,
                        Vec3::new(width, r * 0.15, r * 0.12),
                    ));
                }
            }
            Role::TreadLink(_) => {
                let n = rig.joints.iter().filter(|x| x.part == j.part).count().max(1) as f32;
                let along = j.tread.map(|t| t.length() / n).unwrap_or(len);
                s.cube(frame(c, p, Vec3::new(along * 0.85, r * 0.12, r * 0.9)));
            }
            Role::Steer | Role::Suspension => continue,
            _ => {
                s.cube(frame(
                    c,
                    p + c.x_axis * len * 0.5,
                    Vec3::new(len, len * 0.18, len * 0.18),
                ));
            }
        }
        let name = &sk.joints[i].name;
        let prim = Prim::new(format!("/proxy/{}", name.replace([':', '|', ' '], "_")), s.mesh());
        let prim = Prim {
            geo: Some(Arc::new(crate::core::geo::Geo::from_mesh(&prim.mesh))),
            look: Some(Arc::new(look(j.role))),
            ..prim
        };
        let n = prim.geo.as_ref().map(|g| g.point_count()).unwrap_or(0);
        out.push(write_weights(&prim, &vec![hard(i as u32); n], &names));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rig::bind::tests::two_bones;

    fn cube_at(path: &str, x: f32) -> Prim {
        let mut s = Shape::default();
        s.cube(Mat4::from_translation(Vec3::new(x, 0.0, 0.0)) * Mat4::from_scale(Vec3::splat(0.2)));
        let p = Prim::new(path.into(), s.mesh());
        Prim {
            geo: Some(Arc::new(crate::core::geo::Geo::from_mesh(&p.mesh))),
            ..p
        }
    }

    /// Two pieces: one given to a joint by a row, the other to the bone
    /// nearest it; skinned, each moves whole by its placement, its mesh
    /// untouched.
    #[test]
    fn pieces_bind_and_move_whole() {
        let sk = two_bones();
        let prims = vec![cube_at("/machine/arm", 1.5), cube_at("/machine/base", 0.4)];
        let bound = rigid_bind(
            &prims,
            &sk,
            &[RigidRow {
                prims: "arm".into(),
                joint: "b".into(),
            }],
        );
        let joint = |p: &Prim| read_weights(p, &sk).unwrap()[0][0].0;
        assert_eq!(joint(&bound[0]), 1);
        assert_eq!(joint(&bound[1]), 0);
        // Joint b turned a quarter about Z.
        let mut clip = sk.clone();
        let mut t = sk.joints[1].rest;
        t.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        clip.tracks = Arc::new(vec![vec![], vec![t], vec![]]);
        let moved = rigid_skin(&bound, &sk, &clip, 0);
        assert!(Arc::ptr_eq(&moved[0].mesh, &bound[0].mesh));
        let Placement::One(m) = moved[0].place else {
            panic!("not placed")
        };
        // The arm's centre, at x 1.5, swings to x 1, y 0.5.
        assert!((m.transform_point3(Vec3::new(1.5, 0.0, 0.0)) - Vec3::new(1.0, 0.5, 0.0)).length() < 1e-5);
        assert!(matches!(moved[1].place, Placement::One(m) if (m.transform_point3(Vec3::ZERO)).length() < 1e-5));
    }

    /// Rig Proxy: a shape per deforming joint, bound whole to it.
    #[test]
    fn a_proxy_per_joint() {
        let g = crate::core::rig::build::tests::human_guides();
        let sk = crate::core::rig::build::build(&g, &Default::default()).unwrap();
        let proxy = rig_proxy(&sk);
        let deforming = sk.rig.as_deref().unwrap().joints.iter().filter(|j| j.deform).count();
        assert_eq!(proxy.len(), deforming);
        for p in &proxy {
            let w = read_weights(p, &sk).unwrap();
            let j = w[0][0].0 as usize;
            assert!(w.iter().all(|x| x[0].0 as usize == j && x[0].1 > 0.999));
            assert!(p.path.ends_with(&sk.joints[j].name));
        }
    }
}
