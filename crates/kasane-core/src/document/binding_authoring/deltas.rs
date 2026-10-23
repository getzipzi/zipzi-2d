use super::*;
use crate::*;
#[derive(Clone)]
pub(super) struct DeltaVector {
    pub values: Vec<f32>,
    pub colors: u8,
}
impl DeltaVector {
    pub fn zero(n: usize) -> Self {
        Self {
            values: vec![0.; n],
            colors: 0,
        }
    }
}
pub(super) fn pack(forms: &DeltaKeyforms, slot: usize) -> DeltaVector {
    let mut v = Vec::new();
    let mut colors = 0;
    let mut color = |multiply: Option<[f32; 3]>, screen: Option<[f32; 3]>, v: &mut Vec<f32>| {
        if multiply.is_some() {
            colors |= 1;
        }
        if screen.is_some() {
            colors |= 2;
        }
        v.extend(multiply.unwrap_or([0.; 3]));
        v.extend(screen.unwrap_or([0.; 3]));
    };
    match forms {
        DeltaKeyforms::Mesh(f) => {
            let f = &f[slot];
            for p in &f.positions {
                v.extend([p.x, p.y]);
            }
            v.push(f.opacity.unwrap_or(0.));
            v.push(f.draw_order.unwrap_or(0.));
            color(f.multiply, f.screen, &mut v);
        }
        DeltaKeyforms::Warp(f) => {
            let f = &f[slot];
            for p in &f.points {
                v.extend([p.x, p.y]);
            }
            v.push(f.opacity.unwrap_or(0.));
            color(f.multiply, f.screen, &mut v);
        }
        DeltaKeyforms::Rotation(f) => {
            let f = &f[slot];
            let p = f.origin.unwrap_or_default();
            v.extend([
                p.x,
                p.y,
                f.angle.unwrap_or(0.),
                f.scale.unwrap_or(0.),
                f.opacity.unwrap_or(0.),
            ]);
            color(f.multiply, f.screen, &mut v);
        }
        DeltaKeyforms::Part(f) => v.push(f[slot].draw_order),
        DeltaKeyforms::Glue(f) => v.push(f[slot].intensity),
        DeltaKeyforms::Offscreen(f) => {
            let f = &f[slot];
            v.push(f.opacity);
            // Offscreen multiply stores a multiplier with neutral value 1;
            // all conversion math operates on additive deltas with neutral 0.
            color(f.multiply.map(|c| c.map(|v| v - 1.)), f.screen, &mut v);
        }
    }
    DeltaVector { values: v, colors }
}
pub(super) fn unpack(
    kind: BlendShapeTargetKind,
    forms: Vec<(String, DeltaVector)>,
) -> DeltaKeyforms {
    macro_rules! build {
        ($variant:ident,$f:expr) => {
            DeltaKeyforms::$variant(
                forms
                    .into_iter()
                    .map(|(sample_id, v)| {
                        let f = $f;
                        f(sample_id, v)
                    })
                    .collect(),
            )
        };
    }
    let color = |v: &DeltaVector, offset: usize, bit: u8| {
        if v.colors & bit != 0 {
            Some(v.values[offset..offset + 3].try_into().unwrap())
        } else {
            None
        }
    };
    match kind {
        BlendShapeTargetKind::Mesh => build!(Mesh, |sample_id: String, v: DeltaVector| {
            let n = (v.values.len() - 8) / 2;
            DeltaMeshKeyform {
                sample_id,
                positions: v.values[..n * 2]
                    .chunks_exact(2)
                    .map(|p| Vec2::new(p[0], p[1]))
                    .collect(),
                opacity: Some(v.values[n * 2]),
                draw_order: Some(v.values[n * 2 + 1]),
                multiply: color(&v, n * 2 + 2, 1),
                screen: color(&v, n * 2 + 5, 2),
            }
        }),
        BlendShapeTargetKind::Warp => build!(Warp, |sample_id: String, v: DeltaVector| {
            let n = (v.values.len() - 7) / 2;
            DeltaWarpKeyform {
                sample_id,
                points: v.values[..n * 2]
                    .chunks_exact(2)
                    .map(|p| Vec2::new(p[0], p[1]))
                    .collect(),
                opacity: Some(v.values[n * 2]),
                multiply: color(&v, n * 2 + 1, 1),
                screen: color(&v, n * 2 + 4, 2),
            }
        }),
        BlendShapeTargetKind::Rotation => build!(Rotation, |sample_id: String, v: DeltaVector| {
            DeltaRotationKeyform {
                sample_id,
                origin: Some(Vec2::new(v.values[0], v.values[1])),
                angle: Some(v.values[2]),
                scale: Some(v.values[3]),
                opacity: Some(v.values[4]),
                multiply: color(&v, 5, 1),
                screen: color(&v, 8, 2),
            }
        }),
        BlendShapeTargetKind::Part => {
            build!(Part, |sample_id: String, v: DeltaVector| DeltaPartKeyform {
                sample_id,
                draw_order: v.values[0]
            })
        }
        BlendShapeTargetKind::Glue => {
            build!(Glue, |sample_id: String, v: DeltaVector| DeltaGlueKeyform {
                sample_id,
                intensity: v.values[0]
            })
        }
        BlendShapeTargetKind::Offscreen => {
            build!(Offscreen, |sample_id: String, v: DeltaVector| {
                DeltaOffscreenKeyform {
                    sample_id,
                    opacity: v.values[0],
                    multiply: color(&v, 1, 1).map(|c| c.map(|v| v + 1.)),
                    screen: color(&v, 4, 2),
                }
            })
        }
    }
}
pub(super) fn sample(
    table: &BlendShapeKeyTable,
    forms: &DeltaKeyforms,
    value: f32,
) -> Result<DeltaVector, Status> {
    let n = pack(forms, 0).values.len();
    let axis = BindingAxis {
        id: String::new(),
        // Conversion samples source data even while its driver is pending.
        parameter_id: "conversion".into(),
        keys: table.keys.clone(),
        samples: table.samples.clone(),
    };
    let weights = super::lattice::selection(&[axis], &[value]);
    let mut out = DeltaVector::zero(n);
    let mut chosen = None;
    for (i, w) in weights {
        if i == table.base_key_idx {
            continue;
        }
        let v = pack(forms, i);
        if let Some(colors) = chosen {
            if colors != v.colors {
                return Err(Status::error(
                    "OPTIONAL_CHANNEL_NOT_REPRESENTABLE",
                    "Color presence changes across the resampled segment",
                ));
            }
        } else {
            chosen = Some(v.colors);
        }
        for (x, y) in out.values.iter_mut().zip(v.values) {
            *x += y * w;
        }
    }
    out.colors = chosen.unwrap_or(0);
    Ok(out)
}
impl Document {
    pub(super) fn delta_width(
        &self,
        kind: BlendShapeTargetKind,
        target: &str,
    ) -> Result<usize, Status> {
        let missing = || Status::error("MISSING_OBJECT", target);
        Ok(match kind {
            BlendShapeTargetKind::Mesh => {
                self.get_mesh(target).ok_or_else(missing)?.vertex_ids.len() * 2 + 8
            }
            BlendShapeTargetKind::Warp => {
                self.get_transform(target)
                    .and_then(Transform::warp)
                    .ok_or_else(missing)?
                    .points
                    .len()
                    * 2
                    + 7
            }
            BlendShapeTargetKind::Rotation => {
                self.get_transform(target)
                    .and_then(Transform::rotation)
                    .ok_or_else(missing)?;
                11
            }
            BlendShapeTargetKind::Part => {
                self.get_part(target).ok_or_else(missing)?;
                1
            }
            BlendShapeTargetKind::Glue => {
                self.get_glue(target).ok_or_else(missing)?;
                1
            }
            BlendShapeTargetKind::Offscreen => {
                self.get_offscreen(target).ok_or_else(missing)?;
                7
            }
        })
    }
}

