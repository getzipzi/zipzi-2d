pub use crate::common::import_cases;
use crate::common::purism::*;
use kasane_core::{TransformData, WarpTransform};

use std::collections::HashMap;
use std::os::raw::c_void;
use std::path::PathBuf;

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    Appearance, BindingAxis, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter,
    Transform, Vec2,
};
use kasane_core::Document;
use kasane_moc3::encode_moc3;

pub(crate) fn near(actual: f32, expected: f32, ppu: f32) {
    assert!(actual.is_finite(), "actual is not finite");
    assert!(expected.is_finite(), "expected is not finite");
    let e = (actual as f64 - expected as f64).abs();
    assert!(
        e <= 1e-5 + 1e-5 * (actual as f64).abs().max((expected as f64).abs())
            && e * ppu as f64 <= 0.05,
        "expected {}, actual {}, diff {}, pixel_diff {}",
        expected,
        actual,
        e,
        e * ppu as f64
    );
}

pub(crate) fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

pub(crate) fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest.ends_with("kasane-moc3") {
        manifest.parent().unwrap().parent().unwrap().to_path_buf()
    } else {
        manifest
    }
}

pub(crate) fn local_mao_moc3() -> PathBuf {
    workspace_root().join("models/local/mao/runtime/mao_pro.moc3")
}

pub(crate) struct PurismModelInstance {
    _moc_buffer: AlignedBuffer,
    _model_buffer: AlignedBuffer,
    _moc: *mut c_void,
    model: *mut c_void,
}

impl PurismModelInstance {
    pub(crate) fn new(moc_bytes: &[u8]) -> Self {
        let mut moc_buf = AlignedBuffer::new(moc_bytes.len(), 64);
        unsafe {
            std::ptr::copy_nonoverlapping(
                moc_bytes.as_ptr(),
                moc_buf.as_mut_ptr(),
                moc_bytes.len(),
            );
            assert_eq!(
                csmHasMocConsistency(moc_buf.as_mut_ptr() as *mut c_void, moc_bytes.len() as u32),
                1,
                "Purism consistency check failed"
            );
            let moc =
                csmReviveMocInPlace(moc_buf.as_mut_ptr() as *mut c_void, moc_bytes.len() as u32);
            assert!(!moc.is_null(), "csmReviveMocInPlace returned null");
            let model_size = csmGetSizeofModel(moc);
            assert!(model_size > 0, "csmGetSizeofModel returned 0");
            let mut model_buf = AlignedBuffer::new(model_size as usize, 16);
            let model =
                csmInitializeModelInPlace(moc, model_buf.as_mut_ptr() as *mut c_void, model_size);
            assert!(!model.is_null(), "csmInitializeModelInPlace returned null");

            PurismModelInstance {
                _moc_buffer: moc_buf,
                _model_buffer: model_buf,
                _moc: moc,
                model,
            }
        }
    }

    pub(crate) fn set_parameter(&mut self, param_id: &str, value: f32) {
        unsafe {
            let count = csmGetParameterCount(self.model) as usize;
            let ids = csmGetParameterIds(self.model);
            let values = csmGetParameterValues(self.model);
            for i in 0..count {
                let id_str = c_str_to_str(*ids.add(i));
                if id_str == param_id {
                    *values.add(i) = value;
                    return;
                }
            }
        }
    }

    pub(crate) fn update(&mut self) {
        unsafe {
            csmUpdateModel(self.model);
        }
    }

    pub(crate) fn part_count(&self) -> usize {
        unsafe { csmGetPartCount(self.model) as usize }
    }

    pub(crate) fn drawable_count(&self) -> usize {
        unsafe { csmGetDrawableCount(self.model) as usize }
    }

