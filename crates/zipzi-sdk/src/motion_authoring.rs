//! Typed Motion library planning uses the same domain rules for every SDK client.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use zipzi_core::document::{MotionContainer, MotionEdit, MotionPlan};
#[derive(Debug, Clone)]
pub struct MotionEditPlan {
    version: Version,
    domain: MotionPlan,
}
impl MotionEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
    pub fn migrated_to_root(&self) -> &[String] {
        self.domain.migrated_to_root()
    }
}
#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct MotionCapabilities {
    /// Read this object while its identity/context remains valid.
    pub inspect: CapabilityState,
    /// Modify typed fields through a validated native intent/address.
    pub patch: CapabilityState,
    /// Duplicate the selected object while allocating fresh owned identities.
    pub duplicate: CapabilityState,
    /// Delete the selected object subject to native reference and lock rules.
    pub delete: CapabilityState,
    /// Move the selected object between compatible native owners.
    pub move_object: CapabilityState,
    /// Native preview eligibility; host preview API availability is advertised separately.
    pub preview: CapabilityState,
    /// Create owned child rows in this object.
    pub create_child: CapabilityState,
    /// Change ordered sibling membership without changing identity.
    pub reorder: CapabilityState,
    /// Repair an explicitly unresolved native reference.
    pub repair: CapabilityState,
    /// Edit the owning clip content, preserving stable track/key/event identities.
    pub edit_content: CapabilityState,
}
#[derive(Debug, Clone)]
pub struct MotionCollection {
    pub container: MotionContainer,
    pub entries: Vec<ObjectHandle>,
    pub allows_empty: bool,
}
impl AuthoringSession {
    pub fn plan_motion_edit(&self, intent: MotionEdit) -> Result<MotionEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_motion_edit(intent)
            .map_err(|s| SdkError::from_status(s, "plan_motion_edit", vec![]))?;
        Ok(MotionEditPlan {
            version: self.version(),
            domain,
        })
    }
    pub fn execute_motion_plan(&mut self, plan: &MotionEditPlan) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit motion library", Some(plan.version), |e| {
            e.apply_motion_plan(plan)
        })?;
        Ok(receipt)
    }
    pub fn motion_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<MotionCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let supported = matches!(
            handle.kind(),
            ObjectKind::Motion
                | ObjectKind::MotionGroup
                | ObjectKind::MotionRegistration
                | ObjectKind::MotionTrack
                | ObjectKind::MotionKey
                | ObjectKind::MotionEvent
        );
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if self.document().motion_unlocked(handle.id()).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let preview = if matches!(
            handle.kind(),
            ObjectKind::Motion | ObjectKind::MotionRegistration
        ) {
            CapabilityState::Available
        } else {
            CapabilityState::Unsupported {
                code: "PREVIEW_CLIP_OR_REGISTRATION",
            }
        };
        let movement = if matches!(
            handle.kind(),
            ObjectKind::MotionRegistration
                | ObjectKind::MotionTrack
                | ObjectKind::MotionKey
                | ObjectKind::MotionEvent
        ) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "USE_DOMAIN_ORDER",
            }
        };
        let doc = self.document();
        let descendant_locked = doc
            .editor_state()
            .objects
            .iter()
            .filter(|(_, flags)| flags.locked)
            .any(|(id, _)| match handle.kind() {
                ObjectKind::Motion => {
                    doc.get_motion_track(id)
                        .is_some_and(|(c, _)| c.id == handle.id())
                        || doc
                            .get_motion_key(id)
                            .is_some_and(|(c, _, _)| c.id == handle.id())
                        || doc
                            .get_motion_event(id)
                            .is_some_and(|(c, _)| c.id == handle.id())
                }
                ObjectKind::MotionTrack => doc
                    .get_motion_key(id)
                    .is_some_and(|(_, t, _)| t.id == handle.id()),
                ObjectKind::MotionGroup => doc
                    .get_motion_registration(id)
                    .is_some_and(|(c, _)| c == MotionContainer::Group(handle.id().into())),
                _ => false,
            });
        let subtree = if descendant_locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        let child = if matches!(handle.kind(), ObjectKind::Motion | ObjectKind::MotionGroup) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "NO_CHILD_COLLECTION",
            }
        };
        let repair = if matches!(
            handle.kind(),
            ObjectKind::MotionRegistration | ObjectKind::MotionTrack
        ) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "NO_REFERENCE_SLOT",
            }
        };
        let content = if doc
            .get_motion_track(handle.id())
            .is_some_and(|(_, t)| doc.motion_track_disabled_reason(&t.target).is_some())
        {
            CapabilityState::Unavailable {
                code: "CONFIGURE_TRACK_TARGET",
            }
        } else {
            state.clone()
        };
        Ok(MotionCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            duplicate: subtree.clone(),
            delete: subtree,
            move_object: movement,
            preview,
            create_child: child,
            reorder: if matches!(
                handle.kind(),
                ObjectKind::MotionGroup | ObjectKind::MotionTrack | ObjectKind::MotionRegistration
            ) {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "USE_SEMANTIC_TIME_OPERATION",
                }
            },
            repair,
            edit_content: content,
        })
    }
    pub fn motion_collection(
        &self,
        container: MotionContainer,
    ) -> Result<MotionCollection, SdkError> {
        let entries = match &container {
            MotionContainer::Root => self.document().motion_roots(),
            MotionContainer::Group(id) => {
                self.handle(ObjectKind::MotionGroup, id)?;
                &self.document().get_motion_group(id).unwrap().entries
            }
        }
        .iter()
        .map(|e| self.handle(ObjectKind::MotionRegistration, &e.id))
        .collect::<Result<_, _>>()?;
        Ok(MotionCollection {
            container,
            entries,
            allows_empty: true,
        })
    }
}
impl EditSession<'_> {
    pub fn apply_motion_plan(&mut self, plan: &MotionEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_motion_plan",
            "Replan against current session",
        )?;
        self.apply_authoring_change("apply_motion_plan", "", false, |document| {
            document.apply_motion_edit(&plan.domain)
        })
    }
    pub fn edit_motion(&mut self, intent: MotionEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_motion")?;
        let plan = self
            .candidate_document()
            .plan_motion_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_motion", vec![])))?;
        self.apply_authoring_change("edit_motion", "", false, |document| {
            document.apply_motion_edit(&plan)
        })
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentEditPlan {
    version: Version,
    domain: zipzi_core::document::AttachmentPlan,
}
impl AttachmentEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
}
impl AuthoringSession {
    pub fn plan_attachment_edit(
        &self,
        intent: zipzi_core::document::AttachmentEdit,
    ) -> Result<AttachmentEditPlan, SdkError> {
        Ok(AttachmentEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_attachment_edit(intent)
                .map_err(|s| SdkError::from_status(s, "plan_attachment_edit", vec![]))?,
        })
    }
    pub fn execute_attachment_plan(
        &mut self,
        plan: &AttachmentEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit attachment", Some(plan.version), |e| {
            e.apply_attachment_plan(plan)
        })?;
        Ok(receipt)
    }
}
impl EditSession<'_> {
    pub fn apply_attachment_plan(&mut self, plan: &AttachmentEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_attachment_plan",
            "Replan against current session",
        )?;
        self.apply_authoring_change("apply_attachment_plan", "", false, |document| {
            document.apply_attachment_edit(&plan.domain)
        })
    }
    pub fn edit_attachment(
        &mut self,
        intent: zipzi_core::document::AttachmentEdit,
    ) -> Result<(), SdkError> {
        self.ensure_active("edit_attachment")?;
        let plan = self
            .candidate_document()
            .plan_attachment_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_attachment", vec![])))?;
        self.apply_authoring_change("edit_attachment", "", false, |document| {
            document.apply_attachment_edit(&plan)
        })
    }
}

