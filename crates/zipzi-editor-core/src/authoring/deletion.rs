//! All hierarchy and viewport deletion entries use one Runtime-owned plan.
use super::*;
use anyhow::{ensure, Result};
use std::sync::Arc;
use zipzi_runtime::{JobId, JobValue, PlanId, Work};
use zipzi_sdk::{DeletionIntent, DeletionPage, DeletionSection, DeletionSummary};

struct PlanLease {
    host: crate::RuntimeHost,
    id: PlanId,
}
impl Drop for PlanLease {
    fn drop(&mut self) {
        let _ = self
            .host
            .write()
            .release_modeling_plan(self.host.client_id(), self.id);
    }
}
#[derive(Clone)]
pub struct TreeDeletionPlan {
    lease: Arc<PlanLease>,
    version: zipzi_sdk::Version,
    preview_revision: Option<u64>,
    summary: DeletionSummary,
}
impl std::fmt::Debug for TreeDeletionPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeDeletionPlan")
            .field("summary", &self.summary)
            .finish()
    }
}
impl TreeDeletionPlan {
    pub fn summary(&self) -> &DeletionSummary {
        &self.summary
    }
    pub fn page(
        &self,
        section: DeletionSection,
        offset: usize,
        limit: usize,
    ) -> Result<DeletionPage> {
        Ok(self.lease.host.read().deletion_report_page(
            self.lease.host.client_id(),
            self.lease.id,
            section,
            offset,
            limit,
        )?)
    }
    pub fn removed_ids(&self) -> Vec<String> {
        let mut ids = vec![];
        let mut offset = 0;
        while let Ok(page) = self.page(DeletionSection::Removed, offset, 256) {
            ids.extend(page.rows.into_iter().filter_map(|row| match row {
                zipzi_sdk::DeletionRow::Removal { removal } => Some(removal.object.id),
                _ => None,
            }));
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        ids
    }
    pub fn valid_for(&self, document: &Document) -> bool {
        self.version == document.session.version()
            && self.lease.host.same_host(&document.session.host)
            && self
                .preview_revision
                .is_none_or(|revision| revision == document.preview_revision())
    }
}
#[derive(Clone)]
pub struct TreeDeletionCancellation {
    host: crate::RuntimeHost,
    job: JobId,
}
impl TreeDeletionCancellation {
    pub fn cancel(&self) {
        let _ = self
            .host
            .write()
            .cancel_job(self.host.client_id(), self.job);
    }
}
pub struct TreeDeletionTask {
    cancellation: TreeDeletionCancellation,
    work: Option<Work>,
    preview_revision: Option<u64>,
}
impl TreeDeletionTask {
    pub fn cancellation(&self) -> TreeDeletionCancellation {
        self.cancellation.clone()
    }
    /// Publish completion and release the Job before returning to any weak view callback.
    pub fn run(mut self) -> Result<TreeDeletionPlan> {
        let completion = self.work.take().expect("deletion task runs once").run();
        let host = &self.cancellation.host;
        host.write().complete(completion);
        let result = host
            .read()
            .job_info(host.client_id(), self.cancellation.job)?
            .result;
        host.write()
            .release_job(host.client_id(), self.cancellation.job)?;
        let JobValue::Planned(snapshot) =
            result.ok_or_else(|| anyhow::anyhow!("Deletion job did not complete"))??
        else {
            anyhow::bail!("Unexpected deletion job result")
        };
        Ok(TreeDeletionPlan {
            lease: Arc::new(PlanLease {
                host: host.clone(),
                id: snapshot.plan,
            }),
            version: snapshot.source.version(),
            preview_revision: self.preview_revision,
            summary: snapshot.report.deletion.clone().expect("deletion report"),
        })
    }
}
impl Drop for TreeDeletionTask {
    fn drop(&mut self) {
        if self.work.is_some() {
            let host = &self.cancellation.host;
            let _ = host
                .write()
                .cancel_job(host.client_id(), self.cancellation.job);
            // A taken Work must still return its cancelled completion.
            if let Some(work) = self.work.take() {
                let completion = work.run();
                host.write().complete(completion);
            }
            let _ = host
                .write()
                .release_job(host.client_id(), self.cancellation.job);
        }
    }
}
impl Document {
    pub fn deletion_intent(&self, objects: &[ObjectRef]) -> Result<DeletionIntent> {
        let mut request = zipzi_sdk::DeletionRequest::default();
        for object in objects {
            if object.kind == crate::ObjectKind::GlueBinding {
                ensure!(
                    self.session
                        .read()
                        .document()
                        .get_glue(&object.id)
                        .is_some_and(|g| g.binding.is_some()),
                    "Missing Glue binding"
                );
                request.roots.push(zipzi_sdk::DeletionTarget::Record(
                    zipzi_sdk::DeletionObject {
                        kind: zipzi_sdk::DeletionKind::GlueBinding,
                        id: object.id.clone(),
                        owner_id: None,
                    },
                ));
            } else {
                let kind = object
                    .kind
                    .sdk_kind()
                    .ok_or_else(|| anyhow::anyhow!("Unsupported deletion target"))?;
                let handle = self.session.handle(kind, &object.id)?;
                request
                    .roots
                    .extend(self.session.read().deletion_request(&[handle])?.roots);
            }
        }
        Ok(DeletionIntent {
            request,
            ..Default::default()
        })
    }

