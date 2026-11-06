//! Foreign formats prepare immutable content; only the Runtime owner installs it.
use super::*;
use zipzi_contract::kernel_wire::ImportFormat;

#[derive(Debug)]
pub(crate) struct PreparedImport {
    pub snapshot: Snapshot,
    pub protected_paths: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub diagnostics: Vec<zipzi_project::ResourceDiagnostic>,
    pub name: Option<String>,
}

pub(crate) fn prepare(
    importer: &zipzi_project::ProjectImporter,
    path: &Path,
    format: ImportFormat,
) -> Result<PreparedImport> {
    let path = std::fs::canonicalize(path)
        .map_err(|e| zipzi_core::Status::error("IMPORT_IO", e.to_string()))?;
    let path = if matches!(format, ImportFormat::Model3) {
        zipzi_project::resolve_model3_entry(&path)?
    } else {
        path
    };
    let name = path.file_stem().map(|name| {
        name.to_string_lossy()
            .trim_end_matches(".model3")
            .to_owned()
    });
    let (imported, warnings, diagnostics) = match format {
        ImportFormat::Model3 => {
            let (result, imported, _) = importer.import_model3(&path);
            if !result.status.is_ok() {
                return Err(result.status);
            }
            (
                imported
                    .ok_or_else(|| error("IMPORT_FAILED", "Model3 importer returned no content"))?,
                result.warnings,
                result.diagnostics,
            )
        }
        ImportFormat::Psd => {
            let (imported, report) = importer.import_psd(&path)?;
            (imported, report.warnings, vec![])
        }
        ImportFormat::Cubism => (importer.import_cubism_authoring(&path)?, vec![], vec![]),
    };
    let mut protected_paths = imported.protected_paths;
    protected_paths.push(path);
    protected_paths.sort();
    protected_paths.dedup();
    Ok(PreparedImport {
        snapshot: Snapshot::new(imported.document, imported.resources)?,
        protected_paths,
        warnings,
        diagnostics,
        name,
    })
}

impl Runtime {
    pub fn request_import(
        &mut self,
        client: ClientId,
        expected: Version,
        path: &Path,
        format: ImportFormat,
        budget: TaskBudget,
    ) -> Result<JobId> {
        self.source(client, expected)?;
        let leases = self.admit_job(budget)?;
        Ok(self.enqueue(
            client,
            QuerySource::Published(expected),
            Task::Import {
                importer: zipzi_project::ProjectImporter {
                    resources: self.resources.clone(),
                },
                path: path.to_owned(),
                format,
            },
            leases,
            None,
        ))
    }
}
