//! SDK expression authoring: one domain planner, one publication/history path.
use crate::{
    AuthoringSession, EditReceipt, EditSession, ObjectHandle, ObjectKind, SdkError, Version,
};
use zipzi_core::document::{ExpressionEdit, ExpressionEntryFields, ExpressionPlan};

#[derive(Debug, Clone)]
pub struct ExpressionEditPlan {
    version: Version,
    domain: ExpressionPlan,
}
impl ExpressionEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn affected_expressions(&self) -> &[String] {
        self.domain.affected_expressions()
    }
    pub fn affected_entries(&self) -> &[String] {
        self.domain.affected_entries()
    }
}

pub use zipzi_contract::authoring::{CapabilityState, ExpressionCapabilities};

/// A field address captures both the living entry and the owner context. A move
/// retains the object handle, but invalidates this owner-dependent address.
#[derive(Debug, Clone)]
pub struct ExpressionEntryAddress {
    pub(crate) entry: ObjectHandle,
    pub(crate) owner: ObjectHandle,
    owner_epoch: u64,
    field_epoch: u64,
    pub(crate) expected: ExpressionEntryFields,
}
impl ExpressionEntryAddress {
    pub fn entry_id(&self) -> &str {
        self.entry.id()
    }
    pub fn owner_id(&self) -> &str {
        self.owner.id()
    }
    pub fn expected(&self) -> &ExpressionEntryFields {
        &self.expected
    }
}

