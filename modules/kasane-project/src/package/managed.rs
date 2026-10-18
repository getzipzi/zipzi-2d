//! Ownership of obsolete output files comes from the previous publication,
//! never from filename extensions or the next export's contents alone.
use super::{validate_package_paths, ExportFile};
use crate::filesystem as io;
use kasane_core::Status;
use std::{collections::BTreeMap, fs, path::Path};

pub(super) fn check(
    destination: &Path,
    current: &BTreeMap<String, ExportFile>,
) -> Result<(), Status> {
    let report_path = destination.join("export-report.json");
    io::reject_symlink(&report_path)?;
    let previous: BTreeMap<String, String> = match fs::read(&report_path) {
        Ok(bytes) => {
            let report: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| Status::error("UNMANAGED_OUTPUT", e.to_string()))?;
            report
                .get("managed_files")
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
                .map_err(|e| Status::error("UNMANAGED_OUTPUT", e.to_string()))?
                .unwrap_or_default()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(error) => return Err(io::io_error(error)),
    };
    // Treat the old manifest as untrusted paths; never traverse outside the output.
    validate_package_paths(previous.keys().map(String::as_str))?;
    let allowed: Vec<_> = current
        .keys()
        .chain(previous.keys())
        .map(String::as_str)
        .collect();
    fn visit(
        root: &Path,
        directory: &Path,
        current: &BTreeMap<String, ExportFile>,
        previous: &BTreeMap<String, String>,
        allowed: &[&str],
    ) -> Result<(), Status> {
        for entry in fs::read_dir(directory).map_err(io::io_error)? {
            let entry = entry.map_err(io::io_error)?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Status::error("UNMANAGED_OUTPUT", path.display().to_string()))?
                .join("/");
            let kind = entry.file_type().map_err(io::io_error)?;
            if kind.is_dir() {
                if relative != "textures"
                    && !allowed
                        .iter()
                        .any(|name| name.starts_with(&format!("{relative}/")))
                {
                    return Err(Status::error("UNMANAGED_OUTPUT", relative));
                }
                visit(root, &path, current, previous, allowed)?;
            } else if !kind.is_file() {
                return Err(Status::error("UNMANAGED_OUTPUT", relative));
            } else if relative != "export-report.json" && !current.contains_key(&relative) {
                let hash = previous
                    .get(&relative)
                    .ok_or_else(|| Status::error("UNMANAGED_OUTPUT", &relative))?;
                // A removed asset that was edited independently is no longer safe to delete.
                if crate::content_sha256(&fs::read(&path).map_err(io::io_error)?) != *hash {
                    return Err(Status::error("UNMANAGED_OUTPUT", relative));
                }
            }
        }
        Ok(())
    }
    visit(destination, destination, current, &previous, &allowed)
}
