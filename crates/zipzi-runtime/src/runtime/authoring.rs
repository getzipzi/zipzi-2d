use super::*;
use crate::authoring::{EditOperation, MeshSnapshot, PlanIntent, PlanSnapshot, WriteResult};

enum ReadyOperation {
    ManualMesh(zipzi_sdk::ManualMeshPlan),
    Scene(zipzi_sdk::SceneOperation),
    Modeling(Box<crate::modeling::ModelingCommand>),
    MeshBinding(zipzi_sdk::MeshBindingOperation),
    Parameter(zipzi_sdk::ParameterOperation),
    Mesh(zipzi_sdk::MeshOperation),
    Image(zipzi_sdk::ImageCreateSpec, zipzi_sdk::PreparedImageContent),
    ReplaceImage(String, zipzi_sdk::PreparedImageContent),
    RenameImage(String, String, Option<String>),
    Plan(Arc<zipzi_sdk::ModelingPlan>, bool),
}

impl Runtime {
    pub fn scene_snapshot(
        &self,
        client: ClientId,
        input: crate::authoring::SceneQuery,
    ) -> Result<crate::authoring::SceneBatch> {
        self.with_authoring_source(client, input.session, input.edit, |doc, source| {
            if input.ids.len() > 256 {
                return Err(RuntimeError::Budget("scene query count"));
            }
            // Admit borrowed metadata before response cloning/adapter expansion.
            let mut bytes = 4096usize;
            for id in &input.ids {
                let size = zipzi_sdk::SceneSummary::response_bytes(doc, id).ok_or_else(|| {
                    RuntimeError::Sdk(zipzi_sdk::SdkError {
                        code: "NOT_FOUND".into(),
                        message: "Scene object does not exist".into(),
                        operation: "scene.get",
                        object_ids: vec![id.clone()],
                        field_path: None,
                        expected_version: None,
                        actual_version: None,
                        referrers: Box::new([]),
                    })
                })?;
                bytes = bytes.saturating_add(size.saturating_add(512));
                if bytes > 1 << 20 {
                    return Err(RuntimeError::Budget("scene query bytes"));
                }
            }
            self.check_bytes(bytes.saturating_mul(128), 0, 0)?;
            Ok(crate::authoring::SceneBatch {
                source,
                items: input
                    .ids
                    .iter()
                    .map(|id| zipzi_sdk::SceneSummary::read(doc, id).expect("admitted metadata"))
                    .collect(),
            })
        })
    }

    pub fn plan_scene_authoring(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        operation: zipzi_sdk::SceneOperation,
    ) -> Result<PlanSnapshot> {
        let (plan, source, report) =
            self.plan_modeling(client, session, edit, operation.into_intent())?;
        Ok(PlanSnapshot {
            plan,
            source,
            report,
        })
    }
    pub fn edit_meshes(
        &mut self,
        client: ClientId,
        edit: EditId,
        operations: Vec<zipzi_sdk::MeshOperation>,
    ) -> Result<WriteResult> {
        self.apply_authoring(
            client,
            edit,
            operations
                .into_iter()
                .map(|operation| EditOperation::Mesh { operation })
                .collect(),
        )
    }

