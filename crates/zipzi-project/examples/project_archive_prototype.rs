//! P0 experiment; all outputs must be disposable benchmark artifacts.
//! `fixtures OUTPUT`, `pack DIRECTORY OUTPUT.zipzi`, or
//! `bench directory|archive open|save|save-as INPUT OUTPUT REPEATS`.
mod archive_fixtures;

use std::fs::{self, File};
use std::io::{BufWriter, Seek};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::json;
use zipzi_core::Document;
use zipzi_project::{
    archive::{
        open_archive, write_archive, MeteredIo, OpenedArchive, ProjectFingerprint, PrototypeLimits,
        WrittenArchive,
    },
    DocumentStore,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn status(status: zipzi_core::Status) -> Result<()> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(format!("{}: {}", status.code, status.message).into())
    }
}

fn fingerprint(path: &Path) -> Result<ProjectFingerprint> {
    Ok(ProjectFingerprint::read(
        &mut File::open(path)?,
        PrototypeLimits::default().archive_bytes,
        || true,
    )?)
}

fn pack(
    document: &Document,
    root: &Path,
    target: &Path,
) -> Result<(WrittenArchive, serde_json::Value)> {
    // A private stage in the target filesystem; never create a whole ZIP Vec.
    let parent = target.parent().ok_or("Missing output parent")?;
    let stage = tempfile::NamedTempFile::new_in(parent)?;
    let mut output = MeteredIo::new(stage.reopen()?);
    let result = {
        let mut buffered = BufWriter::with_capacity(1 << 20, &mut output);
        let result = write_archive(
            document,
            root,
            &mut buffered,
            PrototypeLimits::default(),
            || true,
        )?;
        std::io::Write::flush(&mut buffered)?;
        result
    };
    output.inner().sync_all()?;
    output.rewind()?;
    let hash = ProjectFingerprint::read(
        &mut output,
        PrototypeLimits::default().archive_bytes,
        || true,
    )?;
    let counts = output.counts();
    drop(output);
    // persist_noclobber performs atomic no-replace publication. This helper is
    // only used on fresh disposable outputs; production replacement is P1.
    stage.persist_noclobber(target)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    let report = json!({
        "fingerprint": hash, "logical_io": {
            "read_bytes": result.resource_read_bytes + counts.read_bytes,
            "write_bytes": counts.write_bytes,
        },
        "document_bytes": result.document_bytes, "asset_bytes": result.asset_bytes,
        "unique_assets": result.unique_assets,
    });
    Ok((result, report))
}

fn normalized_equivalent(before: &Document, after: &Document) -> bool {
    let mut expected = before.fork_candidate();
    for id in before.asset_order() {
        let mut asset = before.get_asset(id).unwrap().clone();
        if let Some(content) = asset.content.as_mut() {
            let Some(saved) = after.get_asset(id).and_then(|a| a.content.as_ref()) else {
                return false;
            };
            content.source = saved.source.clone();
            content.sha256 = saved.sha256.clone();
        }
        if !expected.replace_asset(asset).status.is_ok() {
            return false;
        }
    }
    expected.same_content(after)
}

fn verify(document: &Document, root: &Path, reopened: &OpenedArchive) -> Result<()> {
    if !normalized_equivalent(document, &reopened.document) {
        return Err("Authoring content differs".into());
    }
    // Compare original and extracted bytes directly, without PNG re-encoding.
    for id in document.asset_order() {
        let original = document.get_asset(id).unwrap();
        let Some(content) = &original.content else {
            continue;
        };
        let source = Path::new(&content.source);
        let source = if source.is_absolute() {
            source.to_owned()
        } else {
            root.join(source)
        };
        let target = &reopened
            .document
            .get_asset(id)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source;
        let mut a = File::open(source)?;
        let mut b = File::open(reopened.asset_root().join(target))?;
        let mut left = vec![0; 1 << 20];
        let mut right = vec![0; 1 << 20];
        loop {
            use std::io::Read;
            left.clear();
            right.clear();
            // Fill bounded chunks so short reads cannot change comparison.
            let n = (&mut a).take(1 << 20).read_to_end(&mut left)?;
            (&mut b).take(1 << 20).read_to_end(&mut right)?;
            if left != right {
                return Err("Resource bytes differ".into());
            }
            if n == 0 {
                break;
            }
        }
    }
    Ok(())
}

fn directory_open(store: &DocumentStore, input: &Path) -> Result<zipzi_project::DocumentSnapshot> {
    let (result, snapshot) = store.open(input);
    status(result.status)?;
    // content: None is intentionally representable. A declared but broken
    // content reference is a fixture error and must not be silently dropped.
    if result.diagnostics.iter().any(|d| {
        snapshot.as_ref().is_none_or(|s| {
            s.document
                .get_asset(&d.asset_id)
                .is_none_or(|a| a.content.is_some())
        })
    }) {
        return Err(format!("Resource diagnostics: {:?}", result.diagnostics).into());
    }
    Ok(snapshot.ok_or("No snapshot")?)
}