    pub(crate) fn get_drawable(&self, runtime_id: &str) -> Option<PurismDrawableData> {
        unsafe {
            let count = csmGetDrawableCount(self.model) as usize;
            let ids = csmGetDrawableIds(self.model);
            let opacities = csmGetDrawableOpacities(self.model);
            let draw_orders = csmGetDrawableDrawOrders(self.model);
            let render_orders = csmGetRenderOrders(self.model);
            let positions = csmGetDrawableVertexPositions(self.model);
            let vertex_counts = csmGetDrawableVertexCounts(self.model);
            let mul_colors = csmGetDrawableMultiplyColors(self.model);
            let scr_colors = csmGetDrawableScreenColors(self.model);

            for i in 0..count {
                let id_str = c_str_to_str(*ids.add(i));
                if id_str == runtime_id {
                    let vc = *vertex_counts.add(i) as usize;
                    let pos_ptr = *positions.add(i);
                    let mut pts = Vec::with_capacity(vc);
                    for v in 0..vc {
                        let pt = *pos_ptr.add(v);
                        pts.push(Vec2::new(pt.x, pt.y));
                    }
                    let mul = *mul_colors.add(i);
                    let scr = *scr_colors.add(i);

                    let uv_ptr = *csmGetDrawableVertexUvs(self.model).add(i);
                    let uvs = (0..vc)
                        .map(|v| {
                            let p = *uv_ptr.add(v);
                            Vec2::new(p.x, p.y)
                        })
                        .collect();
                    let index_count = *csmGetDrawableIndexCounts(self.model).add(i) as usize;
                    let index_ptr = *csmGetDrawableIndices(self.model).add(i);
                    let indices = (0..index_count).map(|v| *index_ptr.add(v) as u32).collect();
                    return Some(PurismDrawableData {
                        uvs,
                        indices,
                        opacity: *opacities.add(i),
                        draw_order: *draw_orders.add(i),
                        render_order: *render_orders.add(i),
                        positions: pts,
                        multiply_color: [mul.x, mul.y, mul.z, mul.w],
                        screen_color: [scr.x, scr.y, scr.z, scr.w],
                    });
                }
            }
        }
        None
    }
}

#[allow(dead_code)]
pub(crate) struct PurismDrawableData {
    pub(crate) uvs: Vec<Vec2>,
    pub(crate) indices: Vec<u32>,
    pub(crate) opacity: f32,
    pub(crate) draw_order: i32,
    pub(crate) render_order: i32,
    pub(crate) positions: Vec<Vec2>,
    pub(crate) multiply_color: [f32; 4],
    pub(crate) screen_color: [f32; 4],
}