    pub fn apply_authoring(
        &mut self,
        client: ClientId,
        edit: EditId,
        operations: Vec<EditOperation>,
    ) -> Result<WriteResult> {
        if self.edit(client, edit)?.state != EditState::Editing {
            return Err(RuntimeError::Aborted);
        }
        if self.edit(client, edit)?.pending_upload.is_some() {
            return Err(RuntimeError::Busy);
        }
        if operations.len() > 1024 {
            return Err(RuntimeError::Budget("authoring operations"));
        }
        for operation in &operations {
            if let EditOperation::Modeling { operation } = operation {
                if let crate::modeling::ModelingCommand::ManualMesh { expected, .. } =
                    operation.as_ref()
                {
                    if operations.len() != 1 {
                        return Err(RuntimeError::InvalidReference);
                    }
                    let entry = self.edit(client, edit)?;
                    let current = SourceRef::Edit {
                        session: entry.session,
                        edit,
                        base: entry.base,
                        sequence: entry.sequence,
                    };
                    if *expected != current {
                        return Err(RuntimeError::InvalidReference);
                    }
                    if self.ready(entry.session)?.version() != entry.base {
                        self.abort_edit(client, edit)?;
                        return Err(RuntimeError::InvalidReference);
                    }
                }
            }
        }
        use zipzi_core::binary_wire::WireWrite;
        // Input resolution, COW detachment, planner workspace and publication
        // growth are admitted before the first SDK mutation in this batch.
        let document = self.edit_document(client, edit)?.estimated_content_bytes();
        let input = operations.expansion_bytes();
        self.refresh_memory_budget();
        let available = self
            .limits
            .accounted_bytes
            .saturating_sub(self.accounted_bytes());
        let fixed = input
            .saturating_add(document.saturating_mul(3))
            .saturating_add(4096);
        if available <= fixed {
            return Err(RuntimeError::Budget("authoring workspace"));
        }
        let session = self.edit(client, edit)?.session;
        let mut limits = self.ready(session)?.authoring_limits();
        limits.max_conversion_bytes = limits
            .max_conversion_bytes
            .min((available - fixed) / 2)
            .max(1);
        let workspace = self.reserve_transfer(
            fixed.saturating_add(limits.max_conversion_bytes.saturating_mul(2)),
        )?;
        let manual_request = operations.iter().any(|op| matches!(op, EditOperation::Modeling {operation} if matches!(operation.as_ref(), crate::modeling::ModelingCommand::ManualMesh {..})));
        let resolved = operations
            .into_iter()
            .enumerate()
            .map(|(index, operation)| {
                Ok(match operation {
                    EditOperation::Scene { operation } => ReadyOperation::Scene(operation),
                    EditOperation::Modeling { operation } => {
                        if let crate::modeling::ModelingCommand::ManualMesh {
                            mesh_id,
                            operation,
                            ..
                        } = *operation
                        {
                            // Geometric rejection is isolated from apply_edit's fatal
                            // transaction error path. Keep the latest valid preview.
                            let sdk = self
                                .edit(client, edit)?
                                .sdk
                                .as_ref()
                                .ok_or(RuntimeError::Aborted)?;
                            ReadyOperation::ManualMesh(
                                sdk.plan_manual_mesh_with_limits(&mesh_id, operation, limits)?,
                            )
                        } else {
                            ReadyOperation::Modeling(operation)
                        }
                    }
                    EditOperation::MeshBinding { operation } => {
                        ReadyOperation::MeshBinding(operation)
                    }
                    EditOperation::Parameter { operation } => ReadyOperation::Parameter(operation),
                    EditOperation::Mesh { operation } => ReadyOperation::Mesh(operation),
                    EditOperation::Resource(crate::authoring::ResourceOperation::CreateImage {
                        spec,
                        image,
                    }) => ReadyOperation::Image(spec, self.prepared_image(client, image)?.clone()),
                    EditOperation::Resource(
                        crate::authoring::ResourceOperation::ReplaceImage { id, image },
                    ) => ReadyOperation::ReplaceImage(
                        id,
                        self.prepared_image(client, image)?.clone(),
                    ),
                    EditOperation::Resource(crate::authoring::ResourceOperation::RenameImage {
                        id,
                        name,
                        expected_name,
                    }) => ReadyOperation::RenameImage(id, name, expected_name),
                    EditOperation::Resource(crate::authoring::ResourceOperation::ApplyPlan {
                        plan,
                    }) => {
                        let (plan, valid) = self.resolve_plan(client, edit, plan)?;
                        // No public Source describes the intermediate state of a batch.
                        // An exact-source Plan therefore applies only at its start.
                        ReadyOperation::Plan(plan, valid && index == 0)
                    }
                })
            })
            .collect::<Result<Vec<_>>>();
        let resolved = match resolved {
            Ok(operations) => operations,
            Err(error) => {
                if !manual_request {
                    self.abort_edit(client, edit)?;
                }
                return Err(error);
            }
        };
        let mesh_transfer = resolved.iter().find_map(|op| match op {
            ReadyOperation::ManualMesh(p) => Some(p.report.clone()),
            _ => None,
        });
        let ids = self.apply_edit(client, edit, |candidate| {
            let workspace = workspace;
            candidate.set_authoring_limits(limits);
            let result = (|| {
                let mut ids = vec![];
                for operation in resolved {
                    match operation {
                        ReadyOperation::Scene(op) => ids.extend(op.apply(candidate)?),
                        ReadyOperation::Modeling(op) => op.apply(candidate)?,
                        ReadyOperation::ManualMesh(plan) => {
                            candidate.apply_manual_mesh_plan(plan)?;
                        }
                        ReadyOperation::MeshBinding(op) => ids.push(op.apply(candidate)?),
                        ReadyOperation::Parameter(op) => ids.push(op.apply(candidate)?),
                        ReadyOperation::Mesh(op) => ids.push(op.apply(candidate)?),
                        ReadyOperation::Image(spec, image) => {
                            ids.push(candidate.create_image_spec(spec, &image)?)
                        }
                        ReadyOperation::ReplaceImage(id, image) => {
                            candidate.replace_prepared_image(&id, &image)?;
                            ids.push(id);
                        }
                        ReadyOperation::RenameImage(id, name, expected) => {
                            let expected = expected.unwrap_or_else(|| {
                                candidate
                                    .candidate_document()
                                    .get_asset(&id)
                                    .map(|asset| asset.name.clone())
                                    .unwrap_or_default()
                            });
                            candidate.edit_resource(zipzi_sdk::ResourceEdit::PatchName {
                                id: id.clone(),
                                expected,
                                name,
                            })?;
                            ids.push(id);
                        }
                        ReadyOperation::Plan(plan, valid) => {
                            super::modeling::apply_plan(&plan, valid, candidate)?;
                            ids.extend(plan.report().affected_ids);
                        }
                    }
                }
                Ok(ids)
            })();
            drop(workspace);
            result
        })?;
        let entry = self.edit(client, edit)?;
        Ok(WriteResult {
            mesh_transfer,
            ids,
            source: SourceRef::Edit {
                session: entry.session,
                edit,
                base: entry.base,
                sequence: entry.sequence,
            },
        })
    }