#[derive(Debug, Clone)]
pub struct MotionRegistrationAddress {
    handle: ObjectHandle,
    owner: Option<ObjectHandle>,
    epoch: u64,
    field_epoch: u64,
    expected: zipzi_core::document::MotionRegistration,
}
impl AuthoringSession {
    pub fn motion_registration_address(
        &self,
        id: &str,
    ) -> Result<MotionRegistrationAddress, SdkError> {
        let (container, entry) = self.document().get_motion_registration(id).ok_or_else(|| {
            SdkError::new(
                "MISSING_MOTION_REGISTRATION",
                id,
                "motion_registration_address",
            )
        })?;
        let owner = match container {
            MotionContainer::Root => None,
            MotionContainer::Group(id) => Some(self.handle(ObjectKind::MotionGroup, &id)?),
        };
        Ok(MotionRegistrationAddress {
            handle: self.handle(ObjectKind::MotionRegistration, id)?,
            owner,
            epoch: self.motion_contexts.get(id).map_or(0, |c| c.1),
            field_epoch: self.animation_field_epoch(id),
            expected: entry.clone(),
        })
    }
    pub fn plan_motion_registration_patch(
        &self,
        address: &MotionRegistrationAddress,
        entry: zipzi_core::document::MotionRegistration,
    ) -> Result<MotionEditPlan, SdkError> {
        self.resolve_handle(&address.handle)?;
        if self.animation_field_epoch(address.handle.id()) != address.field_epoch {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Registration fields changed; capture a new address",
                "plan_motion_registration_patch",
            ));
        }
        if let Some(owner) = &address.owner {
            self.resolve_handle(owner)?;
        }
        let owner = self
            .document()
            .get_motion_registration(address.handle.id())
            .map(|(container, _)| match container {
                MotionContainer::Root => None,
                MotionContainer::Group(id) => Some(id),
            });
        if owner != Some(address.owner.as_ref().map(|h| h.id().to_owned())) {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Registration owner changed",
                "plan_motion_registration_patch",
            ));
        }
        if self
            .motion_contexts
            .get(address.handle.id())
            .map_or(0, |c| c.1)
            != address.epoch
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Registration owner changed",
                "plan_motion_registration_patch",
            ));
        }
        self.plan_motion_edit(MotionEdit::PatchRegistration {
            id: address.handle.id().into(),
            expected: address.expected.clone(),
            entry,
        })
    }
}
/// Content requests follow a living attachment and a non-rewinding content epoch.
#[derive(Debug, Clone)]
pub struct AttachmentLoadToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: ObjectHandle,
    epoch: u64,
}
impl AttachmentLoadToken {
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }
}
impl AuthoringSession {
    pub fn attachment_load_token(&self, id: &str) -> Result<AttachmentLoadToken, SdkError> {
        let handle = self.handle(ObjectKind::Attachment, id)?;
        self.document()
            .motion_unlocked(id)
            .map_err(|s| SdkError::from_status(s, "attachment_load_token", vec![id.into()]))?;
        Ok(AttachmentLoadToken {
            cancelled: self.begin_resource_load(id),
            handle,
            epoch: *self.attachment_content_epochs.get(id).unwrap_or(&0),
        })
    }
    pub fn replace_attachment_content(
        &mut self,
        token: &AttachmentLoadToken,
        bytes: Option<std::sync::Arc<Vec<u8>>>,
    ) -> Result<EditReceipt, SdkError> {
        self.begin_attachment_content_edit(token, bytes)?
            .commit_to(self)
    }