fn bench(format: &str, operation: &str, input: &Path, output: &Path, repeats: usize) -> Result<()> {
    if !matches!(format, "directory" | "archive")
        || !matches!(operation, "open" | "save" | "save-as")
        || repeats == 0
    {
        return Err("Invalid benchmark format/operation/repeats".into());
    }
    fs::create_dir(output)?;
    let store = DocumentStore::new();
    let scratch = tempfile::tempdir_in(output)?;
    let mut times = Vec::new();
    let mut observations = Vec::new();
    if operation == "open" {
        for _ in 0..repeats {
            let start = Instant::now();
            if format == "archive" {
                let opened =
                    open_archive(input, scratch.path(), PrototypeLimits::default(), || true)?;
                times.push(start.elapsed().as_secs_f64() * 1000.);
                observations.push(json!({"logical_io": opened.counts, "peak_working_bytes": opened.peak_working_bytes}));
                drop(opened);
            } else {
                let opened = directory_open(&store, input)?;
                times.push(start.elapsed().as_secs_f64() * 1000.);
                observations.push(json!({"logical_io": null, "peak_working_bytes": 0}));
                drop(opened);
            }
        }
    } else if format == "archive" {
        let opened = open_archive(input, scratch.path(), PrototypeLimits::default(), || true)?;
        let target = output.join("saved.zipzi");
        if operation == "save" {
            fs::copy(input, &target)?;
        }
        for index in 0..repeats {
            let destination = if operation == "save" {
                target.clone()
            } else {
                output.join(format!("copy-{index}.zipzi"))
            };
            let start = Instant::now();
            let existing = if operation == "save" {
                Some(fingerprint(&destination)?)
            } else {
                None
            };
            let stage_target = output.join(format!("staging-{index}.zipzi"));
            let (_, mut report) = pack(&opened.document, opened.asset_root(), &stage_target)?;
            if let Some(expected) = &existing {
                if fingerprint(&destination)? != *expected {
                    return Err("Benchmark destination changed".into());
                }
                fs::rename(&stage_target, &destination)?;
            } else {
                // Both paths belong exclusively to this benchmark invocation.
                fs::hard_link(&stage_target, &destination)?;
                fs::remove_file(&stage_target)?;
            }
            #[cfg(unix)]
            File::open(output)?.sync_all()?;
            times.push(start.elapsed().as_secs_f64() * 1000.);
            let old_bytes = existing.as_ref().map_or(0, |f| f.length);
            let written = fs::metadata(&destination)?.len();
            if let Some(io) = report["logical_io"]["read_bytes"].as_u64() {
                report["logical_io"]["read_bytes"] = json!(io + old_bytes * 2);
            }
            report["peak_working_bytes"] = json!(opened.asset_bytes + old_bytes + written);
            observations.push(report);
            // Validation is outside the timed publication interval.
            let saved = open_archive(
                &destination,
                scratch.path(),
                PrototypeLimits::default(),
                || true,
            )?;
            verify(&opened.document, opened.asset_root(), &saved)?;
            if operation == "save-as" {
                fs::remove_file(destination)?;
            }
        }
    } else {
        let mut opened = directory_open(&store, input)?;
        let mut source = opened.manifest.parent().unwrap().to_owned();
        let target = output.join("saved-directory");
        let mut expected = None;
        if operation == "save" {
            let (result, snapshot) = store.save(&opened.document, &source, &target, None);
            status(result.status)?;
            opened = snapshot.ok_or("No save snapshot")?;
            source = target.clone();
            expected = Some(opened.manifest_sha256.clone());
        }
        for index in 0..repeats {
            let destination = if operation == "save" {
                target.clone()
            } else {
                output.join(format!("copy-{index}"))
            };
            let start = Instant::now();
            let (result, snapshot) =
                store.save(&opened.document, &source, &destination, expected.as_deref());
            status(result.status)?;
            times.push(start.elapsed().as_secs_f64() * 1000.);
            let saved = snapshot.ok_or("No save snapshot")?;
            let reopened = directory_open(&store, &destination)?;
            if !saved.document.same_content(&reopened.document) {
                return Err("Directory roundtrip differs".into());
            }
            observations.push(json!({"logical_io": null, "peak_working_bytes": null}));
            if operation == "save" {
                expected = Some(saved.manifest_sha256.clone());
            } else {
                fs::remove_dir_all(destination)?;
            }
        }
    }
    println!(
        "{}",
        json!({
            "format": format, "operation": operation, "repeats": repeats,
            "milliseconds": times, "observations": observations,
            "roundtrip_equal": operation != "open",
            "experimental_feature": "prototype-directory-document-v21",
        })
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    match args.get(1).and_then(|a| a.to_str()) {
        Some("fixtures") if args.len() == 3 => archive_fixtures::create(Path::new(&args[2])),
        Some("pack") if args.len() == 4 => {
            let root = PathBuf::from(&args[2]);
            let opened = directory_open(&DocumentStore::new(), &root)?;
            let (written, report) = pack(&opened.document, &root, Path::new(&args[3]))?;
            let scratch = tempfile::tempdir()?;
            let reopened = open_archive(Path::new(&args[3]), scratch.path(), PrototypeLimits::default(), || true)?;
            verify(&opened.document, &root, &reopened)?;
            assert!(written.document.same_content(&reopened.document));
            println!("{report}");
            Ok(())
        }
        Some("bench") if args.len() == 7 => bench(
            args[2].to_str().ok_or("Invalid format")?, args[3].to_str().ok_or("Invalid operation")?,
            Path::new(&args[4]), Path::new(&args[5]), args[6].to_str().ok_or("Invalid repeats")?.parse()?,
        ),
        _ => Err("Usage: project_archive_prototype fixtures OUTPUT | pack DIRECTORY OUTPUT.zipzi | bench directory|archive open|save|save-as INPUT OUTPUT REPEATS".into()),
    }
}