    pub fn plan_authoring(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        intent: PlanIntent,
    ) -> Result<PlanSnapshot> {
        let intent = self.with_authoring_source(client, session, edit, |doc, _| {
            Ok(match intent {
                PlanIntent::MeshRename {
                    id,
                    expected_name,
                    name,
                } => zipzi_sdk::ModelingIntent::Scene(zipzi_sdk::SceneEdit::Patch {
                    id: id.clone(),
                    expected: zipzi_sdk::SceneFieldValue::Label(match expected_name {
                        Some(value) => value,
                        None => {
                            let value = doc
                                .get_mesh(&id)
                                .map(|mesh| mesh.name.as_str())
                                .unwrap_or_default();
                            self.check_bytes(
                                value.len().saturating_mul(128).saturating_add(4096),
                                0,
                                0,
                            )?;
                            value.to_owned()
                        }
                    }),
                    value: zipzi_sdk::SceneFieldValue::Label(name),
                    clear_glue_pairs: false,
                }),
                PlanIntent::ImageRename {
                    id,
                    expected_name,
                    name,
                } => zipzi_sdk::ModelingIntent::Resource(zipzi_sdk::ResourceEdit::PatchName {
                    id: id.clone(),
                    expected: match expected_name {
                        Some(value) => value,
                        None => {
                            let value = doc
                                .get_asset(&id)
                                .map(|asset| asset.name.as_str())
                                .unwrap_or_default();
                            self.check_bytes(
                                value.len().saturating_mul(128).saturating_add(4096),
                                0,
                                0,
                            )?;
                            value.to_owned()
                        }
                    },
                    name,
                }),
            })
        })?;
        let (plan, source, report) = self.plan_modeling(client, session, edit, intent)?;
        Ok(PlanSnapshot {
            plan,
            source,
            report,
        })
    }

    pub(super) fn with_authoring_source<T>(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        read: impl FnOnce(&zipzi_core::Document, SourceRef) -> Result<T>,
    ) -> Result<T> {
        self.client(client)?;
        let frozen;
        let (doc, source) = if let Some(edit) = edit {
            let entry = self.edit(client, edit)?;
            if entry.session != session {
                return Err(RuntimeError::InvalidReference);
            }
            (
                self.edit_document(client, edit)?,
                SourceRef::Edit {
                    session,
                    edit,
                    base: entry.base,
                    sequence: entry.sequence,
                },
            )
        } else {
            let entry = self.entry(session)?;
            let source = SourceRef::Formal {
                session,
                version: entry.info.version,
            };
            if let Some(sdk) = &entry.sdk {
                (sdk.document(), source)
            } else {
                frozen = self.source_snapshot(session)?;
                (frozen.snapshot.document(), frozen.source)
            }
        };
        read(doc, source)
    }

