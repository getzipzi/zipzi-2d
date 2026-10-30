//! Source-bound SDK plans. No transport-owned copy of authoring rules.
use super::*;
pub(super) struct PlanEntry {
    pub client: ClientId,
    pub source: SourceSnapshot,
    pub plan: Arc<zipzi_sdk::ModelingPlan>,
    pub bytes: usize,
}
impl Runtime {
    /// Read bounded rows from the owning client's immutable source-bound report.
    /// A stale source remains inspectable; applying the plan still checks source.
    pub fn deletion_report_page(
        &self,
        client: ClientId,
        plan: PlanId,
        section: zipzi_sdk::DeletionSection,
        offset: usize,
        limit: usize,
    ) -> Result<zipzi_sdk::DeletionPage> {
        self.client(client)?;
        let entry = self
            .plans
            .get(&plan)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        let zipzi_sdk::ModelingPlan::Deletion(review) = entry.plan.as_ref() else {
            return Err(RuntimeError::InvalidReference);
        };
        Ok(review.report_page(section, offset, limit)?)
    }
    /// Shared typed entry for local adapters and wire decoding. Admission is
    /// separate from SDK execution; a rejected batch does not advance source.
    pub fn apply_modeling(
        &mut self,
        client: ClientId,
        edit: EditId,
        commands: Vec<crate::modeling::ModelingCommand>,
    ) -> Result<()> {
        self.edit(client, edit)?;
        if commands.len() > 1024 {
            return Err(RuntimeError::Budget("modeling commands"));
        }
        self.apply_authoring(
            client,
            edit,
            commands
                .into_iter()
                .map(|operation| crate::authoring::EditOperation::Modeling {
                    operation: Box::new(operation),
                })
                .collect(),
        )
        .map(|_| ())
    }

