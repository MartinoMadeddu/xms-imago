//! Normals' and Tangents' parameters (`core::frames`).

use crate::core::geo::frames::{TangentsFrom, Weighting};
use crate::core::geo::Context;

use super::form::{group, Status};

pub fn normals_rows(ui: &mut egui::Ui, context: &mut Context, weighting: &mut Weighting, cusp: &mut f32) {
    group(
        ui,
        "Normals",
        Some("Normals made from the geometry of the primitives the Prims Location tab names, or every one, written as normals (the one in another context taken away)"),
        |f| {
            f.choice(
                "Context",
                Some("Point: smooth, one a point. Corner: hard where faces meet past the cusp angle. Face: flat, one a face. Curves have them per point, square to the curve"),
                context,
                &[(Context::Point, "Point"), (Context::Corner, "Corner"), (Context::Face, "Face")],
            );
            let options: Vec<(Weighting, &str)> = Weighting::ALL.iter().map(|w| (*w, w.label())).collect();
            f.choice(
                "Weighting",
                Some("How much each face around a point counts: by its Area, by the Angle it makes there (even across thin triangles), or the same (Uniform)"),
                weighting,
                &options,
            );
            if *context == Context::Corner {
                f.drag(
                    "Cusp Angle",
                    Some("Faces meeting at more than this are hard: 0 flat, 180 smooth"),
                    cusp,
                    0.5,
                    "°",
                );
                *cusp = cusp.clamp(0.0, 180.0);
                f.status(
                    Status::Info,
                    "The Viewer shades by normals per point: a mesh with them per corner shows smooth; renders show its hard edges",
                );
            }
        },
    );
}

pub fn tangents_rows(
    ui: &mut egui::Ui,
    context: &mut Context,
    from: &mut TangentsFrom,
    uv: &mut String,
    bitangents: &mut bool,
) {
    group(
        ui,
        "Tangents",
        Some("Tangents made from the geometry of the primitives the Prims Location tab names, or every one, written as tangents: along U of the texture coordinates on meshes (what normal maps are baked against), along curves"),
        |f| {
            f.choice(
                "Context",
                Some("Point: one a point. Corner: one a corner, apart across UV seams. Curves have them per point"),
                context,
                &[(Context::Point, "Point"), (Context::Corner, "Corner")],
            );
            f.choice(
                "From",
                Some("UV: along U of the texture coordinates, else the edges. Edges: along each corner's edge to the next"),
                from,
                &[(TangentsFrom::Uv, "UV"), (TangentsFrom::Edges, "Edges")],
            );
            if *from == TangentsFrom::Uv {
                f.text(
                    "UV",
                    Some("The texture coordinates: empty, st (or another of their names, or any)"),
                    uv,
                    "st",
                );
            }
            f.toggle(
                "Bitangents",
                Some("Also bitangents: normal × tangent, turned where the coordinates are mirrored"),
                bitangents,
            );
        },
    );
}
