//! The stage-two native workflow: frozen input, isolated Draft, one publication,
//! snapshot history and a save projection. Existing files are never overwritten.
use zipzi_sdk::{AuthoringSession, NameTarget, ProjectIo, ResourceStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: native_authoring INPUT.zipzi NEW_OUTPUT.zipzi".into());
    }
    let source = std::path::absolute(&args[0])?;
    let destination = std::path::absolute(&args[1])?;
    if destination.exists() {
        return Err("output must be a new path".into());
    }
    let cache = std::env::temp_dir().join("zipzi-native-authoring");
    let io = ProjectIo::new(cache, ResourceStore::default());
    let status = |s: zipzi_sdk::Status| std::io::Error::other(format!("{}: {}", s.code, s.message));
    let (mut session, diagnostics) =
        AuthoringSession::open(&io, &source, || true).map_err(status)?;
    if let Some(id) = session.snapshot().document().mesh_order().first().cloned() {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |commands| {
                commands.rename(NameTarget::Mesh(&id), "Edited mesh".into())
            })
            .map_err(status)?;
        let prepared = draft.prepare("Rename mesh").map_err(status)?;
        let changed = !prepared.writes().is_empty();
        session.publish(prepared).map_err(status)?;
        if changed {
            session.undo(session.version()).map_err(status)?;
            session.redo(session.version()).map_err(status)?;
        }
    }
    let receipt = session
        .save_as(&io, &destination, None, || true)
        .map_err(status)?;
    println!(
        "Saved {}; {} resource diagnostics; {} durability warnings",
        receipt.path.display(),
        diagnostics.len(),
        receipt.warnings.len()
    );
    Ok(())
}
