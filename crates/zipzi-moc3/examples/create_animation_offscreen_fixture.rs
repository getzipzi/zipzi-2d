//! Add a static Offscreen to the synthetic animation model for GPU reference.
use std::{collections::HashMap, env, fs};

use zipzi_core::{Offscreen, OffscreenKeyform};
use zipzi_moc3::encode_moc3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let source = args.next().ok_or("source MOC3 path required")?;
    let output = args.next().ok_or("output MOC3 path required")?;
    let texture = args.next().ok_or("texture PNG path required")?;
    let nested = match args.next().as_deref() {
        None => false,
        Some("--nested") => true,
        Some(_) => return Err("expected optional --nested flag".into()),
    };
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let (result, imported, _) = zipzi_project::ProjectImporter::default().import_bare_moc3(
        std::path::Path::new(&source),
        &HashMap::from([(0, std::path::PathBuf::from(texture))]),
    );
    if !result.status.is_ok() || !result.resources_complete() {
        return Err(format!("{result:?}").into());
    }
    let imported = imported.ok_or("missing imported model")?;
    let mut document = imported.document.builder();
    let owner = document
        .part_order()
        .first()
        .ok_or("fixture has no Part")?
        .clone();
    if nested {
        let child_id = document
            .part_order()
            .get(1)
            .ok_or("fixture has no second Part")?;
        let mut child = document
            .part(child_id)
            .ok_or("missing second Part")?
            .clone();
        child.parent_id = owner.clone();
        let result = document.replace_part(child);
        result.map_err(|status| format!("{}: {}", status.code, status.message))?;
    }
    let result = document.create_offscreen(Offscreen {
        id: "00000000-0000-4000-8000-00000000f501".into(),
        runtime_id: "Offscreen0".into(),
        name: "Offscreen 0".into(),
        part_id: owner,
        keyforms: vec![OffscreenKeyform {
            opacity: 0.5,
            multiply: Some([1.0, 1.0, 1.0]),
            screen: Some([0.0, 0.0, 0.0]),
        }],
        ..Default::default()
    });
    result.map_err(|status| format!("{}: {}", status.code, status.message))?;
    if nested {
        let child_id = document
            .part_order()
            .get(1)
            .ok_or("fixture has no second Part")?
            .clone();
        let result = document.create_offscreen(Offscreen {
            id: "00000000-0000-4000-8000-00000000f502".into(),
            runtime_id: "Offscreen1".into(),
            name: "Offscreen 1".into(),
            part_id: child_id,
            keyforms: vec![OffscreenKeyform {
                opacity: 0.7,
                multiply: Some([1.0, 1.0, 1.0]),
                screen: Some([0.0, 0.0, 0.0]),
            }],
            ..Default::default()
        });
        result.map_err(|status| format!("{}: {}", status.code, status.message))?;
    }
    let document = document
        .freeze()
        .map_err(|status| format!("{}: {}", status.code, status.message))?
        .state;
    fs::write(
        output,
        encode_moc3(&document)
            .map_err(|status| format!("{}: {}", status.code, status.message))?
            .bytes,
    )?;
    Ok(())
}
