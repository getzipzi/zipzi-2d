//! Render a sampled synthetic Motion/Pose frame using the public observation path.
use std::{collections::HashMap, env, fs, path::PathBuf};
use zipzi_core::DrawableFrameBounds;

use zipzi_observe::{
    ModelRoi, ObservationInput, Observer, ObserverConfig, RenderRequest, ResolvedObservation,
};
use zipzi_sdk::{AuthoringSession, Snapshot};

const MOTION: &str = "00000000-0000-4000-8000-00000000f902";
const POSE: &str = "00000000-0000-4000-8000-00000000f903";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("expected MOC3 texture motion3 pose3 output.png time".into());
    }
    let importer = zipzi_project::ProjectImporter::default();
    let (result, imported, _) = importer.import_bare_moc3(
        &PathBuf::from(&args[0]),
        &HashMap::from([(0, PathBuf::from(&args[1]))]),
    );
    if !result.status.is_ok() {
        return Err(format!("{}: {}", result.status.code, result.status.message).into());
    }
    let imported = imported.ok_or("import returned no model")?;
    let motion = zipzi_project::import_motion3(
        &imported.document,
        MOTION,
        "Motion",
        &fs::read_to_string(&args[2])?,
    )?;
    let pose =
        zipzi_project::import_pose3(&motion.candidate, POSE, &fs::read_to_string(&args[3])?)?;
    if !motion.diagnostics.is_empty() || !pose.diagnostics.is_empty() {
        return Err(format!(
            "Unresolved animation targets: {:?}, {:?}",
            motion.diagnostics, pose.diagnostics
        )
        .into());
    }
    let session = AuthoringSession::new(
        Snapshot::new(pose.candidate, imported.resources)
            .map_err(|e| format!("{}: {}", e.code, e.message))?,
    );
    let source = session.read_snapshot();
    let mut preview = zipzi_animation::MotionPreview::new(source.snapshot.document());
    preview.bind_source(source.source);
    preview.schedule_motion(MOTION, 0.0)?;
    preview.seek(args[5].parse()?)?;
    let input = ObservationInput::capture_motion_from_snapshot(&session.read_snapshot(), &preview)?;
    let captured = ResolvedObservation::capture_many_with_control(
        vec![input],
        &zipzi_sdk::ConversionControl::default(),
    )?
    .remove(0);
    // The probe chooses its own square view to match Framework's fitted view.
    let frame = preview.evaluate_drawables_including_hidden()?;
    let [x0, y0, x1, y1] = frame.model_bounds(false).unwrap_or([0.; 4]);
    let side = (x1 - x0).max(y1 - y0).max(1.) as f32;
    let center = (((x0 + x1) * 0.5) as f32, ((y0 + y1) * 0.5) as f32);
    let request = RenderRequest {
        width: 128,
        height: 128,
        roi: ModelRoi {
            x0: center.0 - side * 0.5,
            y0: center.1 - side * 0.5,
            x1: center.0 + side * 0.5,
            y1: center.1 + side * 0.5,
        },
        padding_model: 0.,
        topology: None,
        quality: false,
    };
    let mut observer = Observer::new(ObserverConfig {
        width: 128,
        height: 128,
    })?;
    fs::write(&args[4], observer.render(&captured, request)?.png_bytes()?)?;
    let snapshot = preview.snapshot();
    for (index, id) in session
        .snapshot()
        .document()
        .parameter_order()
        .iter()
        .enumerate()
    {
        let runtime_id = &session
            .snapshot()
            .document()
            .parameter(id)
            .ok_or("missing parameter")?
            .runtime_id;
        println!(
            "PARAM {index} {runtime_id} {}",
            snapshot
                .parameters
                .get(id)
                .ok_or("missing parameter value")?
        );
    }
    for (index, id) in session
        .snapshot()
        .document()
        .part_order()
        .iter()
        .enumerate()
    {
        let runtime_id = &session
            .snapshot()
            .document()
            .part(id)
            .ok_or("missing part")?
            .runtime_id;
        println!(
            "PART {index} {runtime_id} {}",
            snapshot
                .part_opacities
                .get(id)
                .ok_or("missing part opacity")?
        );
    }
    println!("MODEL_OPACITY {}", snapshot.model_opacity);
    Ok(())
}
