//! Export only the reviewed P2 mesh/resource/Plan slices. Field definitions come from Rust.
// Projection aliases preserve the reviewed native adapter paths while every
// underlying value and operation declaration comes from the contract crate.
mod zipzi_core {
    pub use zipzi_contract::model::*;
    pub mod api_shape {
        pub use zipzi_contract::api_shape::*;
    }
    pub mod binary_wire {
        pub use zipzi_contract::binary_wire::*;
    }
}
mod zipzi_sdk {
    pub use zipzi_contract::authoring::*;
    pub use zipzi_contract::document::ConversionCost;
    pub use zipzi_contract::spatial::*;
}
mod zipzi_runtime {
    pub use zipzi_contract::runtime::*;
    pub mod authoring {
        pub use zipzi_contract::operations::{
            IMAGE_QUERY, MESH_QUERY, PLAN_QUERY, PREPARED_PIXELS_QUERY,
        };
        pub use zipzi_contract::runtime::*;
    }
    pub mod addressing {
        pub use zipzi_contract::runtime::*;
    }
    pub mod editor_context {
        pub use zipzi_contract::runtime::*;
    }
    pub mod modeling {
        pub use zipzi_contract::runtime::*;
    }
    pub mod observation {
        pub use zipzi_contract::runtime::*;
    }
    pub mod preview {
        pub use zipzi_contract::runtime::*;
    }
    pub mod project_io {
        pub use zipzi_contract::project::*;
    }
    pub mod protocol {
        pub use zipzi_contract::protocol::CONTRACT;
    }
}

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
        Shape::Record { name, .. }
        | Shape::Union { name, .. }
        | Shape::Opaque { name }
        | Shape::SerdeRecord { name, .. }
        | Shape::SerdeEnum { name, .. } => name,
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
    protocol: &'static str,
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
    dtos: Option<serde_json::Value>,
    runtime: Option<serde_json::Value>,
}
fn main() {
    use zipzi_runtime::authoring::*;
    use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
    let (input, output) = MESH_QUERY.shapes();
    let contract = Contract {
        protocol: zipzi_runtime::protocol::CONTRACT,
        version: 1,
        resources: Some(Box::new(resource_contract())),
        binary: Some(binary_contract()),
        dtos: Some(dto_contract()),
        runtime: Some(
            serde_json::json!({"ObjectSummary":zipzi_runtime::ObjectSummary::api_shape(),"RuntimeCapabilities":zipzi_runtime::RuntimeCapabilities::api_shape(),"RuntimeLimits":zipzi_runtime::RuntimeLimits::api_shape()}),
        ),
        modeling: Some(
            serde_json::json!({"version":1,"input_roots":[0,1,2,4,5,6,7,9,11,14,17,19,24],"native_only_roots":[zipzi_sdk::ParameterOperation::api_shape(),zipzi_sdk::MeshBindingOperation::api_shape(),zipzi_sdk::GeometryChecks::api_shape()],"roots":[zipzi_runtime::modeling::ModelingCommand::api_shape(),zipzi_sdk::ModelingIntent::api_shape(),zipzi_sdk::ModelingQuery::api_shape(),zipzi_sdk::OwnedModelingRecords::api_shape(),zipzi_runtime::addressing::AddressSelector::api_shape(),zipzi_runtime::addressing::AddressPatch::api_shape(),zipzi_runtime::addressing::CapabilityDomain::api_shape(),zipzi_runtime::addressing::CollectionQuery::api_shape(),zipzi_runtime::addressing::Capabilities::api_shape(),zipzi_sdk::ObjectKind::api_shape(),zipzi_sdk::ModelingSamples::api_shape(),zipzi_runtime::project_io::ProjectRequest::api_shape(),zipzi_runtime::project_io::ProjectReport::api_shape(),zipzi_runtime::editor_context::EditorContext::api_shape(),zipzi_runtime::preview::PreviewCommand::api_shape(),zipzi_runtime::preview::PreviewState::api_shape(),zipzi_runtime::preview::PreviewBatch::api_shape(),zipzi_sdk::SpatialQuery::api_shape(),zipzi_sdk::SpatialSamples::api_shape(),zipzi_runtime::observation::ObservationView::api_shape(),zipzi_runtime::observation::ObservationBatch::api_shape(),zipzi_sdk::ManualMeshTransferReport::api_shape(),zipzi_sdk::DeletionSummary::api_shape(),zipzi_sdk::DeletionPage::api_shape(),zipzi_sdk::DeletionSection::api_shape()]}),
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
                zipzi_core::MeshConstraintKind,
                "meshes",
                "ConstraintKind",
                "enum"
            ),
            binding!(
                zipzi_core::MeshConstraint,
                "meshes",
                "Constraint",
                "owned_record"
            ),
            binding!(zipzi_core::MeshMeshing, "meshes", "Meshing", "owned_record"),
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
                "Mesh",
                "snapshot"
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
        protocol: zipzi_runtime::protocol::CONTRACT,
        version: 1,
        resources: None,
        scene: None,
        modeling: None,
        binary: None,
        dtos: None,
        runtime: None,
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
                "Image",
                "snapshot"
            ),
            binding!(zipzi_sdk::ConversionCost, "plans", "PlanCost", "record"),
            binding!(
                zipzi_sdk::DeletionSummary,
                "plans",
                "DeletionSummary",
                "external"
            ),
            binding!(
                zipzi_sdk::ModelingPlanReport,
                "plans",
                "PlanReport",
                "record"
            ),
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
        "image":zipzi_runtime::authoring::ImageSnapshot::api_shape(),
        "image_signature":format!("{:032x}",Vec::<zipzi_runtime::authoring::ImageSnapshot>::SIGNATURE),
        "mesh_signature":format!("{:032x}",Vec::<zipzi_runtime::authoring::MeshSnapshot>::SIGNATURE),
        "binding_signature":format!("{:032x}",Vec::<zipzi_core::MeshBinding>::SIGNATURE),
        "source_variants":zipzi_runtime::SourceRef::VARIANTS,
        "source_fields":zipzi_runtime::SourceRef::VARIANT_FIELDS,
        "samples":zipzi_sdk::ModelingSamples::api_shape(),
        "preview":zipzi_runtime::preview::PreviewBatch::api_shape(),
        "observation":zipzi_runtime::observation::ObservationBatch::api_shape(),
        "observation_signature":format!("{:032x}",zipzi_runtime::observation::ObservationBatch::SIGNATURE),
        "spatial":zipzi_sdk::SpatialSamples::api_shape(),
        "spatial_signature":format!("{:032x}",zipzi_sdk::SpatialSamples::SIGNATURE),
        "preview_signature":format!("{:032x}",zipzi_runtime::preview::PreviewBatch::SIGNATURE),
        "sample_signature":format!("{:032x}",zipzi_sdk::ModelingSamples::SIGNATURE),
    })
}

fn dto_contract() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "parameters": [zipzi_sdk::ParameterOperation::api_shape(), zipzi_core::Parameter::api_shape()],
        "bindings": [zipzi_sdk::MeshBindingOperation::api_shape(), zipzi_core::MeshBinding::api_shape()],
        "sampling": [zipzi_sdk::GeometryChecks::api_shape(), zipzi_sdk::ModelingSamples::api_shape()],
        "sampling_outer": zipzi_contract::runtime::ModelingSampleData::api_fields(),
        "references": [zipzi_sdk::SceneCapabilities::api_shape(), zipzi_sdk::ObjectKind::api_shape()],
        "events": {"cursor":zipzi_runtime::EventCursor::api_fields(),"origin":zipzi_runtime::ClientId::api_fields(),"event":zipzi_runtime::RuntimeEvent::api_fields(),"kind":zipzi_runtime::EventKind::api_fields()},
        "planning": zipzi_sdk::ModelingIntent::api_shape(),
    })
}
