use super::*;
use crate::authoring::{EditOperation, MeshSnapshot, PlanIntent, PlanSnapshot, WriteResult};

enum ReadyOperation {
    Mesh(zipzi_sdk::MeshOperation),
    Image(zipzi_sdk::ImageCreateSpec, zipzi_sdk::PreparedImageContent),
    ReplaceImage(String, zipzi_sdk::PreparedImageContent),
    Plan(Arc<zipzi_sdk::ModelingPlan>, bool),
}

impl Runtime {
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
        if operations.len() > 1024 {
            return Err(RuntimeError::Budget("authoring operations"));
        }
        let resolved = operations
            .into_iter()
            .enumerate()
            .map(|(index, operation)| {
                Ok(match operation {
                    EditOperation::Mesh { operation } => ReadyOperation::Mesh(operation),
                    EditOperation::CreateImage { spec, image } => {
                        ReadyOperation::Image(spec, self.prepared_image(client, image)?.clone())
                    }
                    EditOperation::ReplaceImage { id, image } => ReadyOperation::ReplaceImage(
                        id,
                        self.prepared_image(client, image)?.clone(),
                    ),
                    EditOperation::ApplyPlan { plan } => {
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
                self.abort_edit(client, edit)?;
                return Err(error);
            }
        };
        let ids = self.apply_edit(client, edit, |candidate| {
            let mut ids = vec![];
            for operation in resolved {
                match operation {
                    ReadyOperation::Mesh(op) => ids.push(op.apply(candidate)?),
                    ReadyOperation::Image(spec, image) => {
                        ids.push(candidate.create_image_spec(spec, &image)?)
                    }
                    ReadyOperation::ReplaceImage(id, image) => {
                        candidate.replace_prepared_image(&id, &image)?;
                        ids.push(id);
                    }
                    ReadyOperation::Plan(plan, valid) => {
                        super::modeling::apply_plan(&plan, valid, candidate)?;
                        ids.extend(plan.report().affected_ids);
                    }
                }
            }
            Ok(ids)
        })?;
        let entry = self.edit(client, edit)?;
        Ok(WriteResult {
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
        let intent = match intent {
            PlanIntent::MeshRename {
                id,
                expected_name,
                name,
            } => zipzi_sdk::ModelingIntent::Scene(zipzi_sdk::SceneEdit::Patch {
                id,
                expected: zipzi_sdk::SceneFieldValue::Label(expected_name),
                value: zipzi_sdk::SceneFieldValue::Label(name),
                clear_glue_pairs: false,
            }),
            PlanIntent::ImageRename {
                id,
                expected_name,
                name,
            } => zipzi_sdk::ModelingIntent::Resource(zipzi_sdk::ResourceEdit::PatchName {
                id,
                expected: expected_name,
                name,
            }),
        };
        let (plan, source, report) = self.plan_modeling(client, session, edit, intent)?;
        Ok(PlanSnapshot {
            plan,
            source,
            report,
        })
    }

    pub fn mesh_snapshot(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        id: &str,
    ) -> Result<MeshSnapshot> {
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
        // A conservative bound for owned records AND their possible wire
        // expansion, before cloning. Large queries need later paged/buffer APIs.
        let bytes = mesh
            .vertex_ids
            .len()
            .saturating_mul(128)
            .saturating_add(mesh.triangles.len().saturating_mul(64))
            .saturating_add(mesh.id.len())
            .saturating_add(mesh.name.len())
            .saturating_add(4096);
        if bytes > 1 << 20 {
            return Err(RuntimeError::Budget("mesh snapshot"));
        }
        self.check_bytes(bytes.saturating_mul(128), 0, 0)?;
        Ok(MeshSnapshot {
            source,
            id: mesh.id.clone(),
            name: mesh.name.clone(),
            geometry: zipzi_sdk::MeshGeometry {
                vertex_ids: mesh.vertex_ids.clone(),
                positions: mesh.base_positions.clone(),
                uvs: mesh.uvs.clone(),
                triangles: mesh.triangles.clone(),
            },
        })
    }
}
