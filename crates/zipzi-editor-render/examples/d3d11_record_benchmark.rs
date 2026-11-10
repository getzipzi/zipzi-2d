//! CPU recording probe. Model evaluation, upload and GPU drain are outside timing.
#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    use std::time::Instant;
    use zipzi_core::Vec2;
    use zipzi_editor_core::{
        Editor, EditorCommand, LoadRequest, ViewportId, ViewportKind, ViewportMetrics,
        ViewportTransform,
    };
    use zipzi_render::{Affine2, ViewportConfig, texture::straight_rgba_mipmaps};
    use zipzi_render_d3d11::{
        D3d11Context, D3d11Renderer, D3d11Target, D3d11TextureCatalog, upload_rgba8,
    };

    let mut args = std::env::args_os().skip(1);
    let model = std::path::PathBuf::from(args.next().context("model path")?).canonicalize()?;
    let samples: usize = args
        .next()
        .map(|v| v.to_string_lossy().parse())
        .transpose()?
        .unwrap_or(240);
    anyhow::ensure!(samples > 0, "samples must be positive");
    let extent = [1061, 1478];
    let metrics = ViewportMetrics {
        width: extent[0] as f32,
        height: extent[1] as f32,
        scale_factor: 1.,
    };
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    editor.set_canvas_metrics(metrics);
    let request = if model.extension().is_some_and(|ext| ext == "zipzi") {
        LoadRequest::Open(model)
    } else {
        LoadRequest::Model3(model)
    };
    let load = editor.prepare_load(request)?.context("load task")?;
    editor.finish_load(load.run())?;
    let document = editor.active_id().context("document")?;
    let parameters = editor.document().context("document")?.parameters();
    let parameter = parameters
        .iter()
        .find(|p| p.name == "Angle_X")
        .context("Angle_X")?;
    let preview = editor.preview()?;
    let transform = ViewportTransform::new(metrics, preview.view).context("transform")?;
    let scale = transform.scale;
    let view = ViewportConfig {
        transform: Affine2 {
            a: Vec2::new(scale, 0.),
            b: Vec2::new(0., -scale),
            origin: Vec2::new(transform.offset.x, transform.offset.y),
        },
        target_extent: Vec2::new(metrics.width, metrics.height),
        mask_scale: f64::from(scale),
    };
    let mut context = D3d11Context::new()?;
    let mut renderer = D3d11Renderer::new(context.device().clone())?;
    let output = D3d11Target::new(context.device(), extent)?;
    let sources = preview
        .scene
        .textures
        .iter()
        .map(|source| {
            let mips = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
            upload_rgba8(
                context.device(),
                source.width,
                source.height,
                &source.rgba,
                &mips,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let catalog = D3d11TextureCatalog::new(
        preview
            .scene
            .textures
            .iter()
            .zip(&sources)
            .map(|(source, texture)| (source.asset_id.as_str(), texture)),
    );
    let mut frames = Vec::new();
    for i in 0..120 {
        let fraction = (i as f32 / 119. * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        editor.execute_for(
            document,
            EditorCommand::SetPreviewParameters {
                values: vec![(
                    parameter.id.clone(),
                    parameter.minimum + fraction * (parameter.maximum - parameter.minimum),
                )],
            },
        )?;
        frames.push(editor.preview()?.scene.render_frame()?);
    }
    eprintln!(
        "adapter={}, extent={extent:?}, samples={samples}, poses={}",
        context.name(),
        frames.len()
    );
    println!(
        "sample,plan_ms,geometry_ms,layout_ms,targets_ms,encode_ms,finish_ms,total_ms,submit_ms"
    );
    for i in 0..samples + 30 {
        let mut times = [0.; 6];
        let begin = Instant::now();
        let commands = renderer.record(
            &output,
            frames[i % frames.len()].clone(),
            view,
            &catalog,
            |stage| {
                let slot = match stage {
                    "native_d3d11_plan_ready" => 0,
                    "model_synced" => 1,
                    "native_d3d11_layout_ready" => 2,
                    "native_d3d11_targets_ready" => 3,
                    "native_d3d11_encoded" => 4,
                    _ => return,
                };
                times[slot] = begin.elapsed().as_secs_f64() * 1000.;
            },
        )?;
        times[5] = begin.elapsed().as_secs_f64() * 1000.;
        let submit = Instant::now();
        context.submit(&commands)?;
        let submit_ms = submit.elapsed().as_secs_f64() * 1000.;
        // Drain outside timing to avoid measuring an unbounded GPU backlog.
        let pixels = context.read_rgba8(&output)?;
        anyhow::ensure!(pixels.chunks_exact(4).any(|p| p[3] != 0), "empty frame");
        if i >= 30 {
            let mut previous = 0.;
            print!("{}", i - 30);
            for time in times {
                print!(",{:.6}", time - previous);
                previous = time;
            }
            println!(",{:.6},{submit_ms:.6}", times[5]);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("This probe requires Windows and D3D11")
}