    pub fn parameter_snapshot(
        &self,
        client: ClientId,
        input: crate::authoring::ParameterQuery,
    ) -> Result<crate::authoring::ParameterBatch> {
        self.with_authoring_source(client, input.session, input.edit, |doc, source| {
            let ids = input
                .ids
                .as_deref()
                .unwrap_or_else(|| doc.parameter_order());
            if ids.len() > 256 {
                return Err(RuntimeError::Budget("parameter snapshot count"));
            }
            // Admit the complete selection and wire expansion before cloning.
            let mut bytes = 4096usize;
            for id in ids {
                let p = doc.get_parameter(id).ok_or_else(|| {
                    RuntimeError::Sdk(zipzi_sdk::SdkError {
                        code: "MISSING_PARAMETER".into(),
                        message: "Parameter does not exist".into(),
                        operation: "parameters.get",
                        object_ids: vec![id.clone()],
                        field_path: None,
                        expected_version: None,
                        actual_version: None,
                        referrers: Box::new([]),
                    })
                })?;
                bytes = bytes
                    .saturating_add(p.id.len())
                    .saturating_add(p.runtime_id.len())
                    .saturating_add(p.name.len())
                    .saturating_add(512);
                if bytes > 1 << 20 {
                    return Err(RuntimeError::Budget("parameter snapshot bytes"));
                }
            }
            self.check_bytes(bytes.saturating_mul(128), 0, 0)?;
            Ok(crate::authoring::ParameterBatch {
                source,
                parameters: ids
                    .iter()
                    .map(|id| doc.get_parameter(id).unwrap().clone())
                    .collect(),
            })
        })
    }

    /// Preserve the exact read source even when the selection is empty.
    pub fn mesh_batch(
        &self,
        client: ClientId,
        input: crate::authoring::MeshBatchQuery,
    ) -> Result<crate::authoring::MeshBatch> {
        self.with_authoring_source(client, input.session, input.edit, |_, source| {
            Ok(crate::authoring::MeshBatch {
                source,
                items: self.mesh_snapshots(client, input)?,
            })
        })
    }

    pub fn mesh_snapshots(
        &self,
        client: ClientId,
        input: crate::authoring::MeshBatchQuery,
    ) -> Result<Vec<MeshSnapshot>> {
        let bytes = self.mesh_read_bytes(client, input.session, input.edit, &input.ids)?;
        self.check_bytes(bytes, 0, 0)?;
        input
            .ids
            .iter()
            .map(|id| self.mesh_snapshot(client, input.session, input.edit, id))
            .collect()
    }

    pub fn mesh_snapshot(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        id: &str,
    ) -> Result<MeshSnapshot> {
        self.with_authoring_source(client, session, edit, |doc, source| {
            let mesh = doc.get_mesh(id).ok_or_else(|| {
                RuntimeError::Sdk(zipzi_sdk::SdkError {
                    code: "MISSING_MESH".into(),
                    message: "Mesh does not exist".into(),
                    operation: "meshes.get",
                    object_ids: vec![id.into()],
                    field_path: None,
                    expected_version: None,
                    actual_version: None,
                    referrers: Box::new([]),
                })
            })?;
            self.check_bytes(
                self.mesh_read_bytes(client, session, edit, &[id.to_owned()])?,
                0,
                0,
            )?;
            crate::metrics::copy_payload(
                mesh.vertex_ids.len() * std::mem::size_of::<u32>()
                    + (mesh.base_positions.len() + mesh.uvs.len())
                        * std::mem::size_of::<zipzi_core::Vec2>()
                    + mesh.triangles.len() * std::mem::size_of::<[u32; 3]>()
                    + mesh.meshing.as_ref().map_or(0, |m| {
                        m.boundary.len() * std::mem::size_of::<u32>()
                            + m.constraints.len()
                                * std::mem::size_of::<zipzi_core::MeshConstraint>()
                    }),
            );
            Ok(MeshSnapshot {
                source,
                id: mesh.id.clone(),
                name: mesh.name.clone(),
                geometry: zipzi_sdk::MeshGeometry {
                    vertex_ids: mesh.vertex_ids.clone(),
                    positions: mesh.base_positions.clone(),
                    uvs: mesh.uvs.clone(),
                    triangles: mesh.triangles.clone(),
                    meshing: mesh.meshing.clone(),
                },
            })
        })
    }
}
