//! Canonical source inputs only; compiled artifacts retain raw-byte hashes.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn digest(root: &Path, mut files: Vec<PathBuf>) -> String {
    files.sort_by_key(|path| relative_name(root, path));
    let mut hash = Sha256::new();
    for path in files {
        let relative = relative_name(root, &path);
        let data = text_bytes(std::fs::read(path).expect("read build identity input"));
        hash.update(relative.as_bytes());
        hash.update([0]);
        hash.update((data.len() as u64).to_be_bytes());
        hash.update(data);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("source is within workspace")
        .components()
        .map(|part| part.as_os_str().to_str().expect("UTF-8 source path"))
        .collect::<Vec<_>>()
        .join("/")
}

pub fn text_bytes(bytes: Vec<u8>) -> Vec<u8> {
    let text = String::from_utf8(bytes).expect("source identity input must be UTF-8 text");
    text.replace("\r\n", "\n").into_bytes()
}
