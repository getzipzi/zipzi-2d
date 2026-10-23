//! Host adaptation shared by toolbar, menu and shortcut object operations.
//! Typed core planners own validation, defaults and publication; this layer owns
//! input boundaries and inspection. Inspection never activates resource playback.
use crate::*;
use editor_core::{DocumentId, EditingContextToken, ObjectKind, ObjectRef, OperationResult};
use gpui_kit::component::WindowExt;
pub(crate) mod scene_creation;

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
                reason: Some(if *code == "OBJECT_LOCKED" {
                    t!("authoring.locked").into()
                } else {
                    (*code).into()
                }),
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
        match self.editor.editing_context() {
            Some(editor_core::EditingContext::Modeling) => {
                self.editor
                    .execute_for(document, EditorCommand::Navigate(object.clone()))?;
            }
            Some(editor_core::EditingContext::Animation) => {
                self.editor
                    .inspect_animation_resource(document, object.clone())?;
            }
            None => anyhow::bail!("Editing context changed"),
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
        let object = result
            .suggested_selection
            .iter()
            .find(|object| object.kind == kind)
            .cloned();
        if let Some(object) = &object {
            self.inspect_authoring_object(document, object.clone(), true, window, cx)?;
        }
        Ok(object)
    }
}
