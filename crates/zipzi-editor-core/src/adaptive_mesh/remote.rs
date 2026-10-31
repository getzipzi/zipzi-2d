//! Bounded, document-independent Editor service. Results never mutate a session.
use super::*;
use serde::Deserialize;
use serde_json::Value;
use zipzi_runtime::protocol::{failure, success};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    #[serde(default)]
    alpha_hex: String,
    #[serde(default)]
    settings: MeshSettings,
    stage: String,
    #[serde(default)]
    geometry: Option<MeshGeometry>,
}

fn invalid(message: impl Into<String>) -> (&'static str, String) {
    ("INVALID_ARGUMENT", message.into())
}

fn mesh_failure(error: MeshError) -> (&'static str, String) {
    let code = match error {
        MeshError::Input | MeshError::Options => "INVALID_ARGUMENT",
        MeshError::Empty => "MESH_EMPTY_FOREGROUND",
        MeshError::Limit => "MESH_GENERATION_LIMIT",
        MeshError::Geometry(_) => "MESH_GENERATION_FAILED",
    };
    (code, error.to_string())
}

pub fn generate(input: Value) -> Value {
    let result = (|| -> Result<MeshGeometry, (&'static str, String)> {
        let request: Request = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
        let Request {
            width,
            height,
            alpha_hex,
            settings,
            stage,
            geometry,
        } = request;
        // Keep synchronous requests bounded; the ordinary protocol also bounds frames
        // and retains responses for same-request recovery.
        if width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || u64::from(width) * u64::from(height) > 4194304
        {
            return Err(invalid(
                "Editor mesh requests support at most 4194304 source pixels",
            ));
        }
        if settings.max_vertices > 65534 {
            return Err(invalid("At most 65534 vertices are supported"));
        }
        if alpha_hex.len() > 8388608 || alpha_hex.len() % 2 != 0 {
            return Err(invalid("Invalid alpha bytes"));
        }
        let alpha = alpha_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digit = |c: u8| (c as char).to_digit(16).ok_or("Invalid alpha hex");
                Ok(((digit(pair[0])? << 4) | digit(pair[1])?) as u8)
            })
            .collect::<Result<Vec<u8>, &str>>()
            .map_err(invalid)?;
        if stage == "interior" {
            if !alpha.is_empty() {
                return Err(invalid("Interior requests take geometry, not alpha"));
            }
            let geometry = geometry.ok_or_else(|| invalid("Interior requires geometry"))?;
            if geometry.vertex_ids.len() > 65534 || geometry.triangles.len() > 131068 {
                return Err(invalid("Input geometry exceeds Editor request limits"));
            }
            if geometry.positions.len() != geometry.vertex_ids.len()
                || geometry.uvs.len() != geometry.vertex_ids.len()
                || geometry
                    .positions
                    .iter()
                    .chain(&geometry.uvs)
                    .any(|p| !p.x.is_finite() || !p.y.is_finite())
            {
                return Err(invalid("Invalid geometry arrays"));
            }
            fill_existing_geometry(&geometry, width, height, &settings).map_err(mesh_failure)
        } else {
            if geometry.is_some() {
                return Err(invalid("Only interior accepts geometry"));
            }
            let stage = match stage.as_str() {
                "edges" => MeshGenerationStage::Edges,
                "full" => MeshGenerationStage::Full,
                _ => return Err(invalid("Stage must be edges, interior or full")),
            };
            generate_mesh(width, height, &alpha, &settings, stage)
                .map(|result| result.geometry)
                .map_err(mesh_failure)
        }
    })();
    match result {
        Ok(geometry) => success(serde_json::to_value(geometry).expect("finite generated mesh")),
        Err((code, message)) => failure(code, &message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(stage: &str) -> Value {
        json!({"width": 64, "height": 64, "stage": stage,
            "alpha_hex": (0..4096).map(|i| if (8..56).contains(&(i%64)) && (8..56).contains(&(i/64)) {"ff"} else {"00"}).collect::<String>(),
            "settings": {"edge_spacing": 12., "interior_spacing": 16., "clip_to_image": true}})
    }
    #[test]
    fn remote_stages_and_all_fill_algorithms() {
        let edges = generate(input("edges"));
        let original: MeshGeometry = serde_json::from_value(edges["ok"].clone()).unwrap();
        for algorithm in [
            "graded_poisson",
            "simple_triangles",
            "adaptive_quadtree",
            "triangle_fractal",
            "contour_paving",
        ] {
            let mut request = input("full");
            request["settings"]["fill_algorithm"] = json!(algorithm);
            let result = generate(request.clone());
            assert!(result.get("ok").is_some(), "{result}");
            assert_eq!(result, generate(request));
            let filled = generate(
                json!({"width":64,"height":64,"stage":"interior", "geometry": original,
                "settings":{"fill_algorithm":algorithm,"interior_spacing":12.}}),
            );
            let geometry: MeshGeometry = serde_json::from_value(filled["ok"].clone()).unwrap();
            assert!(geometry.vertex_ids.len() >= original.vertex_ids.len());
            for (i, id) in original.vertex_ids.iter().enumerate() {
                let j = geometry.vertex_ids.iter().position(|v| v == id).unwrap();
                assert_eq!(geometry.positions[j], original.positions[i]);
            }
        }
    }
    #[test]
    fn remote_rejects_invalid_payloads_without_panicking() {
        for patch in [
            json!({"stage":"invalid"}),
            json!({"alpha_hex":""}),
            json!({"width":0}),
            json!({"width":16384}),
            json!({"settings":{"edge_spacing":0.}}),
            json!({"settings":{"fill_algorithm":"unknown"}}),
            json!({"unknown":true}),
        ] {
            let mut request = input("full");
            request
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(generate(request).get("error").is_some());
        }
    }

    #[test]
    fn failure_categories_and_minimum_spacing_are_explicit() {
        for field in ["edge_spacing", "interior_spacing"] {
            let mut request = input("full");
            request["settings"][field] = json!(2.);
            assert_eq!(generate(request)["error"]["code"], "INVALID_ARGUMENT");
        }
        let mut empty = input("full");
        empty["alpha_hex"] = json!("00".repeat(4096));
        assert_eq!(generate(empty)["error"]["code"], "MESH_EMPTY_FOREGROUND");
        let mut limited = input("full");
        limited["settings"]["max_vertices"] = json!(3);
        assert_eq!(generate(limited)["error"]["code"], "MESH_GENERATION_LIMIT");
        assert_eq!(
            mesh_failure(MeshError::Geometry("invalid raster contour")).0,
            "MESH_GENERATION_FAILED"
        );
    }
}