    /// Stage a token-checked attachment load without publishing it.
    pub fn begin_attachment_content_edit(
        &self,
        token: &AttachmentLoadToken,
        bytes: Option<std::sync::Arc<Vec<u8>>>,
    ) -> Result<crate::EditSession<'static>, SdkError> {
        self.resolve_handle(&token.handle)?;
        if token.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(SdkError::new(
                "CANCELLED",
                token.handle.id(),
                "replace_attachment_content",
            ));
        }
        if self
            .attachment_content_epochs
            .get(token.handle.id())
            .copied()
            .unwrap_or(0)
            != token.epoch
        {
            return Err(SdkError::new(
                "STALE_CONTENT",
                "Attachment content changed",
                "replace_attachment_content",
            ));
        }
        let expected = self
            .document()
            .get_attachment(token.handle.id())
            .unwrap()
            .clone();
        let mut attachment = expected.clone();
        attachment.bytes = bytes;
        let plan = self.plan_attachment_edit(zipzi_core::document::AttachmentEdit::Replace {
            expected,
            attachment,
        })?;
        let flag = token.cancelled.clone();
        let mut edit = self.begin_owned_edit("Load attachment content", Some(plan.version))?;
        edit.resource_load_controls.push(flag);
        edit.apply_attachment_plan(&plan)?;
        Ok(edit)
    }
}

