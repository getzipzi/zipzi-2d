//! Missing optional assets skip a run; explicitly requested assets must exist.
use anyhow::{Result, ensure};
use std::path::PathBuf;

pub(super) fn available(
    explicit: Option<Vec<PathBuf>>,
    defaults: &[PathBuf],
    required: bool,
) -> Result<Vec<PathBuf>> {
    if let Some(paths) = explicit {
        ensure!(!paths.is_empty(), "No model paths supplied");
        for path in &paths {
            ensure!(
                path.is_file(),
                "Model file is unavailable: {}",
                path.display()
            );
        }
        return Ok(paths);
    }
    let paths: Vec<_> = defaults
        .iter()
        .filter(|path| path.is_file())
        .cloned()
        .collect();
    ensure!(
        !required || !paths.is_empty(),
        "Required licensed models are unavailable"
    );
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_assets_are_all_required_even_when_other_models_exist() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let present = dir.path().join("present.model3.json");
        let missing = dir.path().join("missing.model3.json");
        std::fs::write(&present, "{}")?;
        assert!(available(Some(vec![missing.clone()]), &[present.clone()], false).is_err());
        assert!(available(Some(vec![present.clone(), missing]), &[], false).is_err());
        assert_eq!(
            available(Some(vec![present.clone()]), &[], false)?,
            [present]
        );
        Ok(())
    }

    #[test]
    fn discovery_distinguishes_optional_absence_from_required_assets() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = [dir.path().join("model.model3.json")];
        assert!(available(None, &paths, false)?.is_empty());
        assert!(available(None, &paths, true).is_err());
        std::fs::write(&paths[0], "{}")?;
        assert_eq!(available(None, &paths, true)?, paths);
        Ok(())
    }
}