    pub fn start_tree_deletion(&self, mut intent: DeletionIntent) -> Result<TreeDeletionTask> {
        self.ensure_authoring_idle()?;
        let current = intent
            .request
            .reparent
            .values()
            .any(|s| *s == zipzi_sdk::ReparentStrategy::PreserveEditState);
        if current || intent.request.conversion_context.is_none() {
            intent.request.conversion_context = Some(self.preservation_context());
        }
        let host = self.runtime_host();
        let job = host.write().request_modeling_plan(
            host.client_id(),
            self.runtime_session_id(),
            None,
            zipzi_sdk::ModelingIntent::Deletion(intent),
        )?;
        let work = host.dispatch_job(job)?;
        Ok(TreeDeletionTask {
            cancellation: TreeDeletionCancellation { host, job },
            work: Some(work),
            preview_revision: current.then(|| self.preview_revision()),
        })
    }
    /// Synchronous convenience for non-UI callers; menus use no planning work.
    pub fn plan_tree_deletion(
        &self,
        _projection: PaletteProjection,
        objects: &[ObjectRef],
    ) -> Result<TreeDeletionPlan> {
        self.start_tree_deletion(self.deletion_intent(objects)?)?
            .run()
    }
    /// Explicitly unlock one reported source through the same Runtime session.
    /// Business identities need not opt into scene visibility/lock tree controls.
    pub fn unlock_deletion_source(
        &mut self,
        plan: &TreeDeletionPlan,
        source_id: &str,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        ensure!(
            plan.valid_for(self),
            "Deletion source changed; review a new plan"
        );
        let mut offset = 0;
        let mut reported = false;
        loop {
            let page = plan.page(DeletionSection::Locks, offset, 256)?;
            if page.rows.iter().any(|row|matches!(row,zipzi_sdk::DeletionRow::Lock {blocker} if blocker.source_id==source_id)) {reported=true;break;}
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        ensure!(reported, "This lock source is not in the reviewed plan");
        let mut state = self.model().editor_state().clone();
        let flags = state
            .objects
            .get_mut(source_id)
            .ok_or_else(|| anyhow::anyhow!("Lock source no longer exists"))?;
        ensure!(flags.locked, "Lock source changed; review a new plan");
        flags.locked = false;
        let (_, receipt) =
            self.session
                .edit("Unlock deletion source", Some(plan.version), |edit| {
                    edit.replace_editor_state(state)
                })?;
        self.session.acknowledge_receipts();
        if receipt.changed {
            self.invalidate_projection(false);
            self.pristine_blank = None;
        }
        Ok(self.authoring_result(receipt))
    }
    pub fn execute_tree_deletion(&mut self, plan: &TreeDeletionPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        ensure!(
            plan.valid_for(self),
            "Deletion source changed; review a new plan"
        );
        let previous_index = self.object_index();
        let previous_selection = self.context.selections.clone();
        let receipt = self
            .session
            .execute_modeling_plan(plan.lease.id, plan.version)?;
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, true);
            let index = self.object_index();
            self.context.reconcile(&index);
            if self.context.selections.is_empty() {
                if let Some(fallback) = previous_selection
                    .iter()
                    .rev()
                    .find_map(|object| deletion_selection_fallback(&previous_index, &index, object))
                {
                    self.select_object(fallback)?;
                }
            }
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn unlock_deletion_source(
        &mut self,
        id: crate::DocumentId,
        plan: &TreeDeletionPlan,
        source_id: &str,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Unlock deletion source")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|d| d.unlock_deletion_source(plan, source_id))
    }
    pub fn execute_tree_deletion(
        &mut self,
        id: crate::DocumentId,
        plan: &TreeDeletionPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Object deletion")?;
        let previous_index = self.document().unwrap().object_index();
        let inspected = self.active_inspection().and_then(|i| i.object());
        let result = self
            .active_session_mut()
            .unwrap()
            .execute_authoring(|d| d.execute_tree_deletion(plan))?;
        if result.receipt.changed && self.active_inspection().and_then(|i| i.object()).is_none() {
            if let Some(object) = inspected.and_then(|object| {
                deletion_selection_fallback(
                    &previous_index,
                    &self.document().unwrap().object_index(),
                    &object,
                )
            }) {
                self.inspect_authoring_object(id, object)?;
            }
        }
        Ok(result)
    }
}

// Selection fallback never navigates to another workspace or activates playback.
fn deletion_selection_fallback(
    previous: &crate::ObjectIndex,
    current: &crate::ObjectIndex,
    object: &ObjectRef,
) -> Option<ObjectRef> {
    let parents = |object: &ObjectRef| {
        previous
            .outgoing(object)
            .filter(|r| {
                matches!(
                    r.kind,
                    crate::RelationKind::PartOwner
                        | crate::RelationKind::DeformParent
                        | crate::RelationKind::EntryOf
                        | crate::RelationKind::ParameterGroup
                        | crate::RelationKind::BindingTarget
                        | crate::RelationKind::BlendTarget
                )
            })
            .map(|r| r.target.clone())
            .chain(
                previous
                    .incoming(object)
                    .filter(|r| r.kind == crate::RelationKind::Contains)
                    .map(|r| r.source.clone()),
            )
            .filter(|r| r.kind != crate::ObjectKind::Document)
            .collect::<Vec<_>>()
    };
    let nearest_sibling = || {
        let siblings = parents(object);
        let origin = previous
            .objects
            .iter()
            .position(|o| o.reference == *object)?;
        for distance in 1..previous.objects.len() {
            for slot in [origin.checked_add(distance), origin.checked_sub(distance)]
                .into_iter()
                .flatten()
            {
                if slot >= previous.objects.len() {
                    continue;
                }
                let candidate = &previous.objects[slot];
                if candidate.reference.kind == object.kind
                    && current.get(&candidate.reference).is_some()
                    && parents(&candidate.reference) == siblings
                {
                    return Some(candidate.reference.clone());
                }
            }
        }
        None
    };
    // Business lists keep their current container useful after removing a row.
    if !object.kind.is_edit_target() {
        if let Some(sibling) = nearest_sibling() {
            return Some(sibling);
        }
    }
    let mut queue: std::collections::VecDeque<_> = parents(object).into();
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = queue.pop_front() {
        if !seen.insert(parent.clone()) {
            continue;
        }
        if current.get(&parent).is_some() {
            return Some(parent);
        }
        queue.extend(parents(&parent));
    }
    nearest_sibling()
}
