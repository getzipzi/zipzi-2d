//! A confirmation view owns the exact Runtime Draft it displays. Cloning a UI
//! view shares that lifecycle; it never clones a Session or replays commands.
use crate::runtime_host::{HostedDraft, HostedSession};
use std::sync::{Arc, Mutex};
use zipzi_sdk::{Commands, DraftSource, Publication, Snapshot, SourcedSnapshot, Status, Version};

#[derive(Clone)]
pub struct EditPreview<T> {
    candidate: Arc<Mutex<HostedDraft>>,
    snapshot: SourcedSnapshot,
    baseline: Snapshot,
    pub outcome: T,
}
impl<T: std::fmt::Debug> std::fmt::Debug for EditPreview<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditPreview")
            .field("source", &self.snapshot.source)
            .field("outcome", &self.outcome)
            .finish()
    }
}
impl<T> EditPreview<T> {
    pub(crate) fn convert(
        source: &crate::runtime_host::HostedSource,
        label: &str,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
        run: impl FnOnce(&mut Commands<'_>, &zipzi_core::state::ConversionControl) -> Result<T, Status>
        + Send
        + 'static,
    ) -> Result<Self, Status>
    where
        T: Send + 'static,
    {
        control.check()?;
        let mut candidate = source.begin_draft(label)?;
        let control = control.clone();
        let outcome = candidate.worker_batch(
            zipzi_runtime::TaskBudget {
                workspace_bytes: limits.max_workspace_bytes,
                result_bytes: 0,
            },
            move |commands, cancel| {
                let control = control.linked(&cancel.conversion_control());
                let result = run(commands, &control)?;
                control.check()?;
                cancel.check()?;
                Ok(result)
            },
        )?;
        Self::from_candidate(candidate, source.captured.snapshot.clone(), outcome)
    }
    pub(crate) fn prepare(
        session: &HostedSession,
        label: &str,
        expected: Option<Version>,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> Result<Self, Status> {
        let captured = session.sourced_snapshot();
        let version = match captured.source {
            zipzi_sdk::QuerySource::Published(v) => v,
            _ => unreachable!("hosted published source"),
        };
        if expected.is_some_and(|expected| expected != version) {
            return Err(Status::error("STALE_SOURCE", "Confirmation source changed"));
        }
        let mut candidate = session.begin_draft(label, Some(version))?;
        let outcome = candidate.batch(run)?;
        Self::from_candidate(candidate, captured.snapshot, outcome)
    }
    pub(crate) fn from_candidate(
        candidate: HostedDraft,
        baseline: Snapshot,
        outcome: T,
    ) -> Result<Self, Status> {
        let snapshot = candidate.read_snapshot()?;
        Ok(Self {
            candidate: Arc::new(Mutex::new(candidate)),
            snapshot,
            baseline,
            outcome,
        })
    }
    pub fn source(&self) -> DraftSource {
        match self.snapshot.source {
            zipzi_sdk::QuerySource::Draft(source) => source,
            _ => unreachable!("confirmation owns a Draft"),
        }
    }
    pub fn version(&self) -> Version {
        self.source().base
    }
    pub fn snapshot(&self) -> &SourcedSnapshot {
        &self.snapshot
    }
    pub(crate) fn baseline(&self) -> &Snapshot {
        &self.baseline
    }
    pub fn is_noop(&self) -> bool {
        self.snapshot.snapshot.content_id() == self.baseline.content_id()
    }
    pub(crate) fn publish(&self, session: &mut HostedSession) -> Result<Publication, Status> {
        self.candidate
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .commit_to(session)
    }
}

/// Value captured by an inspector from one published source. Stable identity and
/// the full Version replace session-bound handles and field-epoch registries.
#[derive(Debug, Clone)]
pub struct FieldCapture<T> {
    pub source: Version,
    pub id: String,
    pub owner: Option<String>,
    pub value: T,
}

impl crate::OperationResult {
    /// Nested identities belong to shared roots in WriteSet. Derive native
    /// selection suggestions from just those touched roots and records; they
    /// are presentation hints, never a second publication/change detector.
    pub(super) fn with_preview<T>(mut self, preview: &EditPreview<T>) -> Self {
        use crate::{ObjectKind as K, ObjectRef};
        use zipzi_core::state::{RecordDomain as D, RootDomain as R};
        let before = preview.baseline().document();
        let after = preview.snapshot().snapshot.document();
        let writes = &self.receipt.writes;
        let mut created = Vec::new();
        let mut add = |kind, id: &str| {
            if !before.contains_id(id) {
                created.push(ObjectRef::new(kind, id));
            }
        };
        if writes.root_changed(R::DisplayInfo) {
            let display = after.display_info();
            for group in display.parameter_groups.iter().flatten() {
                add(K::ParameterGroup, &group.id);
                if let Some(parent) = &group.parent_id {
                    if before
                        .parameter_group(&group.id)
                        .is_some_and(|old| old.parent_id != group.parent_id)
                    {
                        self.locations
                            .push(ObjectRef::new(K::ParameterGroup, parent));
                    }
                }
            }
            for set in display.combined_parameters.iter().flatten() {
                add(K::CdiCombinedSet, &set.id);
                for member in &set.members {
                    add(K::CdiCombinedMember, &member.id);
                }
            }
        }
        if writes.root_changed(R::Model3Settings) {
            let settings = after.model3_settings();
            for group in settings.groups.iter().flatten() {
                add(K::ModelParameterGroup, &group.id);
                for member in &group.parameters {
                    add(K::ModelParameterMember, &member.id);
                }
            }
            for area in settings.hit_areas.iter().flatten() {
                add(K::ModelHitArea, &area.id);
            }
        }
        if writes.root_changed(R::Pose) {
            if let Some(pose) = after.pose() {
                add(K::Pose, &pose.id);
                for group in &pose.groups {
                    add(K::PoseGroup, &group.id);
                    for entry in &group.entries {
                        add(K::PoseEntry, &entry.id);
                        for link in entry.links.iter().flatten() {
                            add(K::PoseLink, &link.id);
                        }
                    }
                }
            }
        }
        if writes.root_changed(R::Physics) {
            if let Some(physics) = after.physics() {
                add(K::Physics, &physics.id);
                for group in &physics.data.settings {
                    add(K::PhysicsGroup, &group.id);
                    for input in &group.inputs {
                        add(K::PhysicsInput, &input.id);
                    }
                    for output in &group.outputs {
                        add(K::PhysicsOutput, &output.id);
                    }
                    for node in &group.vertices {
                        add(K::PhysicsNode, &node.id);
                    }
                }
            }
        }
        if writes.root_changed(R::MotionGroups) {
            for group in after.motion_groups() {
                add(K::MotionGroup, &group.id);
                for entry in &group.entries {
                    add(K::MotionRegistration, &entry.id);
                }
            }
        }
        if writes.root_changed(R::MotionRoots) {
            for entry in after.motion_roots() {
                add(K::MotionRegistration, &entry.id);
            }
        }
        for record in writes.records() {
            match record.domain {
                D::Expression => {
                    if let Some(expression) = after.expression(&record.id) {
                        for entry in &expression.entries {
                            add(K::ExpressionEntry, &entry.id);
                        }
                    }
                }
                D::Motion => {
                    if let Some(motion) = after.motion(&record.id) {
                        for track in &motion.tracks {
                            add(K::MotionTrack, &track.id);
                            for key in track.keys() {
                                add(K::MotionKey, &key.id);
                            }
                        }
                        for event in &motion.events {
                            add(K::MotionEvent, &event.id);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.locations.retain(|object| seen.insert(object.clone()));
        for object in created {
            if !self.suggested_selection.contains(&object) {
                self.suggested_selection.push(object.clone());
            }
            if !self.locations.contains(&object) {
                self.locations.push(object);
            }
        }
        self
    }
}
