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
        motion: Option<String>,
        expression: Option<String>,
        channels: Vec<TimelineRowId>,
        keys: Vec<TimelineKeyRef>,
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
                motion, expression, ..
            } => motion
                .as_ref()
                .map(|id| ObjectRef::new(ObjectKind::Motion, id))
                .or_else(|| {
                    expression
                        .as_ref()
                        .map(|id| ObjectRef::new(ObjectKind::Expression, id))
                }),
        }
    }
}
impl Editor {
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
    pub fn active_inspection(&self) -> Option<Inspection> {
        self.inspection(self.editing_context()?)
    }
}
