//! Property sources are resolved from an explicit editing context. Modeling
//! object navigation and Animation resource/channel selection do not overwrite
//! one another; all references retain their document and stable object IDs.
use crate::{
    DocumentId, EditingContext, EditingContextToken, Editor, ObjectKind, ObjectRef, TimelineKeyRef,
    TimelineRowId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectionSource {
    Modeling {
        object: Option<ObjectRef>,
        primary: Option<ObjectRef>,
    },
    Animation {
        object: Option<ObjectRef>,
        motion: Option<String>,
        expression: Option<String>,
        channels: Vec<TimelineRowId>,
        keys: Vec<TimelineKeyRef>,
        events: Vec<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspection {
    pub document: DocumentId,
    pub context: EditingContext,
    pub token: EditingContextToken,
    pub source: InspectionSource,
}
impl Inspection {
    pub fn object(&self) -> Option<ObjectRef> {
        match &self.source {
            InspectionSource::Modeling { object, .. } => object.clone(),
            InspectionSource::Animation {
                object,
                motion,
                expression,
                ..
            } => object.clone().or_else(|| {
                motion
                    .as_ref()
                    .map(|id| ObjectRef::new(ObjectKind::Motion, id))
                    .or_else(|| {
                        expression
                            .as_ref()
                            .map(|id| ObjectRef::new(ObjectKind::Expression, id))
                    })
            }),
        }
    }
}
/// Inspector identity survives saves and transient editing barriers, but not
/// document/context switches (including a switch away and back between frames).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionContextToken {
    document: DocumentId,
    generation: u64,
}
impl Editor {
    pub fn inspection_context_token(&self) -> Option<InspectionContextToken> {
        Some(InspectionContextToken {
            document: self.active_id()?,
            generation: self.inspection_generation,
        })
    }

    pub fn inspection(&self, context: EditingContext) -> Option<Inspection> {
        let document = self.document()?;
        let token = self.editing_context_token()?;
        let source = match context {
            EditingContext::Modeling => InspectionSource::Modeling {
                object: document.context().inspected.clone(),
                primary: document.context().selection.clone(),
            },
            EditingContext::Animation => {
                let animation = self.animation()?;
                let timeline = self.timeline_view();
                InspectionSource::Animation {
                    object: animation.inspected.clone(),
                    motion: animation
                        .motion
                        .as_ref()
                        .map(|motion| motion.clip_id.clone()),
                    expression: animation.expression.clone(),
                    channels: timeline
                        .map(|view| view.channels.iter().cloned().collect())
                        .unwrap_or_default(),
                    keys: timeline
                        .map(|view| view.keys.iter().cloned().collect())
                        .unwrap_or_default(),
                    events: timeline
                        .map(|view| view.events.iter().cloned().collect())
                        .unwrap_or_default(),
                }
            }
        };
        Some(Inspection {
            document: token.document,
            context,
            token,
            source,
        })
    }
    pub fn inspect_timeline_selection(&mut self, document: DocumentId) -> anyhow::Result<()> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .animation
            .playback
            .inspected = None;
        Ok(())
    }
    pub fn inspect_animation_resource(
        &mut self,
        document: DocumentId,
        object: ObjectRef,
    ) -> anyhow::Result<()> {
        self.ensure_animation_target(document)?;
        anyhow::ensure!(
            matches!(
                object.kind,
                ObjectKind::Motion
                    | ObjectKind::Expression
                    | ObjectKind::Physics
                    | ObjectKind::PhysicsGroup
            ) && self
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .is_some(),
            "Resource no longer exists"
        );
        self.cancel_timeline_edit();
        self.active_session_mut()
            .unwrap()
            .animation
            .playback
            .inspected = Some(object);
        Ok(())
    }
    pub fn active_inspection(&self) -> Option<Inspection> {
        self.inspection(self.editing_context()?)
    }
}
