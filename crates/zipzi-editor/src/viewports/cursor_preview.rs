use super::*;
use crate::ui::compact;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Disableable, button::Button};
use zipzi_editor_core::{
    AnimationPreviewMode, AnimationSession, DocumentId, Editor, ObjectKind, WorkspaceKind,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct PreviewControls {
    document: DocumentId,
    active: bool,
    manual: bool,
    bypass: bool,
    bypass_enabled: bool,
    disabled: bool,
}

pub(super) fn state(editor: &Editor, disabled: bool) -> Option<PreviewControls> {
    let animation = editor.animation();
    let active = editor.cursor_physics_active();
    Some(PreviewControls {
        document: editor.active_id()?,
        active,
        manual: !active && animation.is_some_and(|a| a.physics_preview.manual),
        bypass: animation.is_some_and(|a| a.physics_preview.bypass),
        bypass_enabled: animation.is_some_and(can_bypass),
        disabled,
    })
}

fn can_bypass(animation: &AnimationSession) -> bool {
    animation.preview_mode == AnimationPreviewMode::Result || animation.physics_preview.manual
}

pub(super) fn controls(owner: WeakEntity<EditorView>, state: PreviewControls) -> impl IntoElement {
    let PreviewControls {
        document,
        active,
        manual,
        bypass,
        bypass_enabled,
        disabled,
    } = state;
    let toggle_owner = owner.clone();
    let manual_owner = owner.clone();
    let bypass_owner = owner.clone();
    compact::header_control(compact::group(
        "animation-preview-controls",
        t!("physics.cursor_preview"),
        Axis::Horizontal,
        vec![
            compact::header_button(compact::toggle(
                Button::new("physics-cursor-preview"),
                active,
            ))
            .icon(IconName::MousePointer)
            .tooltip(t!("physics.cursor_preview"))
            .accessibility_label(t!("physics.cursor_preview"))
            .disabled(disabled)
            .on_click(move |_, window, cx| {
                let _ = toggle_owner.update(cx, |view, cx| {
                    if !preview_controls_available(view, document, window, cx) {
                        return;
                    }
                    let enabled = !view.editor.cursor_physics_active();
                    let result = view.editor.set_physics_cursor(document, enabled);
                    view.animation_anchor = None;
                    view.complete_edit(result, cx);
                });
            }),
            compact::header_button(compact::toggle(Button::new("physics-manual"), manual))
                .icon(IconName::SlidersHorizontal)
                .tooltip(t!("physics.manual"))
                .accessibility_label(t!("physics.manual"))
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    let _ = manual_owner.update(cx, |view, cx| {
                        if !preview_controls_available(view, document, window, cx) {
                            return;
                        }
                        let manual = view.editor.animation().unwrap().physics_preview.manual
                            && !view.editor.cursor_physics_active();
                        let result = view.editor.set_physics_manual(document, !manual);
                        view.animation_anchor = None;
                        view.complete_edit(result, cx);
                    });
                }),
            compact::header_button(compact::toggle(Button::new("physics-bypass"), bypass))
                .icon(IconName::Ban)
                .tooltip(t!("physics.bypass"))
                .accessibility_label(t!("physics.bypass"))
                .disabled(disabled || !bypass_enabled)
                .on_click(move |_, window, cx| {
                    let _ = bypass_owner.update(cx, |view, cx| {
                        if !preview_controls_available(view, document, window, cx) {
                            return;
                        }
                        let animation = view.editor.animation().unwrap();
                        if can_bypass(animation) {
                            let bypass = !animation.physics_preview.bypass;
                            let result = view.editor.set_physics_bypass(document, bypass);
                            view.animation_anchor = None;
                            view.complete_edit(result, cx);
                        }
                    });
                }),
            compact::header_button(Button::new("physics-parameter-mapping"))
                .icon(IconName::Settings2)
                .tooltip(t!("physics.parameter_mapping"))
                .accessibility_label(t!("physics.parameter_mapping"))
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        if !preview_controls_available(view, document, window, cx) {
                            return;
                        }
                        let Some(object) = view.editor.document().and_then(|doc| {
                            doc.object_index().objects.iter().find_map(|info| {
                                (info.reference.kind == ObjectKind::Document)
                                    .then(|| info.reference.clone())
                            })
                        }) else {
                            return;
                        };
                        let result =
                            view.inspect_authoring_object(document, object, false, window, cx);
                        if result.is_ok()
                            && let Some(dock) = &view.dock
                        {
                            dock.properties
                                .update(cx, |panel, cx| panel.reveal_parameter_mapping(cx));
                        }
                        view.complete_edit(result, cx);
                    });
                }),
        ],
    ))
}

fn preview_controls_available(
    view: &mut EditorView,
    document: DocumentId,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> bool {
    !view.modal
        && !window.has_active_dialog(cx)
        && !view.editor.is_busy()
        && view.editor.active_id() == Some(document)
        && view.editor.workspace() == WorkspaceKind::Animation
        && view.prepare_property_inputs(document, window, cx)
}