/// Key time order and Bezier roles use dedicated curve operations, never row sorting.
#[derive(Debug, Clone)]
pub enum MotionCurveEdit {
    MoveKeys(Vec<crate::MotionKeyMove>),
    InsertKey {
        track: String,
        id: String,
        time: f32,
        value: Option<f32>,
    },
    DeleteKeys(Vec<crate::MotionKeyRef>),
    Interpolation {
        keys: Vec<crate::MotionKeyRef>,
        kind: crate::MotionInterpolation,
    },
    BezierHandle {
        end: crate::MotionKeyRef,
        handle: crate::MotionBezierHandle,
        point: zipzi_core::document::MotionPoint,
    },
    ScaleTime {
        duration: f32,
    },
}
#[derive(Debug, Clone)]
pub struct MotionCurvePlan {
    version: Version,
    candidate: crate::MotionClipCandidate,
}
impl AuthoringSession {
    pub fn plan_motion_curve_edit(
        &self,
        id: &str,
        intent: MotionCurveEdit,
    ) -> Result<MotionCurvePlan, SdkError> {
        let document = self.document();
        let original = document
            .get_motion(id)
            .cloned()
            .ok_or_else(|| SdkError::new("MISSING_MOTION", id, "plan_motion_curve_edit"))?;
        let mut candidate = crate::MotionClipCandidate::new(document, original.clone())?;
        match intent {
            MotionCurveEdit::MoveKeys(moves) => candidate.move_keys(document, &moves)?,
            MotionCurveEdit::InsertKey {
                track,
                id,
                time,
                value,
            } => candidate.insert_key(document, &track, &id, time, value)?,
            MotionCurveEdit::DeleteKeys(keys) => candidate.delete_keys(document, &keys)?,
            MotionCurveEdit::Interpolation { keys, kind } => {
                candidate.set_interpolation(document, &keys, kind)?
            }
            MotionCurveEdit::BezierHandle { end, handle, point } => {
                candidate.set_handle(document, &end, handle, point)?
            }
            MotionCurveEdit::ScaleTime { duration } => candidate.scale_time(document, duration)?,
        }
        document
            .motion_unlocked(id)
            .map_err(|s| SdkError::from_status(s, "plan_motion_curve_edit", vec![id.into()]))?;
        for old in &original.tracks {
            if !candidate.clip().tracks.contains(old) {
                for id in std::iter::once(&old.id).chain(old.keys().map(|k| &k.id)) {
                    document.motion_unlocked(id).map_err(|s| {
                        SdkError::from_status(s, "plan_motion_curve_edit", vec![id.clone()])
                    })?;
                }
            }
        }
        Ok(MotionCurvePlan {
            version: self.version(),
            candidate,
        })
    }
    pub fn execute_motion_curve_plan(
        &mut self,
        plan: &MotionCurvePlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit motion curve", Some(plan.version), |e| {
            e.replace_motion_candidate(plan.candidate.clone())
        })?;
        Ok(receipt)
    }
}

#[derive(Debug, Clone)]
pub struct MotionExportPlan {
    version: Version,
    omitted: Vec<zipzi_core::document::DisabledMotionTrack>,
    text: String,
}
impl MotionExportPlan {
    pub fn omitted(&self) -> &[zipzi_core::document::DisabledMotionTrack] {
        &self.omitted
    }
}
impl AuthoringSession {
    pub fn plan_available_motion_export(&self, id: &str) -> Result<MotionExportPlan, SdkError> {
        let projection = self.document().motion_projection(id).map_err(|s| {
            SdkError::from_status(s, "plan_available_motion_export", vec![id.into()])
        })?;
        let text = zipzi_project::export_available_motion3(self.document(), id)
            .map_err(|e| SdkError::new(&e.code, &e.message, "plan_available_motion_export"))?;
        Ok(MotionExportPlan {
            version: self.version(),
            omitted: projection.disabled,
            text,
        })
    }
    pub fn export_motion_plan(&self, plan: &MotionExportPlan) -> Result<String, SdkError> {
        if self.version() != plan.version {
            return Err(SdkError::new(
                "STALE_PLAN",
                "Review the current omission set",
                "export_motion_plan",
            ));
        }
        Ok(plan.text.clone())
    }
}

#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct AttachmentCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to load the referenced resource contents; inspect its state/reason
    /// before invoking the operation.
    pub load_content: CapabilityState,
}
impl AuthoringSession {
    pub fn attachment_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<AttachmentCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let state = if handle.kind() != ObjectKind::Attachment {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if self.document().editor_state().get(handle.id()).locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let delete = if state == CapabilityState::Available
            && !self.document().references_to(handle.id()).is_empty()
        {
            CapabilityState::Unavailable {
                code: "OBJECT_REFERENCED",
            }
        } else {
            state.clone()
        };
        Ok(AttachmentCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            duplicate: state.clone(),
            delete,
            load_content: state,
        })
    }
}

impl crate::AuthoringSnapshot {
    pub fn plan_attachment_edit(
        &self,
        intent: zipzi_core::document::AttachmentEdit,
    ) -> Result<AttachmentEditPlan, SdkError> {
        Ok(AttachmentEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_attachment_edit(intent)
                .map_err(|s| SdkError::from_status(s, "plan_attachment_edit", vec![]))?,
        })
    }
}

impl crate::AuthoringSnapshot {
    /// Plan a typed motion edit on an immutable source, without changing preview/history.
    pub fn plan_motion_edit(
        &self,
        intent: zipzi_core::document::MotionEdit,
    ) -> Result<MotionEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_motion_edit(intent)
            .map_err(|s| SdkError::from_status(s, "plan_motion_edit", vec![]))?;
        Ok(MotionEditPlan {
            version: self.version(),
            domain,
        })
    }
}
