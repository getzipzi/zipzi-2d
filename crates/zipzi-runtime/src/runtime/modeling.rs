//! Source-bound SDK plans. No transport-owned copy of authoring rules.
use super::*;
pub(super) struct PlanEntry {
    pub client: ClientId,
    pub source: SourceSnapshot,
    pub plan: Arc<zipzi_sdk::ModelingPlan>,
    pub bytes: usize,
}
impl Runtime {
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
        let id = PlanId {
            runtime: self.instance,
            serial: self.serial(),
        };
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
        let sdk_plan = p.plan.clone();
        self.apply_edit(client, edit, |candidate| {
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
            sdk_plan.apply(candidate)
        })
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