    /// Preparation is part of this write, so a preparation failure poisons the
    /// candidate just like a failed SDK edit. Independent preparation is still
    /// a separate operation and has no effect on an Edit.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_rgba_image(
        &mut self,
        client: ClientId,
        edit: EditId,
        id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        replace: bool,
    ) -> Result<()> {
        self.write_image(client, edit, id, name, replace, |budget| {
            zipzi_project::PreparedImageContent::from_rgba_bytes(width, height, rgba, budget)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_rgba_blob(
        &mut self,
        client: ClientId,
        edit: EditId,
        id: String,
        name: String,
        width: u32,
        height: u32,
        blob: BlobId,
        replace: bool,
    ) -> Result<()> {
        let bytes = self.blob(client, blob)?;
        self.write_image(client, edit, id, name, replace, |budget| {
            zipzi_project::PreparedImageContent::from_rgba(width, height, bytes, budget)
        })
    }

    fn write_image(
        &mut self,
        client: ClientId,
        edit: EditId,
        id: String,
        name: String,
        replace: bool,
        prepare: impl FnOnce(
            &zipzi_project::ImageMemoryBudget,
        ) -> std::result::Result<
            zipzi_project::PreparedImageContent,
            zipzi_core::Status,
        >,
    ) -> Result<()> {
        let budget = self.image_budget.clone();
        self.apply_edit(client, edit, |candidate| {
            let image = prepare(&budget).map_err(|s| zipzi_sdk::SdkError {
                code: s.code.into(),
                message: s.message.into(),
                operation: "prepare_rgba",
                object_ids: vec![id.clone()],
                field_path: None,
                expected_version: None,
                actual_version: None,
                referrers: Box::new([]),
            })?;
            if replace {
                candidate.replace_prepared_image(&id, &image)
            } else {
                candidate.create_prepared_image(id, name, &image)
            }
        })
    }

    pub fn plan_modeling(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        intent: zipzi_sdk::ModelingIntent,
    ) -> Result<(PlanId, SourceRef, zipzi_sdk::ModelingPlanReport)> {
        self.client(client)?;
        if self.plans.values().filter(|p| p.client == client).count()
            >= self.limits.plans_per_client
        {
            return Err(RuntimeError::Budget("plans"));
        }
        let mut limits = self.ready(session)?.authoring_limits();
        let snapshot_bytes = if let Some(edit) = edit {
            if self.edit(client, edit)?.session != session {
                return Err(RuntimeError::InvalidReference);
            }
            self.edit_document(client, edit)?.estimated_content_bytes()
        } else {
            self.document(session)?.estimated_content_bytes()
        };
        let remaining = self
            .limits
            .plan_bytes
            .saturating_sub(self.plans.values().map(|p| p.bytes).sum());
        // Account the captured source, generated records and plan input before
        // allocating the snapshot/domain plan. Conversion costs are added below.
        let base_bytes = snapshot_bytes
            .saturating_mul(3)
            .saturating_add(serialized_bytes(&intent)?.saturating_mul(4));
        if base_bytes > remaining {
            return Err(RuntimeError::Budget("plan bytes"));
        }
        self.check_bytes(base_bytes, 0, 0)?;
        limits.max_conversion_bytes = limits
            .max_conversion_bytes
            .min((remaining - base_bytes) / 2);
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let plan = source
            .snapshot
            .plan_modeling(intent, limits, &Default::default())?;
        let report = plan.report();
        let bytes = base_bytes.saturating_add(
            report
                .cost
                .map(|c| c.payload_bytes.saturating_add(c.temporary_bytes))
                .unwrap_or(0),
        );
        if bytes > remaining {
            return Err(RuntimeError::Budget("plan bytes"));
        }
        self.check_bytes(bytes, 0, 0)?;
        let id = PlanId::from_parts(self.instance, self.serial());
        let identity = source.source;
        self.plans.insert(
            id,
            PlanEntry {
                client,
                source,
                plan: Arc::new(plan),
                bytes,
            },
        );
        Ok((id, identity, report))
    }
    pub fn apply_modeling_plan(
        &mut self,
        client: ClientId,
        edit: EditId,
        plan: PlanId,
    ) -> Result<()> {
        self.apply_authoring(
            client,
            edit,
            vec![crate::authoring::EditOperation::Resource(
                crate::authoring::ResourceOperation::ApplyPlan { plan },
            )],
        )
        .map(|_| ())
    }
    pub(super) fn resolve_plan(
        &self,
        client: ClientId,
        edit: EditId,
        plan: PlanId,
    ) -> Result<(Arc<zipzi_sdk::ModelingPlan>, bool)> {
        self.client(client)?;
        let p = self
            .plans
            .get(&plan)
            .ok_or(RuntimeError::InvalidReference)?;
        if p.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        let entry = self.edit(client, edit)?;
        let valid = match p.source.source {
            SourceRef::Formal { session, version } => {
                entry.session == session && entry.base == version && entry.sequence == 0
            }
            SourceRef::Edit {
                session,
                edit: original,
                base,
                sequence,
            } => {
                entry.session == session
                    && edit == original
                    && entry.base == base
                    && entry.sequence == sequence
            }
        };
        Ok((p.plan.clone(), valid))
    }
    pub fn release_modeling_plan(&mut self, client: ClientId, plan: PlanId) -> Result<()> {
        self.client(client)?;
        if let Some(p) = self.plans.get(&plan) {
            if p.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            self.plans.remove(&plan);
            self.collect_image_resources();
        }
        Ok(())
    }
    pub fn modeling_plan_count(&self) -> usize {
        self.plans.len()
    }
}

pub(super) fn apply_plan(
    plan: &zipzi_sdk::ModelingPlan,
    valid: bool,
    candidate: &mut zipzi_sdk::EditSession<'_>,
) -> std::result::Result<(), zipzi_sdk::SdkError> {
    if !valid {
        return Err(zipzi_sdk::SdkError {
            code: "STALE_PLAN".into(),
            message: "Plan source no longer matches this candidate".into(),
            operation: "apply_modeling_plan",
            object_ids: vec![],
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        });
    }
    plan.apply(candidate)
}
