//! Kasane → linked CMO3/CAN3 package, reusable by the future CLI.
use kasane_project::{encode_project_cubism, publish_cubism, CubismOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: project_cubism PROJECT OUTPUT_DIRECTORY [BASENAME]";
    let source = std::path::PathBuf::from(args.next().ok_or(usage)?);
    let destination = std::path::PathBuf::from(args.next().ok_or(usage)?);
    let mut options = CubismOptions::default();
    if let Some(name) = args.next() {
        options.base_name = name.into_string().map_err(|_| "basename must be UTF-8")?;
    }
    if args.next().is_some() {
        return Err(usage.into());
    }
    let artifact = encode_project_cubism(&source, &options)?;
    for diagnostic in &artifact.report.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    let publication = publish_cubism(&artifact, &destination)?;
    for warning in publication.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
