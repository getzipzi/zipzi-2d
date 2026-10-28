#[path = "../source_identity.rs"]
mod source_identity;

use source_identity::{relative_name, text_bytes};
use std::path::Path;

#[test]
fn canonical_fixture_has_one_cross_platform_digest() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    let manifest = root.join("Cargo.toml");
    let source = root.join("crates/中文/lib.rs");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    let expected = "763a744031f58117890a73c612f3e02df911a4ed874b464311af6a78527f71c9";
    for ending in ["\n", "\r\n"] {
        std::fs::write(
            &manifest,
            "[package]\nname=\"test\"\n".replace('\n', ending),
        )
        .unwrap();
        std::fs::write(
            &source,
            "pub fn answer() -> u8 { 42 }\n".replace('\n', ending),
        )
        .unwrap();
        assert_eq!(
            source_identity::digest(root, vec![source.clone(), manifest.clone()]),
            expected
        );
        assert_eq!(
            source_identity::digest(root, vec![manifest.clone(), source.clone()]),
            expected
        );
    }
    std::fs::write(&source, "pub fn answer() -> u8 { 43 }\n").unwrap();
    assert_ne!(
        source_identity::digest(root, vec![source, manifest]),
        expected
    );
}

#[test]
fn checkout_line_endings_have_identical_canonical_bytes() {
    assert_eq!(
        text_bytes(b"a\r\nb\r\n".to_vec()),
        text_bytes(b"a\nb\n".to_vec())
    );
    assert_ne!(
        text_bytes(b"a\nb\n".to_vec()),
        text_bytes(b"a\nc\n".to_vec())
    );
    assert_eq!(text_bytes(b"a\rb".to_vec()), b"a\rb");
}

#[test]
fn names_are_repository_relative_with_forward_slashes() {
    let root = Path::new("checkout");
    assert_eq!(
        relative_name(root, &root.join("crates").join("中文").join("lib.rs")),
        "crates/中文/lib.rs"
    );
}