impl AuthoringSession {
    pub fn expression_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<ExpressionCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let supported = matches!(
            handle.kind(),
            ObjectKind::Expression | ObjectKind::ExpressionEntry
        );
        let owner = self
            .document()
            .get_expression_entry(handle.id())
            .map(|(owner, _)| owner.id.as_str());
        let locked = self.document().editor_state().get(handle.id()).locked
            || owner.is_some_and(|id| self.document().editor_state().get(id).locked);
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let owned_locked = self
            .document()
            .get_expression(handle.id())
            .is_some_and(|owner| {
                owner
                    .entries
                    .iter()
                    .any(|e| self.document().editor_state().get(&e.id).locked)
            });
        let subtree_state = if owned_locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        let entry = handle.kind() == ObjectKind::ExpressionEntry;
        Ok(ExpressionCapabilities {
            inspect: CapabilityState::Available,
            create_entry: if !entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "ENTRY_CANNOT_OWN_ENTRIES",
                }
            },
            patch: state.clone(),
            move_entry: if entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "EXPRESSION_HAS_NO_OWNER",
                }
            },
            duplicate: subtree_state.clone(),
            delete: subtree_state,
            repair: if entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "NO_REFERENCE_SLOT",
                }
            },
            preview: if entry {
                CapabilityState::Unsupported {
                    code: "PREVIEW_OWNER_EXPRESSION",
                }
            } else if supported {
                CapabilityState::Available
            } else {
                state
            },
        })
    }

    pub fn plan_expression_edit(
        &self,
        edit: ExpressionEdit,
    ) -> Result<ExpressionEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_expression_edit(edit)
            .map_err(|s| SdkError::from_status(s, "plan_expression_edit", vec![]))?;
        Ok(ExpressionEditPlan {
            version: self.version(),
            domain,
        })
    }
    pub fn execute_expression_plan(
        &mut self,
        plan: &ExpressionEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit expression", Some(plan.version), |edit| {
            edit.apply_expression_plan(plan)
        })?;
        Ok(receipt)
    }
    pub fn expression_entry_address(&self, id: &str) -> Result<ExpressionEntryAddress, SdkError> {
        let (owner, entry) = self.document().get_expression_entry(id).ok_or_else(|| {
            SdkError::new("MISSING_EXPRESSION_ENTRY", id, "expression_entry_address")
        })?;
        Ok(ExpressionEntryAddress {
            entry: self.handle(ObjectKind::ExpressionEntry, id)?,
            owner: self.handle(ObjectKind::Expression, &owner.id)?,
            owner_epoch: self
                .expression_entry_contexts
                .get(id)
                .map_or(0, |context| context.1),
            expected: entry.into(),
            field_epoch: self.animation_field_epoch(id),
        })
    }
    pub fn plan_expression_entry_patch(
        &self,
        address: &ExpressionEntryAddress,
        fields: ExpressionEntryFields,
    ) -> Result<ExpressionEditPlan, SdkError> {
        self.resolve_handle(&address.entry)?;
        self.resolve_handle(&address.owner)?;
        if self
            .expression_entry_contexts
            .get(address.entry.id())
            .map_or(0, |context| context.1)
            != address.owner_epoch
            || self.animation_field_epoch(address.entry.id()) != address.field_epoch
            || self
                .document()
                .get_expression_entry(address.entry.id())
                .is_none_or(|(owner, _)| owner.id != address.owner.id())
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Entry context changed; capture a new address",
                "plan_expression_entry_patch",
            ));
        }
        self.plan_expression_edit(ExpressionEdit::PatchEntry {
            id: address.entry.id().into(),
            expected: address.expected.clone(),
            fields,
        })
    }
}
impl EditSession<'_> {
    /// Invalid previews preserve the last candidate and keep the gesture active.
    pub fn preview_expression_entry(
        &mut self,
        id: &str,
        fields: ExpressionEntryFields,
    ) -> Result<(), SdkError> {
        self.ensure_active("preview_expression_entry")?;
        let (_, entry) = self
            .candidate_document()
            .get_expression_entry(id)
            .ok_or_else(|| {
                SdkError::new("MISSING_EXPRESSION_ENTRY", id, "preview_expression_entry")
            })?;
        let intent = ExpressionEdit::PatchEntry {
            id: entry.id.clone(),
            expected: entry.into(),
            fields,
        };
        // Plan before applying: a rejected value leaves both the last candidate
        // and the gesture available for correction.
        let plan = self
            .candidate_document()
            .plan_expression_edit(intent)
            .map_err(|s| SdkError::from_status(s, "preview_expression_entry", vec![id.into()]))?;
        let result = self.document().apply_expression_edit(&plan);
        self.record(result, "preview_expression_entry", id)
    }

    pub fn apply_expression_plan(&mut self, plan: &ExpressionEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_expression_plan",
            "Replan against the current session",
        )?;
        let result = self.document().apply_expression_edit(&plan.domain);
        self.record(result, "apply_expression_plan", "")?;
        self.track_expression_deletions(plan.domain.affected_entries());
        for id in plan.domain.affected_expressions() {
            let key = crate::types::ObjectKey {
                kind: ObjectKind::Expression,
                id: id.clone(),
            };
            if self.base_keys.contains(&key)
                && self.candidate_document().get_expression(id).is_none()
            {
                self.erased_keys.insert(key);
            }
        }
        Ok(())
    }
    /// Compose domain operations inside a larger SDK transaction.
    pub fn edit_expression(&mut self, intent: ExpressionEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_expression")?;
        let plan = self
            .candidate_document()
            .plan_expression_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_expression", vec![])))?;
        let result = self.document().apply_expression_edit(&plan);
        self.record(result, "edit_expression", "")?;
        self.track_expression_deletions(plan.affected_entries());
        for id in plan.affected_expressions() {
            let key = crate::types::ObjectKey {
                kind: ObjectKind::Expression,
                id: id.clone(),
            };
            if self.base_keys.contains(&key)
                && self.candidate_document().get_expression(id).is_none()
            {
                self.erased_keys.insert(key);
            }
        }
        Ok(())
    }
}

