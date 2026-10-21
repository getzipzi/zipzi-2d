//! The inspector lifecycle is shared by object and transient selection editors.
//! The adapter preserves each domain's existing preview/undo implementation.
use super::*;
use editor_core::{PropertyEditToken, PropertyPatch, TimelineCandidateIdentity};

#[derive(Clone, Debug)]
pub(crate) enum PreviewToken {
    Object(PropertyEditToken),
    Timeline {
        target: PropertyTarget,
        context: editor_core::EditingContextToken,
        token: TimelineCandidateIdentity,
    },
}
impl PreviewToken {
    pub(crate) fn retains_target(&self, editor: &Editor) -> bool {
        matches!(self, Self::Timeline { .. }) && self.owned(editor)
    }
    pub(crate) fn owned(&self, editor: &Editor) -> bool {
        match self {
            Self::Object(token) => editor.property_edit_owned(token),
            Self::Timeline {
                target,
                context,
                token,
            } => {
                editor.editing_context_token_valid(*context)
                    && editor.active_id() == Some(target.document)
                    && editor
                        .timeline_edit()
                        .is_some_and(|edit| edit.identity.edit == token.edit)
            }
        }
    }
    pub(crate) fn finish(&self, editor: &mut Editor) -> anyhow::Result<()> {
        anyhow::ensure!(self.owned(editor), "Property edit ended");
        match self {
            Self::Object(token) => editor.finish_property_edit(token),
            Self::Timeline { target, token, .. } => {
                editor.finish_timeline_edit(target.document, *token)
            }
        }
    }
    pub(crate) fn cancel(&self, editor: &mut Editor) {
        match self {
            Self::Object(token) => editor.cancel_property_edit(token),
            Self::Timeline { target, token, .. } => {
                editor.cancel_timeline_edit_for(target.document, *token);
            }
        }
    }
}
impl PropertyTarget {
    pub(crate) fn sample_time(&self, editor: &Editor) -> Option<u32> {
        matches!(
            self.timeline,
            Some(timeline::TimelineTarget::Channel { .. })
        )
        .then(|| editor.animation().map_or(0., |a| a.edit_time).to_bits())
    }
    pub(crate) fn has_object_details(&self) -> bool {
        self.timeline.is_none()
    }
    pub(crate) fn title(&self, editor: &Editor) -> Option<(String, SharedString, bool)> {
        let document = editor.document()?;
        let locked = document
            .ensure_editable(std::slice::from_ref(&self.object))
            .is_err();
        if let Some(selection) = &self.timeline {
            return Some((
                selection.title(editor).to_string(),
                t!("timeline.title").into(),
                locked,
            ));
        }
        Some((
            document.object_index().name(&self.object).to_owned(),
            (provider(self.object.kind).type_name)(document, &self.object),
            locked,
        ))
    }

    pub(crate) fn definition(&self, editor: &Editor) -> FormDefinition {
        if self.object.kind == ObjectKind::PhysicsGroup {
            return physics::definition(editor, &self.object);
        }
        self.timeline.as_ref().map_or_else(
            || definition(self.object.kind),
            |target| target.definition(editor),
        )
    }
    pub(crate) fn values(&self, editor: &Editor) -> anyhow::Result<PropertyValues> {
        anyhow::ensure!(
            editor.active_id() == Some(self.document),
            "Document changed"
        );
        match &self.timeline {
            Some(target) => target.values(editor),
            None => editor
                .document()
                .ok_or_else(|| anyhow::anyhow!("Document closed"))?
                .property_values(&self.object),
        }
    }
    pub(crate) fn commit(&self, editor: &mut Editor, patch: PropertyPatch) -> anyhow::Result<()> {
        if let Some(target) = &self.timeline {
            anyhow::ensure!(
                super::target(editor).as_ref() == Some(self),
                "Selection changed"
            );
            let command = target.command(editor, &patch.changes)?;
            editor.execute_timeline(self.document, command)
        } else {
            let context = editor
                .editing_context_token()
                .ok_or_else(|| anyhow::anyhow!("Editing context changed"))?;
            editor.commit_properties(context, self.object.clone(), patch)
        }
    }
    pub(crate) fn editable(&self, editor: &Editor) -> bool {
        (provider(self.object.kind).editable_in)(self.context)
            && editor.document().is_some_and(|doc| {
                doc.ensure_editable(std::slice::from_ref(&self.object))
                    .is_ok()
            })
            && (self.timeline.is_none()
                || editor.animation().is_some_and(|a| !a.playing)
                    && editor.timeline_projection().is_some_and(|p| p.editable))
    }
}
pub(crate) fn begin_edit(
    editor: &mut Editor,
    target: &PropertyTarget,
    patch: &PropertyPatch,
) -> anyhow::Result<PreviewToken> {
    let context = editor
        .editing_context_token()
        .ok_or_else(|| anyhow::anyhow!("Editing context changed"))?;
    if target.timeline.is_some() {
        anyhow::ensure!(
            super::target(editor).as_ref() == Some(target),
            "Selection changed"
        );
        let token = editor.begin_timeline_edit(target.document, "Edit motion property")?;
        Ok(PreviewToken::Timeline {
            target: target.clone(),
            context,
            token,
        })
    } else {
        editor
            .begin_property_edit(context, target.object.clone(), patch)
            .map(PreviewToken::Object)
    }
}
pub(crate) fn update_edit(
    editor: &mut Editor,
    token: &PreviewToken,
    patch: &PropertyPatch,
) -> anyhow::Result<()> {
    anyhow::ensure!(token.owned(editor), "Property edit ended");
    match token {
        PreviewToken::Object(token) => editor.update_property_edit(token, patch),
        PreviewToken::Timeline { target, token, .. } => {
            let command = target
                .timeline
                .as_ref()
                .unwrap()
                .command(editor, &patch.changes)?;
            editor
                .update_timeline_edit(target.document, *token, command)
                .map(|_| ())
        }
    }
}