pub(crate) fn create_m1_fixture_doc() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(320.0, 240.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "Tex0".to_string(),
            source: "textures/0.png".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    let p1 = Parameter {
        id: id(3),
        runtime_id: "ParamAngle".to_string(),
        name: "Angle".to_string(),
        minimum: -30.0,
        maximum: 30.0,
        default_value: 0.0,
        decimal_places: 4,
        ..Default::default()
    };
    assert!(doc.create_parameter(p1).status.is_ok());

    let warp = Transform {
        id: id(4),
        runtime_id: "WarpRoot".to_string(),
        name: "WarpRoot".to_string(),
        data: TransformData::Warp(WarpTransform {
            bezier: None,
            rows: 2,
            columns: 2,
            quad: true,
            points: vec![
                Vec2::new(120.0, 40.0),
                Vec2::new(320.0, 40.0),
                Vec2::new(520.0, 40.0),
                Vec2::new(120.0, 240.0),
                Vec2::new(320.0, 240.0),
                Vec2::new(520.0, 240.0),
                Vec2::new(120.0, 440.0),
                Vec2::new(320.0, 440.0),
                Vec2::new(520.0, 440.0),
            ],
        }),
        ..Default::default()
    };
    assert!(doc.create_transform(warp).status.is_ok());

    let m1 = Mesh {
        id: id(5),
        name: "QuadMesh".to_string(),
        runtime_id: "ArtMeshQuad".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(220.0, 140.0),
            Vec2::new(420.0, 140.0),
            Vec2::new(420.0, 340.0),
            Vec2::new(220.0, 340.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        deformer_id: id(4),
        appearance: Appearance {
            opacity: 1.0,
            multiply: [0.9, 0.8, 0.7],
            screen: [0.1, 0.2, 0.3],
        },
        draw_order: Some(15.0),
        ..Default::default()
    };
    assert!(doc.create_mesh(m1).status.is_ok());

    let binding = MeshBinding {
        id: id(6),
        mesh_id: id(5),
        axes: vec![BindingAxis {
            parameter_id: id(3),
            keys: vec![-30.0, 0.0, 30.0],
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![-30.0],
                positions: vec![
                    Vec2::new(200.0, 150.0),
                    Vec2::new(400.0, 130.0),
                    Vec2::new(410.0, 330.0),
                    Vec2::new(210.0, 350.0),
                ],
                appearance: Appearance {
                    opacity: 0.8,
                    multiply: [0.8, 0.7, 0.6],
                    screen: [0.0, 0.1, 0.2],
                },
                draw_order: Some(12.0),
            },
            MeshKeyform {
                keys: vec![0.0],
                positions: vec![
                    Vec2::new(220.0, 140.0),
                    Vec2::new(420.0, 140.0),
                    Vec2::new(420.0, 340.0),
                    Vec2::new(220.0, 340.0),
                ],
                appearance: Appearance {
                    opacity: 1.0,
                    multiply: [0.9, 0.8, 0.7],
                    screen: [0.1, 0.2, 0.3],
                },
                draw_order: Some(15.0),
            },
            MeshKeyform {
                keys: vec![30.0],
                positions: vec![
                    Vec2::new(240.0, 130.0),
                    Vec2::new(440.0, 150.0),
                    Vec2::new(430.0, 350.0),
                    Vec2::new(230.0, 330.0),
                ],
                appearance: Appearance {
                    opacity: 0.9,
                    multiply: [1.0, 0.9, 0.8],
                    screen: [0.2, 0.3, 0.4],
                },
                draw_order: Some(18.0),
            },
        ],
    };
    assert!(doc.create_binding(binding).status.is_ok());

    doc
}

pub(crate) fn exported_drawable(doc: &Document) -> PurismDrawableData {
    let exported = encode_moc3(doc).unwrap();
    let mut runtime = PurismModelInstance::new(&exported.bytes);
    runtime.update();
    runtime.get_drawable("MeshQuad").unwrap()
}

pub(crate) fn assert_runtime_matches(doc: &Document, bytes: &[u8], samples: &[Vec<f32>]) {
    let re_export = encode_moc3(doc).unwrap();
    let mut original = PurismModelInstance::new(bytes);
    let mut exported = PurismModelInstance::new(&re_export.bytes);
    for sample in samples {
        let mut values = HashMap::new();
        for (id, value) in doc.parameter_order().iter().zip(sample) {
            let parameter = doc.get_parameter(id).unwrap();
            values.insert(id.clone(), *value);
            original.set_parameter(&parameter.runtime_id, *value);
            exported.set_parameter(&parameter.runtime_id, *value);
        }
        original.update();
        exported.update();
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(doc, &values, &mut frame).is_ok());
        for d in frame.drawables {
            for actual in [
                original.get_drawable(&d.runtime_id).unwrap(),
                exported.get_drawable(&d.runtime_id).unwrap(),
            ] {
                assert_eq!(d.indices.as_ref(), actual.indices.as_slice());
                assert_eq!(d.uvs.len(), actual.uvs.len());
                for (a, b) in d.uvs.iter().zip(&actual.uvs) {
                    near(a.x, b.x, 1.0);
                    near(a.y, b.y, 1.0);
                }
                assert_eq!(d.positions.len(), actual.positions.len());
                for (a, b) in d.positions.iter().zip(&actual.positions) {
                    near(
                        a.x / doc.source_canvas().unwrap().pixels_per_unit,
                        b.x,
                        doc.source_canvas().unwrap().pixels_per_unit,
                    );
                    near(
                        a.y / doc.source_canvas().unwrap().pixels_per_unit,
                        b.y,
                        doc.source_canvas().unwrap().pixels_per_unit,
                    );
                }
                near(d.opacity, actual.opacity, 1.0);
                assert_eq!(d.draw_order, actual.draw_order);
                assert_eq!(d.render_order, actual.render_order);
                for c in 0..4 {
                    near(d.multiply_color[c], actual.multiply_color[c], 1.0);
                    near(d.screen_color[c], actual.screen_color[c], 1.0);
                }
            }
        }
    }
}
