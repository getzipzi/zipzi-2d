//! Library-level Kasane → CMO3 usage; the future CLI can reuse these APIs.
use kasane_project::{encode_project_cmo3, publish_cmo3, Cmo3Options};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = std::path::PathBuf::from(
        args.next()
            .ok_or("usage: project_cmo3 PROJECT OUTPUT.cmo3")?,
    );
    let destination = std::path::PathBuf::from(
        args.next()
            .ok_or("usage: project_cmo3 PROJECT OUTPUT.cmo3")?,
    );
    if args.next().is_some() {
        return Err("usage: project_cmo3 PROJECT OUTPUT.cmo3".into());
    }
    let artifact = encode_project_cmo3(&source, &Cmo3Options::default())?;
    for diagnostic in &artifact.report.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    let publication = publish_cmo3(&artifact, &destination)?;
    for warning in publication.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
