//! Explorer loads a content-addressed copy outside Velopack's replaceable `current`.
use sha2::{Digest, Sha256};
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

fn installed_launcher(root: &Path) -> anyhow::Result<Option<PathBuf>> {
    if !root.join("Update.exe").is_file() {
        return Ok(None);
    }
    // Velopack names the stable execution stub after --packTitle, not --mainExe.
    let launcher = root.join("Zipzi Editor.exe");
    anyhow::ensure!(
        launcher.is_file(),
        "Missing installed Editor launcher: {}",
        launcher.display()
    );
    Ok(Some(launcher))
}

pub(super) fn register(uninstall: bool) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let Some(current) = exe
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "current"))
    else {
        return Ok(());
    };
    let root = current
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing installation directory"))?;
    let Some(launcher) = installed_launcher(root)? else {
        return Ok(());
    };
    let source = current.join("ZipziProjectPreview.dll");
    let data = std::fs::read(&source)?;
    let digest: String = Sha256::digest(&data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let shells = root.join("shell-preview");
    let directory = shells.join(&digest);
    let dll = directory.join("ZipziProjectPreview.dll");
    if !uninstall && !dll.is_file() {
        std::fs::create_dir_all(&directory)?;
        // Write once: Explorer may still be using this exact content-addressed file.
        std::fs::write(&dll, data)?;
    }
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(current.join("Register-FileAssociation.ps1"))
        .arg("-EditorPath")
        .arg(&launcher)
        .arg("-PreviewPath")
        .arg(&dll)
        .args(if uninstall {
            vec!["-Unregister"]
        } else {
            vec![]
        })
        .creation_flags(0x08000000)
        .status()?;
    anyhow::ensure!(status.success(), "Shell registration failed ({status})");
    if !uninstall {
        clean_unlocked_copies(&shells, &digest);
    }
    Ok(())
}

fn clean_unlocked_copies(root: &Path, current: &str) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == current || name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        // Only the exact generated file, never traverse junctions or unknown contents.
        if entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        {
            let _ = std::fs::remove_file(entry.path().join("ZipziProjectPreview.dll"));
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_registration_uses_pack_title_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("Update.exe"), []).unwrap();
        let launcher = root.join("Zipzi Editor.exe");
        std::fs::write(&launcher, []).unwrap();
        assert_eq!(installed_launcher(root).unwrap(), Some(launcher));
    }

    #[test]
    fn missing_installed_launcher_reports_error_instead_of_skipping_registration() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("Update.exe"), []).unwrap();
        std::fs::write(root.join("zipzi-editor.exe"), []).unwrap();
        assert!(installed_launcher(root).is_err());
    }

    #[test]
    fn portable_registration_is_left_to_the_registration_script() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(installed_launcher(directory.path()).unwrap(), None);
    }
}
