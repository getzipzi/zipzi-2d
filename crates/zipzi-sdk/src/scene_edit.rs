//! Selected language-neutral scene inputs. SDK/core retain defaults and planning.
use crate::{EditSession, ModelingIntent, SdkError};
use zipzi_core::{document::*, *};

pub use zipzi_contract::authoring::SceneSummary;
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
pub trait SceneSummaryRead: Sized {
    fn response_bytes(document: &Document, id: &str) -> Option<usize>;
    fn read(document: &Document, id: &str) -> Option<Self>;
}
impl SceneSummaryRead for SceneSummary {
    /// Conservative escaped-wire bound, without copying or traversing geometry.
    fn response_bytes(document: &Document, id: &str) -> Option<usize> {
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
    fn read(document: &Document, id: &str) -> Option<Self> {
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

pub use zipzi_contract::authoring::GluePairSpec;
pub use zipzi_contract::authoring::OffscreenFormSpec;
pub use zipzi_contract::authoring::ParameterGroupCreateSpec;
pub use zipzi_contract::authoring::PartCreateSpec;
pub use zipzi_contract::authoring::ScalarSample;
pub use zipzi_contract::authoring::SceneOperation;
pub use zipzi_contract::authoring::ScenePlacement;
pub use zipzi_contract::authoring::VertexPatchSpec;
pub use zipzi_contract::authoring::VertexWeightSpec;
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
pub trait SceneOperationApply {
    fn into_intent(self) -> ModelingIntent;
    fn apply(self, edit: &mut EditSession<'_>) -> Result<Vec<String>, SdkError>;
}
impl SceneOperationApply for SceneOperation {
    /// Construct existing domain intentions; no scene rules live in adapters.
    fn into_intent(self) -> ModelingIntent {
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
    fn apply(self, edit: &mut EditSession<'_>) -> Result<Vec<String>, SdkError> {
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
