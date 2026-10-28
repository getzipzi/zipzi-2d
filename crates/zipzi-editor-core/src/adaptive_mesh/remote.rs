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

pub fn generate(input: Value) -> Value {
    let result = (|| -> Result<MeshGeometry, String> {
        let request: Request = serde_json::from_value(input).map_err(|e| e.to_string())?;
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
            return Err("Editor mesh requests support at most 4194304 source pixels".into());
        }
        if settings.max_vertices > 65534 {
            return Err("At most 65534 vertices are supported".into());
        }
        if alpha_hex.len() > 8388608 || alpha_hex.len() % 2 != 0 {
            return Err("Invalid alpha bytes".into());
        }
        let alpha = alpha_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digit = |c: u8| (c as char).to_digit(16).ok_or("Invalid alpha hex");
                Ok(((digit(pair[0])? << 4) | digit(pair[1])?) as u8)
            })
            .collect::<Result<Vec<u8>, &str>>()?;
        if stage == "interior" {
            if !alpha.is_empty() {
                return Err("Interior requests take geometry, not alpha".into());
            }
            let geometry = geometry.ok_or("Interior requires geometry")?;
            if geometry.vertex_ids.len() > 65534 || geometry.triangles.len() > 131068 {
                return Err("Input geometry exceeds Editor request limits".into());
            }
            if geometry.positions.len() != geometry.vertex_ids.len()
                || geometry.uvs.len() != geometry.vertex_ids.len()
                || geometry
                    .positions
                    .iter()
                    .chain(&geometry.uvs)
                    .any(|p| !p.x.is_finite() || !p.y.is_finite())
            {
                return Err("Invalid geometry arrays".into());
            }
            fill_existing_geometry(&geometry, width, height, &settings).map_err(|e| e.to_string())
        } else {
            if geometry.is_some() {
                return Err("Only interior accepts geometry".into());
            }
            let stage = match stage.as_str() {
                "edges" => MeshGenerationStage::Edges,
                "full" => MeshGenerationStage::Full,
                _ => return Err("Stage must be edges, interior or full".into()),
            };
            generate_mesh(width, height, &alpha, &settings, stage)
                .map(|result| result.geometry)
                .map_err(|e| e.to_string())
        }
    })();
    match result {
        Ok(geometry) => success(serde_json::to_value(geometry).expect("finite generated mesh")),
        Err(message) => failure("INVALID_ARGUMENT", &message),
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
}