/// Owned candidate for one gesture. Invalid previews are recoverable, dropping
/// the candidate cancels, and only commit publishes an event/history entry.
pub struct ExpressionEntryCandidate {
    address: ExpressionEntryAddress,
    edit: EditSession<'static>,
}
impl AuthoringSession {
    pub fn begin_expression_entry_edit(
        &self,
        address: &ExpressionEntryAddress,
    ) -> Result<ExpressionEntryCandidate, SdkError> {
        self.plan_expression_entry_patch(address, address.expected.clone())?;
        Ok(ExpressionEntryCandidate {
            address: address.clone(),
            edit: self.begin_owned_edit("Edit expression entry", Some(self.version()))?,
        })
    }
}
impl ExpressionEntryCandidate {
    pub fn preview(&mut self, fields: ExpressionEntryFields) -> Result<(), SdkError> {
        self.edit
            .preview_expression_entry(self.address.entry.id(), fields)
    }
    pub fn projection(&self) -> Result<zipzi_core::document::ExpressionProjection, SdkError> {
        self.edit
            .candidate_document()
            .expression_projection(self.address.owner.id())
            .map_err(|s| {
                SdkError::from_status(
                    s,
                    "preview_expression_entry",
                    vec![self.address.owner.id().into()],
                )
            })
    }
    pub fn commit(self, session: &mut AuthoringSession) -> Result<EditReceipt, SdkError> {
        session.resolve_handle(&self.address.entry)?;
        session.resolve_handle(&self.address.owner)?;
        self.edit.commit_to(session)
    }
    pub fn cancel(self) {}
}

/// Export plans capture the exact omission set and document version. Execution
/// cannot silently accept a larger loss after a concurrent edit.
#[derive(Debug, Clone)]
pub struct ExpressionExportPlan {
    version: Version,
    omitted: Vec<zipzi_core::document::DisabledExpressionEntry>,
    text: String,
}
impl ExpressionExportPlan {
    pub fn omitted(&self) -> &[zipzi_core::document::DisabledExpressionEntry] {
        &self.omitted
    }
}
impl AuthoringSession {
    pub fn plan_available_expression_export(
        &self,
        id: &str,
    ) -> Result<ExpressionExportPlan, SdkError> {
        let projection = self.document().expression_projection(id).map_err(|s| {
            SdkError::from_status(s, "plan_available_expression_export", vec![id.into()])
        })?;
        let text =
            zipzi_project::export_available_expression3(self.document(), id).map_err(|e| {
                let mut error =
                    SdkError::new(&e.code, &e.message, "plan_available_expression_export");
                error.field_path = Some(e.path.into());
                error
            })?;
        Ok(ExpressionExportPlan {
            version: self.version(),
            omitted: projection.disabled,
            text,
        })
    }
    pub fn export_expression_plan(&self, plan: &ExpressionExportPlan) -> Result<String, SdkError> {
        if self.version() != plan.version {
            return Err(SdkError::new(
                "STALE_PLAN",
                "Replan to review the current omission set",
                "export_expression_plan",
            ));
        }
        Ok(plan.text.clone())
    }
    pub fn expression_collection(
        &self,
        owner: &ObjectHandle,
    ) -> Result<ExpressionCollection, SdkError> {
        self.resolve_handle(owner)?;
        if owner.kind() != ObjectKind::Expression {
            return Err(SdkError::new(
                "WRONG_OBJECT_KIND",
                "Collection owner must be an expression",
                "expression_collection",
            ));
        }
        let entries = self
            .document()
            .get_expression(owner.id())
            .unwrap()
            .entries
            .iter()
            .map(|e| self.handle(ObjectKind::ExpressionEntry, &e.id))
            .collect::<Result<_, _>>()?;
        Ok(ExpressionCollection {
            owner: owner.clone(),
            entries,
            semantic_order: true,
            allows_empty: true,
        })
    }
}
#[derive(Debug, Clone)]
pub struct ExpressionCollection {
    pub owner: ObjectHandle,
    pub entries: Vec<ObjectHandle>,
    pub semantic_order: bool,
    pub allows_empty: bool,
}

impl crate::AuthoringSnapshot {
    /// Plan a typed expression edit on an immutable source, without changing preview/history.
    pub fn plan_expression_edit(
        &self,
        intent: zipzi_core::document::ExpressionEdit,
    ) -> Result<ExpressionEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_expression_edit(intent)
            .map_err(|s| SdkError::from_status(s, "plan_expression_edit", vec![]))?;
        Ok(ExpressionEditPlan {
            version: self.version(),
            domain,
        })
    }
}
