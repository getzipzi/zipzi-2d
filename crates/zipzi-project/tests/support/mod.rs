#![allow(dead_code)]
use std::io::{Cursor, Read, Write};
use zipzi_core::{
    state::{DocumentBuilder, DocumentState},
    Status,
};
use zipzi_project::{ProjectIo, ResourceStore, ResourceView};

pub fn prepare(
    state: &DocumentState,
    write: impl FnOnce(&mut DocumentBuilder) -> Result<(), Status>,
) -> DocumentState {
    let mut builder = state.builder();
    write(&mut builder).unwrap();
    builder.freeze().unwrap().state
}

pub fn native_bytes(state: &DocumentState) -> Vec<u8> {
    native_bytes_with_resources(state, &ResourceView::default())
}
pub fn native_bytes_with_resources(state: &DocumentState, resources: &ResourceView) -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("model.zipzi");
    ProjectIo::new(root.path(), ResourceStore::default())
        .save(state, resources, &path, None, || true)
        .unwrap();
    std::fs::read(path).unwrap()
}

pub fn load_native(bytes: &[u8]) -> Result<DocumentState, Status> {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("model.zipzi");
    std::fs::write(&path, bytes).unwrap();
    ProjectIo::new(root.path(), ResourceStore::default())
        .load(&path, || true)
        .map(|loaded| loaded.document)
        .map_err(Into::into)
}

pub fn roundtrip(state: &DocumentState) -> DocumentState {
    load_native(&native_bytes(state)).unwrap()
}

pub fn project_json(bytes: &[u8]) -> serde_json::Value {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let entry = archive.by_name("project.json").unwrap();
    serde_json::from_reader(entry).unwrap()
}

/// Preserve container integrity so malformed-document tests reach codec admission.
pub fn replace_project_json(bytes: &[u8], document: &serde_json::Value) -> Vec<u8> {
    replace_project_text(bytes, &serde_json::to_string(document).unwrap())
}
pub fn replace_project_text(bytes: &[u8], document: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let document = document.as_bytes().to_vec();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name == "project.json" {
            data = document.clone();
        } else if name == "format.json" {
            let mut format: serde_json::Value = serde_json::from_slice(&data).unwrap();
            format["document_sha256"] = Sha256::digest(&document)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into();
            data = serde_json::to_vec(&format).unwrap();
        }
        output
            .start_file(
                name,
                zip::write::SimpleFileOptions::default().compression_method(entry.compression()),
            )
            .unwrap();
        output.write_all(&data).unwrap();
    }
    output.finish().unwrap().into_inner()
}
