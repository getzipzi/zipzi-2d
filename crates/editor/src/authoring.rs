//! Host adaptation shared by toolbar, menu and shortcut object operations.
//! Typed core planners own validation, defaults and publication; this layer owns
//! input boundaries and inspection. Inspection never activates resource playback.
use crate::*;
use editor_core::{DocumentId, EditingContextToken, ObjectKind, ObjectRef, OperationResult};
use gpui_kit::component::WindowExt;
pub(crate) mod animation;
mod availability;
pub(crate) mod business;
mod presentation;
pub(crate) use availability::{authoring_error, error_code};
pub(crate) mod scene_creation;
pub(crate) use presentation::AuthoringPresentation;

/// A deferred menu must not target a restored incarnation of a deleted object.
/// Until a domain supplies a narrower address, reject changed document versions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoringContext {
    editing: EditingContextToken,
    revision: u64,
}
impl AuthoringContext {
    pub fn capture(editor: &Editor) -> Option<Self> {
        Some(Self {
            editing: editor.editing_context_token()?,
            revision: editor.document()?.model_generation(),
        })
    }
}

/// A presentation-only resource destination dialog keeps the ongoing run alive.
/// The view retires its guard on close/drop; document/workspace changes expire it.
#[derive(Clone)]
pub(crate) struct PlaybackDialogGuard {
    context: AuthoringContext,
    active: std::rc::Rc<std::cell::Cell<bool>>,
}
impl PlaybackDialogGuard {
    pub(crate) fn new(context: AuthoringContext) -> Self {
        Self {
            context,
            active: std::rc::Rc::new(std::cell::Cell::new(true)),
        }
    }
    pub(crate) fn valid(&self, editor: &Editor) -> bool {
        self.active.get() && editor.editing_context_token() == Some(self.context.editing)
    }
    pub(crate) fn close(&self) {
        self.active.set(false);
    }
}

/// Unsupported actions are absent. Unavailable actions retain the core reason.
pub(crate) struct ActionAvailability {
    pub enabled: bool,
    pub reason: Option<SharedString>,
}
impl ActionAvailability {
    pub fn from_core(state: &kasane_sdk::CapabilityState) -> Option<Self> {
        use kasane_sdk::CapabilityState;
        match state {
            CapabilityState::Unsupported { .. } => None,
            CapabilityState::Available => Some(Self {
                enabled: true,
                reason: None,
            }),
            CapabilityState::Unavailable { code } => Some(Self {
                enabled: false,
                reason: Some(
                    availability::reason_message(code)
                        .map(Into::into)
                        .unwrap_or_else(|| (*code).into()),
                ),
            }),
        }
    }
}

impl EditorView {
    pub(crate) fn execute_authoring(
        &mut self,
        context: AuthoringContext,
        window: &mut Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&mut Editor, DocumentId) -> anyhow::Result<OperationResult>,
    ) -> anyhow::Result<OperationResult> {
        anyhow::ensure!(
            !self.modal
                && !window.has_active_dialog(cx)
                && AuthoringContext::capture(&self.editor) == Some(context),
            "Object operation context changed"
        );
        anyhow::ensure!(
            self.prepare_property_inputs(context.editing.document, window, cx),
            "Finish the current input before editing objects"
        );
        operation(&mut self.editor, context.editing.document)
    }

    pub(crate) fn inspect_authoring_object(
        &mut self,
        document: DocumentId,
        object: ObjectRef,
        focus_field: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.editor.active_id() == Some(document),
            "Document changed"
        );
        self.editor
            .inspect_authoring_object(document, object.clone())?;
        if object.kind.is_binding() {
            self.reveal_keyforms(window, cx);
        }
        if let Some(dock) = &self.dock {
            dock.show(editor_core::PanelKind::Properties, None, window, cx);
            if focus_field {
                let panel = dock.properties.downgrade();
                let context = AuthoringContext::capture(&self.editor);
                // The owner borrow has ended before controls synchronize their form.
                window.defer(cx, move |window, cx| {
                    let _ = panel.update(cx, |panel, cx| {
                        panel.focus_authoring_object(context, document, &object, window, cx);
                    });
                });
            }
        }
        Ok(())
    }

    pub(crate) fn present_created_object(
        &mut self,
        document: DocumentId,
        result: &OperationResult,
        kind: ObjectKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Option<ObjectRef>> {
        self.present_authoring_result(
            document,
            result,
            AuthoringPresentation::Created {
                kind,
                projection: None,
            },
            window,
            cx,
        )
    }
}

pub(crate) fn creation_commands() -> impl Iterator<Item = editor_core::CommandId> {
    scene_creation::all_commands().chain([
        editor_core::CommandId::CreatePhysicsGroup,
        editor_core::CommandId::CreatePoseGroup,
        editor_core::CommandId::CreatePoseEntry,
        editor_core::CommandId::CreatePoseLink,
        editor_core::CommandId::CreateMotion,
        editor_core::CommandId::CreateMotionGroup,
        editor_core::CommandId::CreateExpression,
        editor_core::CommandId::CreateExpressionEntry,
        editor_core::CommandId::CreateParameter,
        editor_core::CommandId::CreateParameterGroup,
    ])
}
pub(crate) fn availability_reason(code: &str) -> String {
    availability::reason_message(code).unwrap_or_else(|| code.to_owned())
}
