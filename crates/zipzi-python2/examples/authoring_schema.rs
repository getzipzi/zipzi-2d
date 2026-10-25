//! Export only the reviewed P2 mesh/resource/Plan slices. Field definitions come from Rust.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    resources: Option<Box<Contract>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scene: Option<serde_json::Value>,
    modeling: Option<serde_json::Value>,
    binary: Option<serde_json::Value>,
}
fn main() {
    use zipzi_runtime::authoring::*;
    use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
    let (input, output) = MESH_QUERY.shapes();
    let contract = Contract {
        version: 1,
        resources: Some(Box::new(resource_contract())),
        binary: Some(binary_contract()),
        modeling: Some(
            serde_json::json!({"version":1,"input_roots":[0,1,2,4,5,6,7,9,11],"roots":[zipzi_runtime::modeling::ModelingCommand::api_shape(),zipzi_sdk::ModelingIntent::api_shape(),zipzi_sdk::ModelingQuery::api_shape(),zipzi_sdk::OwnedModelingRecords::api_shape(),zipzi_runtime::addressing::AddressSelector::api_shape(),zipzi_runtime::addressing::AddressPatch::api_shape(),zipzi_runtime::addressing::CapabilityDomain::api_shape(),zipzi_runtime::addressing::CollectionQuery::api_shape(),zipzi_runtime::addressing::Capabilities::api_shape(),zipzi_sdk::ObjectKind::api_shape(),zipzi_sdk::ModelingSamples::api_shape(),zipzi_runtime::project_io::ProjectRequest::api_shape(),zipzi_runtime::project_io::ProjectReport::api_shape(),zipzi_runtime::editor_context::EditorContext::api_shape()]}),
        ),
        scene: Some(serde_json::json!({
            "version": 1,
            "roots": [zipzi_sdk::SceneOperation::api_shape(), zipzi_sdk::SceneSummary::api_shape()],
            "bindings": [
                binding!(zipzi_sdk::SceneOperation, "scene", "Operation", "union"),
                binding!(zipzi_sdk::PartCreateSpec, "scene", "PartSpec", "record"),
                binding!(zipzi_sdk::ParameterGroupCreateSpec, "scene", "ParameterGroupSpec", "record"),
                binding!(zipzi_sdk::ScenePlacement, "scene", "Placement", "record"),
                binding!(zipzi_sdk::SceneSummary, "scene", "Summary", "record"),
                binding!(zipzi_sdk::VertexWeightSpec, "scene", "VertexWeight", "record"),
                binding!(zipzi_sdk::ScalarSample, "scene", "Sample", "record"),
                binding!(zipzi_sdk::VertexPatchSpec, "scene", "VertexPatch", "record"),
                binding!(zipzi_sdk::GluePairSpec, "scene", "GluePair", "record"),
                binding!(zipzi_sdk::OffscreenFormSpec, "scene", "OffscreenForm", "record"),
            ],
        })),
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

fn resource_contract() -> Contract {
    use zipzi_runtime::authoring::*;
    let mut operations = vec![];
    macro_rules! operation {
        ($registration:ident, $backend:literal, $category:literal) => {{
            let (input, output) = $registration.shapes();
            operations.push(Operation {
                name: $registration.name,
                backend: $backend,
                capability: "typed_resources_plans",
                category: $category,
                handler: concat!("zipzi_runtime::authoring::", stringify!($registration)),
                input,
                output,
            });
        }};
    }
    operation!(IMAGE_QUERY, "image", "query");
    operation!(PREPARED_PIXELS_QUERY, "prepared_pixels", "query");
    operation!(PLAN_QUERY, "plan", "planning");
    Contract {
        version: 1,
        resources: None,
        scene: None,
        modeling: None,
        binary: None,
        bindings: vec![
            binding!(
                zipzi_sdk::ImageCreateSpec,
                "resources",
                "ImageSpec",
                "record"
            ),
            binding!(
                zipzi_runtime::authoring::ResourceUsage,
                "resources",
                "Usage",
                "record"
            ),
            binding!(
                zipzi_runtime::authoring::ImagePixels,
                "resources",
                "ImagePixels",
                "record"
            ),
            binding!(
                zipzi_runtime::authoring::ImageSnapshot,
                "resources",
                "Snapshot",
                "record"
            ),
            binding!(zipzi_sdk::ConversionCost, "plans", "Cost", "record"),
            binding!(zipzi_sdk::ModelingPlanReport, "plans", "Report", "record"),
            binding!(
                zipzi_runtime::authoring::PlanSnapshot,
                "backend",
                "PlanSnapshot",
                "record"
            ),
            binding!(
                zipzi_runtime::authoring::PreparedImageInfo,
                "backend",
                "PreparedImageInfo",
                "record"
            ),
            binding!(
                zipzi_runtime::authoring::ResourceOperation,
                "ops",
                "ResourceOperation",
                "union"
            ),
            binding!(
                zipzi_runtime::authoring::PlanIntent,
                "plans",
                "Intent",
                "union"
            ),
            binding!(
                zipzi_runtime::authoring::ImageQuery,
                "backend",
                "ImageQuery",
                "query"
            ),
            binding!(
                zipzi_runtime::authoring::PlanQuery,
                "backend",
                "PlanQuery",
                "query"
            ),
            binding!(
                zipzi_runtime::authoring::PreparedPixelsQuery,
                "backend",
                "PreparedPixelsQuery",
                "query"
            ),
            binding!(
                zipzi_runtime::ImageId,
                "resources",
                "PreparedImage",
                "handle"
            ),
            binding!(zipzi_runtime::PlanId, "plans", "Plan", "handle"),
            binding!(zipzi_runtime::SessionId, "native", "SessionToken", "token"),
            binding!(zipzi_runtime::EditId, "native", "EditToken", "token"),
            binding!(zipzi_runtime::SourceRef, "_values", "Source", "source"),
            binding!(zipzi_sdk::Version, "_values", "Version", "version"),
        ],
        roots: vec![
            ResourceOperation::api_shape(),
            PlanIntent::api_shape(),
            PreparedImageInfo::api_shape(),
            ResourceUsage::api_shape(),
        ],
        operations,
    }
}

fn binary_contract() -> serde_json::Value {
    use zipzi_core::binary_wire::WireWrite;
    serde_json::json!({"version":1,"input_signature":format!("{:032x}",Vec::<zipzi_runtime::authoring::EditOperation>::SIGNATURE),
        "output_signature":format!("{:032x}",zipzi_sdk::OwnedModelingRecords::SIGNATURE),
        "edit_variants":zipzi_runtime::authoring::EditOperation::VARIANTS,
        "inputs":[zipzi_runtime::modeling::ModelingCommand::api_shape(),zipzi_sdk::SceneOperation::api_shape(),zipzi_sdk::MeshOperation::api_shape(),zipzi_sdk::ParameterOperation::api_shape(),zipzi_sdk::MeshBindingOperation::api_shape(),zipzi_runtime::authoring::ResourceOperation::api_shape()],
        "output":zipzi_sdk::OwnedModelingRecords::api_shape(),
        "mesh":zipzi_runtime::authoring::MeshSnapshot::api_shape(),
        "mesh_signature":format!("{:032x}",Vec::<zipzi_runtime::authoring::MeshSnapshot>::SIGNATURE),
        "binding_signature":format!("{:032x}",Vec::<zipzi_core::MeshBinding>::SIGNATURE),
        "source_variants":zipzi_runtime::SourceRef::VARIANTS,
        "source_fields":zipzi_runtime::SourceRef::VARIANT_FIELDS,
        "samples":zipzi_sdk::ModelingSamples::api_shape(),
        "sample_signature":format!("{:032x}",zipzi_sdk::ModelingSamples::SIGNATURE),
    })
}
