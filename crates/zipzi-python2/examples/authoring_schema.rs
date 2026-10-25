//! Export only the reviewed P2 mesh slice. Field definitions come from Rust.
use serde::Serialize;
use zipzi_core::api_shape::{ApiShape, Shape};

#[derive(Serialize)]
struct Binding {
    path: &'static str,
    rust: String,
    module: &'static str,
    name: &'static str,
    form: &'static str,
}
fn bind<T: ApiShape>(
    path: &'static str,
    module: &'static str,
    name: &'static str,
    form: &'static str,
) -> Binding {
    let rust = match T::api_shape() {
        Shape::Record { name, .. } | Shape::Union { name, .. } | Shape::Opaque { name } => name,
        _ => panic!("Binding must name a Rust type"),
    };
    Binding {
        path,
        rust,
        module,
        name,
        form,
    }
}
macro_rules! binding {
    ($ty:ty, $module:literal, $name:literal, $form:literal) => {
        bind::<$ty>(stringify!($ty), $module, $name, $form)
    };
}
#[derive(Serialize)]
struct Operation {
    name: &'static str,
    backend: &'static str,
    capability: &'static str,
    category: &'static str,
    handler: &'static str,
    input: Shape,
    output: Shape,
}
#[derive(Serialize)]
struct Contract {
    version: u32,
    bindings: Vec<Binding>,
    roots: Vec<Shape>,
    operations: Vec<Operation>,
}
fn main() {
    use zipzi_runtime::authoring::*;
    use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
    let (input, output) = MESH_QUERY.shapes();
    let contract = Contract {
        version: 1,
        bindings: vec![
            binding!(zipzi_core::Vec2, "meshes", "Point", "point"),
            binding!(
                zipzi_sdk::MeshGeometry,
                "meshes",
                "Geometry",
                "owned_record"
            ),
            binding!(zipzi_sdk::MeshCreateSpec, "meshes", "MeshSpec", "record"),
            binding!(
                zipzi_runtime::authoring::MeshSnapshot,
                "meshes",
                "Snapshot",
                "record"
            ),
            binding!(zipzi_sdk::MeshOperation, "ops", "MeshOperation", "union"),
            binding!(
                zipzi_runtime::authoring::MeshQuery,
                "backend",
                "MeshQuery",
                "query"
            ),
            binding!(zipzi_runtime::SessionId, "native", "SessionToken", "token"),
            binding!(zipzi_runtime::EditId, "native", "EditToken", "token"),
            binding!(zipzi_runtime::SourceRef, "_values", "Source", "source"),
        ],
        roots: vec![
            MeshGeometry::api_shape(),
            MeshCreateSpec::api_shape(),
            MeshOperation::api_shape(),
        ],
        operations: vec![Operation {
            name: MESH_QUERY.name,
            backend: "mesh",
            capability: "typed_meshes",
            category: "query",
            handler: "zipzi_runtime::authoring::MESH_QUERY",
            input,
            output,
        }],
    };
    println!("{}", serde_json::to_string_pretty(&contract).unwrap());
}
