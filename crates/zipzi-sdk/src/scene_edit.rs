//! Selected language-neutral scene inputs. SDK/core retain defaults and planning.
use crate::{EditSession, ModelingIntent, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{document::*, *};

zipzi_core::api_record! {
    /// Metadata projection; geometry remains in the explicit mesh/CPU queries.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SceneSummary {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        pub runtime_id: String,
        /// Object category for this metadata record; use it to select the corresponding domain API.
        pub object_kind: String,
        /// Optional organizational Part ID; None means root or not applicable for this object kind.
        pub part_id: Option<String>,
        /// Optional deformation parent for meshes/transforms or organization parent for
        /// Parts/groups; None means root or not applicable.
        pub parent_id: Option<String>,
        /// Whether this object participates in evaluation; editor hiding is a separate state.
        pub enabled: bool,
    }
}
type SummaryFields<'a> = (
    &'a str,
    &'a str,
    &'static str,
    Option<&'a str>,
    Option<&'a str>,
    bool,
);
fn summary_fields<'a>(document: &'a Document, id: &str) -> Option<SummaryFields<'a>> {
    let (name, runtime_id, object_kind, part_id, parent_id, enabled) =
        if let Some(m) = document.get_mesh(id) {
            (
                &m.name,
                &m.runtime_id,
                "mesh",
                (!m.part_id.is_empty()).then_some(m.part_id.as_str()),
                (!m.deformer_id.is_empty()).then_some(m.deformer_id.as_str()),
                m.enabled,
            )
        } else if let Some(t) = document.get_transform(id) {
            (
                &t.name,
                &t.runtime_id,
                "transform",
                t.part_id.as_ref().map(|v| v.as_str()),
                t.parent_id.as_ref().map(|v| v.as_str()),
                t.enabled,
            )
        } else if let Some(p) = document.get_part(id) {
            (
                &p.name,
                &p.runtime_id,
                "part",
                None,
                (!p.parent_id.is_empty()).then_some(p.parent_id.as_str()),
                p.enabled,
            )
        } else if let Some(g) = document.get_glue(id) {
            (&g.name, &g.runtime_id, "glue", None, None, true)
        } else if let Some(o) = document.get_offscreen(id) {
            (
                &o.name,
                &o.runtime_id,
                "offscreen",
                Some(o.part_id.as_str()),
                None,
                o.enabled,
            )
        } else if let Some(g) = document.get_parameter_group(id) {
            (
                &g.name,
                &g.runtime_id,
                "parameter_group",
                None,
                g.parent_id.as_deref(),
                true,
            )
        } else {
            return None;
        };
    Some((name, runtime_id, object_kind, part_id, parent_id, enabled))
}
impl SceneSummary {
    /// Conservative escaped-wire bound, without copying or traversing geometry.
    pub fn response_bytes(document: &Document, id: &str) -> Option<usize> {
        let (name, runtime, kind, part, parent, _) = summary_fields(document, id)?;
        let text = [
            id,
            name,
            runtime,
            kind,
            part.unwrap_or_default(),
            parent.unwrap_or_default(),
        ];
        Some(text.iter().fold(4096usize, |n, text| {
            n.saturating_add(text.len().saturating_mul(8))
        }))
    }
    pub fn read(document: &Document, id: &str) -> Option<Self> {
        let (name, runtime_id, object_kind, part_id, parent_id, enabled) =
            summary_fields(document, id)?;
        Some(Self {
            id: id.into(),
            name: name.into(),
            runtime_id: runtime_id.into(),
            object_kind: object_kind.into(),
            part_id: part_id.map(Into::into),
            parent_id: parent_id.map(Into::into),
            enabled,
        })
    }
}

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct PartCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Optional new object UUID; None allocates a fresh UUID in the SDK when creation executes.
        pub id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        pub parent_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        pub before: Option<String>,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ParameterGroupCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Optional new object UUID; None allocates a fresh UUID in the SDK when creation executes.
        pub id: Option<String>,
        /// Optional external group identifier; None uses the native default empty string.
        pub runtime_id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        pub parent_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        pub before: Option<String>,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ScenePlacement {
        /// Optional organizational Part ID; None uses the root organization.
        pub part_id: Option<String>,
        /// Optional deformation-parent transform ID; None uses model space.
        pub parent_id: Option<String>,
        /// Optional organization-order sibling ID; None appends in the selected organization
        /// collection.
        pub before: Option<String>,
        /// Optional deformation-order sibling ID; None appends in the selected deformation
        /// collection.
        pub deformation_before: Option<String>,
    }
}
zipzi_core::api_union! {
    tag = "op";
    #[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub enum SceneOperation {
        CreatePart {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: PartCreateSpec,
        },
        CreateParameterGroup {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: ParameterGroupCreateSpec,
        },
        CreateWarp {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Explicit organizational and deformation placement, with independent ordering
            /// anchors.
            placement: ScenePlacement,
            /// Optional [left, bottom, right, top] warp bounds in parent coordinates; None
            /// requests native defaults.
            bounds: Option<[f32; 4]>,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateRotation {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Explicit organizational and deformation placement, with independent ordering
            /// anchors.
            placement: ScenePlacement,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateGlue {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and
            /// unresolved references retain runtime identifiers.
            a: String,
            /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and
            /// unresolved references retain runtime identifiers.
            b: String,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateOffscreen {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Owning Part object ID; it must name an existing Part.
            part_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        MoveToPart {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveDeformation {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveParameters {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parameter-group ID in the document metadata hierarchy. None selects the
            /// root/unassigned container.
            group_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveParameterGroups {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DuplicateScene {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Include the deformation subtree when true; false affects only the selected objects.
            subtree: bool,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DeleteScene {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Include the deformation subtree when true; false affects only the selected objects.
            subtree: bool,
        },
        DeleteOrganization {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Preserve contained objects by dissolving their organization container instead of
            /// deleting its contents.
            dissolve: bool,
        },
        SetSceneLabel {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Exact current field value used as an optimistic precondition; stale values
            /// reject the edit.
            expected: String,
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
        },
        SetSceneEnabled {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Exact current field value used as an optimistic precondition; stale values
            /// reject the edit.
            expected: bool,
            /// Whether this object participates in evaluation; editor hiding is a separate state.
            enabled: bool,
        },
        ReparentLocal {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        ReplaceTriangles {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Triangle triples of stable vertex IDs; preserve winding and do not substitute
            /// position-array offsets.
            triangles: Vec<[u32; 3]>,
        },
        ReorderMeshVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Complete new order of stable vertex IDs; preserves identities and associated
            /// per-vertex payload.
            order: Vec<u32>,
        },
        InsertMeshVertex {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable u32 vertex ID within the mesh, not an offset into its position array.
            vertex_id: u32,
            /// Absolute XY authoring position for this vertex, not a displacement.
            position: [f32; 2],
            /// Texture UV pair for this vertex; not a model-space position.
            uv: [f32; 2],
            /// Source vertex IDs and interpolation weights used to seed dependent shape content.
            weights: Vec<VertexWeightSpec>,
            /// Optional stable vertex ID before which to insert new vertices; None appends.
            /// This is not a position-array index.
            before: Option<u32>,
        },
        DeleteMeshVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable vertex IDs in this mesh, not offsets into positions.
            vertices: Vec<u32>,
            /// Explicitly remove dependent topology/keyform/Glue content; false rejects
            /// referenced-vertex deletion.
            remove_dependents: bool,
        },
        CreateBlendTable {
            /// Stable document parameter ID, not its exported runtime identifier.
            parameter_id: String,
            /// Strictly increasing finite parameter samples with stable IDs; base_sample_id
            /// must name one of them.
            samples: Vec<ScalarSample>,
            /// Sample ID defining the neutral/base shape in this key table.
            base_sample_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateMeshBlend {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable BlendShape key-table ID in this document.
            table_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        SetMeshBlendVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable binding ID owning the addressed axes or keyforms.
            binding_id: String,
            /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
            /// parameter value.
            sample_id: String,
            /// Ordered content patches applied together by the native edit.
            updates: Vec<VertexPatchSpec>,
        },
        InsertGluePair {
            /// Stable Glue object ID owning the affected vertex pairs.
            glue_id: String,
            /// Complete Glue pair to insert or replace; endpoint IDs belong to the two
            /// connected meshes.
            pair: GluePairSpec,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DeleteGluePairs {
            /// Stable Glue object ID owning the affected vertex pairs.
            glue_id: String,
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
        },
        PatchOffscreenKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
            /// object ID.
            slot: u32,
            /// Scalar value in the referenced parameter domain.
            value: OffscreenFormSpec,
        },
        InsertOffscreenKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Scalar value in the referenced parameter domain.
            value: OffscreenFormSpec,
            /// Optional stable vertex ID before which to insert new vertices; None appends.
            /// This is not a position-array index.
            before: Option<u32>,
        },
        SetOffscreenStaticKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Complete shape sample to insert or replace; unrelated samples are retained.
            keyform: i32,
        },
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct VertexWeightSpec {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        pub vertex_id: u32,
        /// Dimensionless interpolation weight applied to the referenced source vertex.
        pub weight: f32,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ScalarSample {
        /// Stable UUID assigned to this parameter sample.
        pub id: String,
        /// Finite parameter sample coordinate in the driving parameter domain.
        pub value: f32,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct VertexPatchSpec {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        pub vertex_id: u32,
        /// Optional BlendShape XY displacement for this vertex; None preserves its delta.
        pub position: Option<[f32; 2]>,
        /// UV edits require the base-shape content API; keep None for BlendShape patches.
        pub uv: Option<[f32; 2]>,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct GluePairSpec {
        /// Dimensionless contribution of endpoint A; default Glue pair weight is 0.5.
        pub weight_a: f32,
        /// Dimensionless contribution of endpoint B; default Glue pair weight is 0.5.
        pub weight_b: f32,
        /// Optional new Glue-pair UUID; None allocates a fresh pair identity in the SDK.
        pub id: Option<String>,
        /// Vertex ID in endpoint mesh A; None explicitly leaves this endpoint unattached.
        pub vertex_a: Option<u32>,
        /// Vertex ID in endpoint mesh B; None explicitly leaves this endpoint unattached.
        pub vertex_b: Option<u32>,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct OffscreenFormSpec {
        /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
        /// above 1 are retained without clamping.
        pub opacity: f32,
        /// RGB multiply factors in channel order R, G, B; (1, 1, 1) is neutral.
        pub multiply: Option<[f32; 3]>,
        /// RGB screen factors in channel order R, G, B; (0, 0, 0) is neutral.
        pub screen: Option<[f32; 3]>,
    }
}
fn identity(id: Option<String>) -> String {
    id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}
fn scene_create(
    id: Option<String>,
    name: String,
    object: SceneCreate,
    p: ScenePlacement,
) -> ModelingIntent {
    ModelingIntent::Scene(SceneEdit::Create {
        id: identity(id),
        name,
        object,
        part_id: p.part_id,
        parent_id: p.parent_id,
        before: p.before,
        deformation_before: p.deformation_before,
    })
}
impl SceneOperation {
    /// Construct existing domain intentions; no scene rules live in adapters.
    pub fn into_intent(self) -> ModelingIntent {
        let scope = |subtree| {
            if subtree {
                SceneCopyScope::DeformationSubtree
            } else {
                SceneCopyScope::Selected
            }
        };
        match self {
            Self::CreatePart { spec } => {
                ModelingIntent::Organization(OrganizationEdit::CreatePart {
                    part: Part {
                        id: identity(spec.id),
                        name: spec.name,
                        parent_id: spec.parent_id.unwrap_or_default(),
                        ..Default::default()
                    },
                    before: spec.before,
                })
            }
            Self::CreateParameterGroup { spec } => {
                let id = identity(spec.id);
                ModelingIntent::Organization(OrganizationEdit::CreateGroup {
                    group: CdiParameterGroup {
                        runtime_id: spec.runtime_id.unwrap_or_else(|| id.clone()),
                        id,
                        name: spec.name,
                        parent_id: spec.parent_id,
                        extensions: Default::default(),
                    },
                    before: spec.before,
                })
            }
            Self::CreateWarp {
                name,
                placement,
                bounds,
                id,
            } => scene_create(id, name, SceneCreate::Warp { bounds }, placement),
            Self::CreateRotation {
                name,
                placement,
                id,
            } => scene_create(id, name, SceneCreate::Rotation, placement),
            Self::CreateGlue {
                name,
                a,
                b,
                part_id,
                id,
            } => scene_create(
                id,
                name,
                SceneCreate::Glue {
                    a: a.into(),
                    b: b.into(),
                },
                ScenePlacement {
                    part_id,
                    parent_id: None,
                    before: None,
                    deformation_before: None,
                },
            ),
            Self::CreateOffscreen { name, part_id, id } => scene_create(
                id,
                name,
                SceneCreate::Offscreen,
                ScenePlacement {
                    part_id: Some(part_id),
                    parent_id: None,
                    before: None,
                    deformation_before: None,
                },
            ),
            Self::MoveToPart {
                ids,
                part_id,
                before,
            } => ModelingIntent::Organization(OrganizationEdit::Move {
                ids,
                destination: OrganizationCollection::PartMembers { part_id },
                before,
            }),
            Self::MoveDeformation {
                ids,
                parent_id,
                before,
            } => ModelingIntent::Scene(SceneEdit::Move {
                ids,
                destination: SceneCollection::Deformation { parent_id },
                before,
            }),
            Self::MoveParameters {
                ids,
                group_id,
                before,
            } => ModelingIntent::Organization(OrganizationEdit::Move {
                ids,
                destination: OrganizationCollection::Parameters { group_id },
                before,
            }),
            Self::MoveParameterGroups {
                ids,
                parent_id,
                before,
            } => ModelingIntent::Organization(OrganizationEdit::Move {
                ids,
                destination: OrganizationCollection::ParameterGroups { parent_id },
                before,
            }),
            Self::DuplicateScene {
                ids,
                subtree,
                part_id,
                before,
            } => ModelingIntent::Scene(SceneEdit::Duplicate {
                ids,
                scope: scope(subtree),
                destination: Some(OrganizationCollection::PartMembers { part_id }),
                before,
            }),
            Self::DeleteScene { ids, subtree } => ModelingIntent::Scene(SceneEdit::Delete {
                ids,
                scope: scope(subtree),
            }),
            Self::DeleteOrganization { ids, dissolve } => {
                ModelingIntent::Organization(OrganizationEdit::Delete {
                    ids,
                    policy: if dissolve {
                        OrganizationDeletePolicy::Dissolve
                    } else {
                        OrganizationDeletePolicy::Contents
                    },
                })
            }
            Self::SetSceneLabel { id, expected, name } => ModelingIntent::Scene(SceneEdit::Patch {
                id,
                expected: SceneFieldValue::Label(expected),
                value: SceneFieldValue::Label(name),
                clear_glue_pairs: false,
            }),
            Self::SetSceneEnabled {
                id,
                expected,
                enabled,
            } => ModelingIntent::Scene(SceneEdit::Patch {
                id,
                expected: SceneFieldValue::Enabled(expected),
                value: SceneFieldValue::Enabled(enabled),
                clear_glue_pairs: false,
            }),
            Self::ReparentLocal {
                ids,
                parent_id,
                before,
            } => {
                let mut edit = ReparentEdit::keep_local(ids, parent_id.unwrap_or_default());
                edit.placement = Some(ReparentPlacement { before });
                ModelingIntent::Scene(SceneEdit::Reparent { edit })
            }
            Self::ReplaceTriangles { mesh_id, triangles } => {
                ModelingIntent::Content(SceneContentEdit::ReplaceTriangles { mesh_id, triangles })
            }
            Self::ReorderMeshVertices { mesh_id, order } => {
                ModelingIntent::Content(SceneContentEdit::ReorderMeshVertices { mesh_id, order })
            }
            Self::InsertMeshVertex {
                mesh_id,
                vertex_id,
                position,
                uv,
                weights,
                before,
            } => ModelingIntent::Content(SceneContentEdit::InsertMeshVertices {
                mesh_id,
                before,
                vertices: vec![NewMeshVertex {
                    vertex_id,
                    position: Vec2::new(position[0], position[1]),
                    uv: Vec2::new(uv[0], uv[1]),
                    seed: if weights.is_empty() {
                        VertexSeed::Base
                    } else {
                        VertexSeed::Interpolate {
                            weights: weights
                                .into_iter()
                                .map(|v| VertexWeight {
                                    vertex_id: v.vertex_id,
                                    weight: v.weight,
                                })
                                .collect(),
                        }
                    },
                }],
            }),
            Self::DeleteMeshVertices {
                mesh_id,
                vertices,
                remove_dependents,
            } => ModelingIntent::Content(SceneContentEdit::DeleteMeshVertices {
                mesh_id,
                vertices,
                policy: if remove_dependents {
                    VertexDeletePolicy::RemoveDependentContent
                } else {
                    VertexDeletePolicy::RejectReferences
                },
            }),
            Self::CreateBlendTable {
                parameter_id,
                samples,
                base_sample_id,
                id,
            } => ModelingIntent::Binding(crate::BindingAuthoringEdit::Blend(
                BlendAuthoringEdit::CreateTable {
                    table: BlendShapeKeyTable {
                        id: identity(id),
                        parameter_id,
                        samples: samples
                            .into_iter()
                            .map(|v| BindingSample {
                                id: v.id,
                                value: v.value,
                            })
                            .collect(),
                        base_sample_id,
                        keys: vec![],
                        base_key_idx: 0,
                    },
                },
            )),
            Self::CreateMeshBlend {
                mesh_id,
                table_id,
                id,
            } => ModelingIntent::Binding(crate::BindingAuthoringEdit::Blend(
                BlendAuthoringEdit::CreateBinding {
                    id: identity(id),
                    target_id: mesh_id,
                    target_kind: BlendShapeTargetKind::Mesh,
                    key_table_id: table_id,
                },
            )),
            Self::SetMeshBlendVertices {
                mesh_id,
                binding_id,
                sample_id,
                updates,
            } => ModelingIntent::Content(SceneContentEdit::MeshVertices {
                mesh_id,
                target: ShapeTarget::Blend {
                    binding_id,
                    sample_id,
                },
                updates: updates
                    .into_iter()
                    .map(|v| VertexContentPatch {
                        vertex_id: v.vertex_id,
                        position: v.position.map(|p| Vec2::new(p[0], p[1])),
                        uv: v.uv.map(|p| Vec2::new(p[0], p[1])),
                    })
                    .collect(),
            }),
            Self::InsertGluePair {
                glue_id,
                pair,
                before,
            } => ModelingIntent::Content(SceneContentEdit::InsertGluePair {
                glue_id,
                before,
                pair: GlueVertexPair {
                    id: identity(pair.id),
                    vertex_a: pair.vertex_a,
                    vertex_b: pair.vertex_b,
                    weight_a: pair.weight_a,
                    weight_b: pair.weight_b,
                },
            }),
            Self::DeleteGluePairs { glue_id, ids } => {
                ModelingIntent::Content(SceneContentEdit::DeleteGluePairs { glue_id, ids })
            }
            Self::PatchOffscreenKeyform {
                offscreen_id,
                slot,
                value,
            } => ModelingIntent::Content(SceneContentEdit::PatchOffscreenKeyform {
                offscreen_id,
                slot: slot as usize,
                value: OffscreenKeyform {
                    opacity: value.opacity,
                    multiply: value.multiply,
                    screen: value.screen,
                },
            }),
            Self::InsertOffscreenKeyform {
                offscreen_id,
                value,
                before,
            } => ModelingIntent::Content(SceneContentEdit::InsertOffscreenKeyform {
                offscreen_id,
                before: before.map(|v| v as usize),
                value: OffscreenKeyform {
                    opacity: value.opacity,
                    multiply: value.multiply,
                    screen: value.screen,
                },
            }),
            Self::SetOffscreenStaticKeyform {
                offscreen_id,
                keyform,
            } => ModelingIntent::Content(SceneContentEdit::SetOffscreenMapping {
                offscreen_id,
                updates: vec![OffscreenMappingPatch {
                    slot: PartKeyformSlot::Static,
                    keyform,
                }],
            }),
        }
    }
    pub fn apply(self, edit: &mut EditSession<'_>) -> Result<Vec<String>, SdkError> {
        match self.into_intent() {
            ModelingIntent::Scene(intent) => edit.edit_scene_ids(intent),
            ModelingIntent::Organization(intent) => {
                let ids = match &intent {
                    OrganizationEdit::CreatePart { part, .. } => vec![part.id.clone()],
                    OrganizationEdit::CreateGroup { group, .. } => vec![group.id.clone()],
                    OrganizationEdit::Move { ids, .. } | OrganizationEdit::Delete { ids, .. } => {
                        ids.clone()
                    }
                    _ => unreachable!(),
                };
                edit.edit_organization(intent)?;
                Ok(ids)
            }
            ModelingIntent::Content(intent) => {
                let id = match &intent {
                    SceneContentEdit::ReplaceTriangles { mesh_id, .. }
                    | SceneContentEdit::ReorderMeshVertices { mesh_id, .. }
                    | SceneContentEdit::InsertMeshVertices { mesh_id, .. }
                    | SceneContentEdit::DeleteMeshVertices { mesh_id, .. }
                    | SceneContentEdit::MeshVertices { mesh_id, .. } => mesh_id.clone(),
                    SceneContentEdit::InsertGluePair { glue_id, .. }
                    | SceneContentEdit::DeleteGluePairs { glue_id, .. } => glue_id.clone(),
                    SceneContentEdit::PatchOffscreenKeyform { offscreen_id, .. }
                    | SceneContentEdit::InsertOffscreenKeyform { offscreen_id, .. }
                    | SceneContentEdit::SetOffscreenMapping { offscreen_id, .. } => {
                        offscreen_id.clone()
                    }
                    _ => unreachable!(),
                };
                edit.edit_scene_content(intent)?;
                Ok(vec![id])
            }
            ModelingIntent::Binding(intent) => {
                let id = match &intent {
                    crate::BindingAuthoringEdit::Blend(BlendAuthoringEdit::CreateTable {
                        table,
                    }) => table.id.clone(),
                    crate::BindingAuthoringEdit::Blend(BlendAuthoringEdit::CreateBinding {
                        id,
                        ..
                    }) => id.clone(),
                    _ => unreachable!(),
                };
                edit.binding_authoring(intent)?;
                Ok(vec![id])
            }
            _ => unreachable!(),
        }
    }
}
