use anyhow::Context as _;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use zipzi_core::document::MotionPoint;
use zipzi_sdk::{CompiledCurve, CompiledSegment, sample_motion_curve};

fn numeric(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn curve(v: &Value) -> CompiledCurve {
    let numbers = v["Segments"].as_array().unwrap();
    let point_at = |i| MotionPoint {
        time: numeric(&numbers[i]),
        value: numeric(&numbers[i + 1]),
    };
    let mut segments = Vec::new();
    let mut i = 2;
    while i < numbers.len() {
        let kind = numbers[i].as_u64().unwrap();
        segments.push(match kind {
            0 => CompiledSegment::Linear {
                end: point_at(i + 1),
            },
            1 => CompiledSegment::Bezier {
                control1: point_at(i + 1),
                control2: point_at(i + 3),
                end: point_at(i + 5),
            },
            2 => CompiledSegment::Stepped {
                end: point_at(i + 1),
            },
            3 => CompiledSegment::InverseStepped {
                end: point_at(i + 1),
            },
            _ => panic!("unexpected segment kind"),
        });
        i += if kind == 1 { 7 } else { 3 };
    }
    CompiledCurve {
        initial: point_at(0),
        segments: Arc::new(segments),
    }
}

fn recount(motion: &mut Value) {
    let curves = motion["Curves"].as_array().unwrap();
    let compiled: Vec<_> = curves.iter().map(curve).collect();
    let count = compiled.iter().map(|c| c.segments.len()).sum::<usize>();
    let points = compiled
        .iter()
        .map(|c| {
            1 + c
                .segments
                .iter()
                .map(|s| {
                    if matches!(s, CompiledSegment::Bezier { .. }) {
                        3
                    } else {
                        1
                    }
                })
                .sum::<usize>()
        })
        .sum::<usize>();
    motion["Meta"]["CurveCount"] = json!(curves.len());
    motion["Meta"]["TotalSegmentCount"] = json!(count);
    motion["Meta"]["TotalPointCount"] = json!(points);
}

pub(crate) fn variants(source: &Path) -> anyhow::Result<(tempfile::TempDir, PathBuf, Value)> {
    let model: Value = serde_json::from_slice(&std::fs::read(source)?)?;
    let base = source.parent().unwrap();
    let motions = model["FileReferences"]["Motions"]
        .as_object()
        .context("model has no motions")?;
    let mut candidates = Vec::new();
    for entries in motions.values() {
        for entry in entries.as_array().unwrap() {
            let path = base.join(entry["File"].as_str().unwrap());
            let motion: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            candidates.push((
                motion["Meta"]["TotalSegmentCount"].as_u64().unwrap_or(0),
                path,
                motion,
            ));
        }
    }
    let (_, motion_path, original) = candidates
        .into_iter()
        .max_by_key(|(n, _, _)| *n)
        .context("no motion files")?;
    let temp = tempfile::tempdir()?;
    // The importer deliberately rejects symlinks escaping the model directory.
    // Copy into a disposable directory; source assets and tracked fixtures stay untouched.
    fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let destination = target.join(entry.file_name());
            if path.is_dir() {
                copy_tree(&path, &destination)?;
            } else {
                std::fs::copy(path, destination)?;
            }
        }
        Ok(())
    }
    copy_tree(base, temp.path())?;
    let mut natural = original.clone();
    let first = natural["Curves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["Target"] == "Parameter")
        .context("no parameter curves")?
        .clone();
    let mut unresolved = first.clone();
    unresolved["Id"] = json!("TimelineMissingParameter");
    natural["Curves"]
        .as_array_mut()
        .unwrap()
        .extend([first, unresolved]);
    recount(&mut natural);
    let mut dense = natural.clone();
    let duration = numeric(&dense["Meta"]["Duration"]);
    let restricted = dense["Meta"]["AreBeziersRestricted"].as_bool().unwrap();
    for c in dense["Curves"].as_array_mut().unwrap() {
        let raw = curve(c);
        let end = raw
            .segments
            .last()
            .map_or(duration, |s| s.end().time)
            .max(duration);
        let mut numbers = vec![json!(0.), json!(sample_motion_curve(&raw, 0., restricted))];
        for i in 1..=256 {
            let time = end * i as f32 / 256.;
            numbers.extend([
                json!(0),
                json!(time),
                json!(sample_motion_curve(&raw, time, restricted)),
            ]);
        }
        c["Segments"] = json!(numbers);
    }
    recount(&mut dense);
    let mut opaque = original.clone();
    opaque["TimelineVendorExtension"] = json!({"version":7,"payload":["preserve",0.1137]});
    let mut model = model;
    let mut registrations = serde_json::Map::new();
    for (name, motion) in [("Natural", natural), ("Dense", dense), ("Opaque", opaque)] {
        let file = format!("timeline-{name}.motion3.json");
        std::fs::write(temp.path().join(&file), serde_json::to_vec(&motion)?)?;
        registrations.insert(format!("Timeline{name}"), json!([{"File":file}]));
    }
    model["FileReferences"]["Motions"] = Value::Object(registrations);
    let path = temp.path().join("timeline-acceptance.model3.json");
    std::fs::write(&path, serde_json::to_vec(&model)?)?;
    Ok((
        temp,
        path,
        json!({"model":source,"motion":motion_path,"original_curves":original["Meta"]["CurveCount"],"original_segments":original["Meta"]["TotalSegmentCount"],"dense_points_per_track":257}),
    ))
}
