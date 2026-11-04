//! Deterministic, synthetic stress cases, not replacements for real models.
use std::fs;
use std::path::Path;
use zipzi_core::{
    BindingAxis, Canvas, Document, ImageAsset, ImageContent, Mesh, MeshBinding, MeshKeyform,
    Parameter, Vec2,
};
use zipzi_project::{content_sha256, encode_project};

fn id(index: u128) -> String {
    uuid::Uuid::from_u128(index).to_string()
}

fn initialized() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(
            id(1),
            Canvas::new(2048., 2048., Vec2::new(1024., 1024.), 100.)
        )
        .is_ok());
    document
}

fn texture(document: &mut Document, root: &Path, index: u128, side: u32) -> super::Result<()> {
    let mut rgba = vec![0; side as usize * side as usize * 4];
    // Fixed PRNG produces poorly compressible texels without a random dependency.
    let mut state = index as u64 + 1;
    for pixel in rgba.chunks_exact_mut(4) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        pixel.copy_from_slice(&[(state >> 16) as u8, (state >> 8) as u8, state as u8, 255]);
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, side, side);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header()?.write_image_data(&rgba)?;
    }
    let source = format!("assets/{index}.png");
    fs::write(root.join(&source), &bytes)?;
    super::status(
        document
            .add_asset(ImageAsset {
                id: id(index),
                name: format!("texture {index}"),
                unresolved_source: None,
                content: Some(ImageContent {
                    source,
                    width: side,
                    height: side,
                    sha256: content_sha256(&bytes),
                }),
            })
            .status,
    )
}

fn geometry(document: &mut Document) -> super::Result<()> {
    let parameter = id(100);
    super::status(
        document
            .create_parameter(Parameter {
                id: parameter.clone(),
                runtime_id: "Angle".into(),
                minimum: -1.,
                maximum: 1.,
                ..Default::default()
            })
            .status,
    )?;
    for index in 0..32 {
        let mut mesh = Mesh {
            id: id(1000 + index),
            texture_asset_id: id(2).into(),
            ..Default::default()
        };
        const SIDE: u32 = 48;
        for y in 0..SIDE {
            for x in 0..SIDE {
                mesh.vertex_ids.push(y * SIDE + x);
                mesh.base_positions.push(Vec2::new(x as f32, y as f32));
                mesh.uvs
                    .push(Vec2::new(x as f32 / SIDE as f32, y as f32 / SIDE as f32));
            }
        }
        for y in 0..SIDE - 1 {
            for x in 0..SIDE - 1 {
                let p = y * SIDE + x;
                mesh.triangles
                    .extend([[p, p + 1, p + SIDE], [p + 1, p + SIDE + 1, p + SIDE]]);
            }
        }
        super::status(document.create_mesh(mesh.clone()).status)?;
        super::status(
            document
                .create_binding(MeshBinding {
                    id: id(2000 + index),
                    mesh_id: mesh.id,
                    axes: vec![BindingAxis {
                        parameter_id: parameter.clone(),
                        keys: vec![-1., 0., 1.],
                        ..Default::default()
                    }],
                    keyforms: [-1., 0., 1.]
                        .iter()
                        .map(|&key| MeshKeyform {
                            keys: vec![key],
                            positions: mesh
                                .base_positions
                                .iter()
                                .map(|p| Vec2::new(p.x + key, p.y))
                                .collect(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                })
                .status,
        )?;
    }
    Ok(())
}

pub(super) fn create(output: &Path) -> super::Result<()> {
    // Refuse to mix new fixtures with existing user files.
    fs::create_dir(output)?;
    for category in [
        "empty",
        "small-images",
        "large-texture",
        "heavy-keyforms",
        "missing",
    ] {
        let root = output.join(category);
        fs::create_dir(&root)?;
        fs::create_dir(root.join("assets"))?;
        let mut document = initialized();
        match category {
            "small-images" => {
                for index in 2..258 {
                    texture(&mut document, &root, index, 32)?;
                }
            }
            "large-texture" => texture(&mut document, &root, 2, 2048)?,
            "heavy-keyforms" => {
                texture(&mut document, &root, 2, 64)?;
                geometry(&mut document)?;
            }
            "missing" => super::status(
                document
                    .add_asset(ImageAsset {
                        id: id(2),
                        name: "explicitly missing".into(),
                        content: None,
                        unresolved_source: None,
                    })
                    .status,
            )?,
            _ => (),
        }
        fs::write(
            root.join("project.zipzi.json"),
            encode_project(&document).map_err(|s| format!("{}: {}", s.code, s.message))?,
        )?;
    }
    Ok(())
}