/// Ordinary table edits retain each existing authored form, including optional
/// channels and the non-contributing base entry. Only generated/rebased forms
/// need a numeric reconstruction.
pub(super) fn retain_existing(source: &DeltaKeyforms, result: &mut DeltaKeyforms) {
    macro_rules! retain {
        ($source:expr, $result:expr) => {{
            let old: std::collections::HashMap<_, _> =
                $source.iter().map(|f| (f.sample_id.as_str(), f)).collect();
            for f in $result {
                if let Some(previous) = old.get(f.sample_id.as_str()) {
                    *f = (*previous).clone();
                }
            }
        }};
    }
    match (source, result) {
        (DeltaKeyforms::Mesh(a), DeltaKeyforms::Mesh(b)) => retain!(a, b),
        (DeltaKeyforms::Warp(a), DeltaKeyforms::Warp(b)) => retain!(a, b),
        (DeltaKeyforms::Rotation(a), DeltaKeyforms::Rotation(b)) => retain!(a, b),
        (DeltaKeyforms::Part(a), DeltaKeyforms::Part(b)) => retain!(a, b),
        (DeltaKeyforms::Glue(a), DeltaKeyforms::Glue(b)) => retain!(a, b),
        (DeltaKeyforms::Offscreen(a), DeltaKeyforms::Offscreen(b)) => retain!(a, b),
        _ => unreachable!("validated target kind is unchanged"),
    }
}
pub(super) fn remap_ids(source: &DeltaKeyforms, ids: &[BindingSample]) -> DeltaKeyforms {
    let mut result = source.clone();
    macro_rules! remap {
        ($forms:expr) => {
            for (f, s) in $forms.iter_mut().zip(ids) {
                f.sample_id = s.id.clone();
            }
        };
    }
    match &mut result {
        DeltaKeyforms::Mesh(f) => remap!(f),
        DeltaKeyforms::Warp(f) => remap!(f),
        DeltaKeyforms::Rotation(f) => remap!(f),
        DeltaKeyforms::Part(f) => remap!(f),
        DeltaKeyforms::Glue(f) => remap!(f),
        DeltaKeyforms::Offscreen(f) => remap!(f),
    }
    result
}
