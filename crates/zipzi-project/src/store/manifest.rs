use crate::filesystem as io;
use std::path::{Path, PathBuf};
use zipzi_core::Status;
/// One host's identity for an existing archive or a not-yet-created file.
/// The extension is a UI convention; explicit SDK paths are preserved.
pub fn project_identity(path: &Path) -> Result<PathBuf, Status> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io::io_error)?.join(path)
    };
    io::reject_symlink(&absolute)?;
    let resolved = io::local_path(&absolute)?;
    if std::fs::metadata(&resolved).is_ok_and(|m| !m.is_file()) {
        return Err(Status::error(
            "UNSUPPORTED_PROJECT_PATH",
            "Expected a project file; directory projects are unsupported",
        ));
    }
    Ok(resolved)
}
