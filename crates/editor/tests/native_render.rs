#![cfg(target_os = "macos")]

#[path = "../src/mesh_edges.rs"]
mod mesh_edges;

#[path = "native_render/mesh_edges.rs"]
mod mesh_edges_tests;
#[path = "native_render/metal.rs"]
mod metal;
#[path = "native_render/overlay.rs"]
mod overlay;
