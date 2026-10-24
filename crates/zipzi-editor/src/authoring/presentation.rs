//! Explicit post-operation intent. All entry points share the same guarded
//! deferred location; inspection and resource activation remain separate.
use super::*;
use zipzi_editor_core::{PaletteProjection, PanelKind};

#[derive(Clone)]
pub(crate) enum AuthoringPresentation {
    Created {
        kind: ObjectKind,
        projection: Option<PaletteProjection>,
    },
    Moved {
        object: ObjectRef,
        projection: PaletteProjection,
    },
    AfterDelete {
        candidates: Vec<ObjectRef>,
    },
}
fn panel(object: &ObjectRef, animation: bool, projection: Option<PaletteProjection>) -> PanelKind {
    use ObjectKind as K;
    let kind = match object.kind {
        K::Parameter | K::ParameterGroup => PanelKind::Parameters,
        K::Part | K::Mesh | K::Deformer | K::Offscreen => {
            if projection == Some(PaletteProjection::Deformation) {
                PanelKind::Deformers
            } else {
                PanelKind::PartsTree
            }
        }
        K::MeshBinding | K::SceneBinding | K::GlueBinding | K::BindingAxis | K::BindingSample => {
            PanelKind::Keyforms
        }
        K::BlendBinding | K::BlendConstraint | K::BlendKeyTable => PanelKind::BlendShapes,
        K::Glue | K::GlueVertexPair => PanelKind::Glue,
        K::DrawOrderGroup => PanelKind::DrawOrder,
        K::Texture | K::Attachment => PanelKind::Resources,
        K::Motion
        | K::MotionGroup
        | K::MotionRegistration
        | K::MotionTrack
        | K::MotionKey
        | K::MotionEvent => {
            if animation {
                PanelKind::AnimationMotions
            } else {
                PanelKind::Relations
            }
        }
        K::Expression | K::ExpressionEntry => {
            if animation {
                PanelKind::AnimationExpressions
            } else {
                PanelKind::Relations
            }
        }
        K::Physics | K::PhysicsGroup | K::PhysicsInput | K::PhysicsOutput | K::PhysicsNode => {
            if animation {
                PanelKind::AnimationPhysics
            } else {
                PanelKind::Properties
            }
        }
        K::Pose | K::PoseGroup | K::PoseEntry | K::PoseLink => {
            if animation {
                PanelKind::AnimationPose
            } else {
                PanelKind::Pose
            }
        }
        K::Document => PanelKind::Properties,
        K::ModelParameterGroup
        | K::ModelParameterMember
        | K::ModelHitArea
        | K::CdiCombinedSet
        | K::CdiCombinedMember
        | K::Unresolved => PanelKind::Relations,
    };
    if animation
        && !kind
            .spec()
            .available(zipzi_editor_core::WorkspaceKind::Animation)
    {
        PanelKind::Properties
    } else {
        kind
    }
}
impl EditorView {
    pub(crate) fn present_authoring_result(
        &mut self,
        document: DocumentId,
        result: &OperationResult,
        intent: AuthoringPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Option<ObjectRef>> {
        anyhow::ensure!(
            self.editor.active_id() == Some(document),
            "Document changed"
        );
        if !result.receipt.changed {
            return Ok(None);
        }
        let (object, projection, created, inspect, focus) = match intent {
            AuthoringPresentation::Created { kind, projection } => {
                let Some(object) = result
                    .suggested_selection
                    .iter()
                    .find(|r| r.kind == kind)
                    .cloned()
                else {
                    return Ok(None);
                };
                (object, projection, true, true, true)
            }
            AuthoringPresentation::Moved { object, projection } => {
                (object, Some(projection), false, false, false)
            }
            AuthoringPresentation::AfterDelete { candidates } => {
                let index = self.editor.document().unwrap().object_index();
                let Some(object) = candidates.into_iter().find(|r| index.get(r).is_some()) else {
                    return Ok(None);
                };
                (object, None, false, true, false)
            }
        };
        if inspect {
            if let Some(dock) = &self.dock {
                dock.show(
                    panel(
                        &object,
                        self.editor.editing_context()
                            == Some(zipzi_editor_core::EditingContext::Animation),
                        projection,
                    ),
                    None,
                    window,
                    cx,
                );
            }
            self.inspect_authoring_object(document, object.clone(), focus, window, cx)?;
        }
        self.defer_authoring_location(
            document,
            object.clone(),
            projection,
            created,
            inspect,
            window,
            cx,
        );
        Ok(Some(object))
    }
    pub(crate) fn defer_authoring_location(
        &self,
        document: DocumentId,
        object: ObjectRef,
        projection: Option<PaletteProjection>,
        created: bool,
        require_inspection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.weak_entity();
        let expected = AuthoringContext::capture(&self.editor);
        window.defer(cx, move |window, cx| {
            let Some(owner) = owner.upgrade() else { return };
            let state = owner.read(cx);
            if AuthoringContext::capture(&state.editor) != expected
                || state.editor.active_id() != Some(document)
                || (require_inspection
                    && state
                        .editor
                        .active_inspection()
                        .and_then(|i| i.object())
                        .as_ref()
                        != Some(&object))
            {
                return;
            }
            locate(&owner, document, object, projection, created, window, cx);
        });
    }
}
pub(crate) fn locate(
    owner: &Entity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
    projection: Option<PaletteProjection>,
    created: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let state = owner.read(cx);
    if state.editor.active_id() != Some(document)
        || state
            .editor
            .document()
            .is_none_or(|doc| doc.object_index().get(&object).is_none())
    {
        return;
    }
    let Some(dock) = &state.dock else { return };
    let kind = panel(
        &object,
        state.editor.editing_context() == Some(zipzi_editor_core::EditingContext::Animation),
        projection,
    );
    let parts = dock.part_tree.clone();
    let deformer = dock.deformer_tree.clone();
    let parameters = dock.parameters.clone();
    let motions = dock.animation_motions.clone();
    let expressions = dock.animation_expressions.clone();
    let document_panel = dock.document_panels.get(&kind).cloned();
    let animation_panel = dock.animation_inspectors.get(&kind).cloned();
    match kind {
        PanelKind::PartsTree => parts.update(cx, |p, cx| {
            if created {
                p.locate_created_object(document, object, window, cx)
            } else {
                p.locate_moved_object(document, object, cx)
            }
        }),
        PanelKind::Deformers => deformer.update(cx, |p, cx| {
            if created {
                p.locate_created_object(document, object, window, cx)
            } else {
                p.locate_moved_object(document, object, cx)
            }
        }),
        PanelKind::Parameters => parameters.update(cx, |p, cx| {
            p.locate_moved_object(document, &object, window, cx)
        }),
        PanelKind::AnimationMotions => {
            motions.update(cx, |p, cx| p.locate_object(document, &object, cx))
        }
        PanelKind::AnimationExpressions => {
            expressions.update(cx, |p, cx| p.locate_object(document, &object, cx))
        }
        _ => {
            if let Some(p) = document_panel {
                p.update(cx, |p, cx| p.locate_object(document, object, cx));
            } else if let Some(p) = animation_panel {
                p.update(cx, |p, cx| p.locate_object(document, &object, cx));
            }
        }
    }
}
