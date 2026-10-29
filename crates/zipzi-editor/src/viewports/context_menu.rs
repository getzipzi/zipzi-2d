//! Canvas object picking menu and document-scoped actions.
use super::*;
use gpui_kit::component::menu::PopupMenu;

impl ViewportSurface {
    pub(super) fn object_menu(
        menu_owner: &WeakEntity<EditorView>,
        menu_canvas: &WeakEntity<Self>,
        mut menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        menu = crate::ui::compact::menu(menu);
        let Some(owner) = menu_owner.upgrade() else {
            return menu;
        };
        let Some(canvas) = menu_canvas.upgrade() else {
            return menu;
        };
        if window.has_active_dialog(cx) {
            return menu;
        }
        let Some(document) = owner.read(cx).editor.active_id() else {
            return menu;
        };
        if !owner.read(cx).editor.manual_active()
            && !owner.update(cx, |view, cx| {
                view.prepare_property_inputs(document, window, cx)
            })
        {
            return menu;
        }
        let state = owner.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.editing_context() != Some(zipzi_editor_core::EditingContext::Modeling)
        {
            return menu;
        }
        let point = canvas.read(cx).local(window.mouse_position());
        let candidates = state.editor.pick_candidates(
            point,
            state
                .modeling_frame_for(canvas.read(cx).viewport)
                .map(|f| f.canvas.version),
        );
        let Some(binding) = canvas.read(cx).binding(&state.editor) else {
            return menu;
        };
        let visibility = state.editor.deformer_visibility();
        if state.editor.manual_active() {
            let edge = state
                .modeling_frame_for(canvas.read(cx).viewport)
                .and_then(|frame| state.editor.manual_edge_at(&frame.canvas, point));
            let target = state.editor.manual_target();
            let deletion = state.editor.command_for_viewport(
                CommandId::DeleteObject,
                binding,
                zipzi_editor_core::CommandSource::ContextMenu,
            );
            let delete_owner = menu_owner.clone();
            menu = menu.item(
                PopupMenuItem::new(t!(CommandId::ManualRemoveSelection.spec().label).into_owned())
                    .action(Box::new(crate::actions::RunCommand::targeted(deletion)))
                    .disabled(!state.editor.command_available(deletion))
                    .on_click(move |_, window, cx| {
                        let _ = delete_owner
                            .update(cx, |view, cx| view.run_invocation(deletion, window, cx));
                    }),
            );
            let command = CommandId::ManualReconnect;
            let invocation = state.editor.command_for_viewport(
                command,
                binding,
                zipzi_editor_core::CommandSource::ContextMenu,
            );
            let enabled = state.editor.command_state(invocation).enabled;
            let owner = owner.downgrade();
            let canvas = canvas.downgrade();
            menu = menu.item(
                PopupMenuItem::new(t!(command.spec().label).into_owned())
                    .disabled(!enabled)
                    .on_click(move |_, window, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                owner.run_invocation(invocation, window, cx);
                            });
                        }
                        if let Some(canvas) = canvas.upgrade() {
                            let focus = canvas.read(cx).focus.clone();
                            window.defer(cx, move |window, cx| window.focus(&focus, cx));
                        }
                    }),
            );
            let edge_owner = menu_owner.clone();
            let edge_canvas = menu_canvas.clone();
            menu = menu
                .item(
                    PopupMenuItem::new(if edge.is_some_and(|edge| !edge.constrained) {
                        t!("editor.manual_constrain_edge")
                    } else {
                        t!("editor.manual_release_constraint")
                    })
                    .disabled(edge.is_none() || target.is_none())
                    .on_click(move |_, window, cx| {
                        let (Some(owner), Some(target), Some(edge)) =
                            (edge_owner.upgrade(), target.clone(), edge)
                        else {
                            return;
                        };
                        owner.update(cx, |owner, cx| {
                            if !owner.editor.viewport_binding_valid(binding)
                                || owner.editor.editing_context()
                                    != Some(zipzi_editor_core::EditingContext::Modeling)
                                || owner.modal
                                || owner.editor.is_busy()
                            {
                                return;
                            }
                            let result = owner.editor.execute(EditorCommand::SetMeshConstraint {
                                target,
                                endpoints: edge.endpoints,
                                constrained: !edge.constrained,
                            });
                            owner.complete_edit(result, cx);
                        });
                        if let Some(canvas) = edge_canvas.upgrade() {
                            let focus = canvas.read(cx).focus.clone();
                            window.defer(cx, move |window, cx| window.focus(&focus, cx));
                        }
                    }),
                )
                .separator();
        }
        if candidates.is_empty() {
            menu = menu.item(PopupMenuItem::new(t!("editor.no_canvas_objects")).disabled(true));
        }
        for candidate in candidates {
            let owner = owner.downgrade();
            let canvas = canvas.downgrade();
            menu = menu.item(
                PopupMenuItem::new(candidate.label).on_click(move |_, window, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return;
                    };
                    owner.update(cx, |owner, cx| {
                        if !owner.editor.viewport_binding_valid(binding)
                            || owner.editor.editing_context()
                                != Some(zipzi_editor_core::EditingContext::Modeling)
                            || owner.modal
                            || owner.editor.is_busy()
                        {
                            return;
                        }
                        let result =
                            owner
                                .editor
                                .execute(EditorCommand::SelectCanvasObjects(vec![
                                    candidate.object.clone(),
                                ]));
                        owner.complete_edit(result, cx);
                    });
                    if let Some(canvas) = canvas.upgrade() {
                        let focus = canvas.read(cx).focus.clone();
                        // Let menu dismissal finish before returning keyboard input.
                        window.defer(cx, move |window, cx| window.focus(&focus, cx));
                    }
                }),
            );
        }
        menu = menu.separator();
        for (command, checked) in [
            (CommandId::ToggleAncestorDeformers, visibility.ancestors),
            (CommandId::ToggleDeformers, visibility.all),
        ] {
            let invocation = state.editor.command_for_viewport(
                command,
                binding,
                zipzi_editor_core::CommandSource::ContextMenu,
            );
            let owner = owner.downgrade();
            let canvas = canvas.downgrade();
            menu = menu.item(
                PopupMenuItem::new(t!(command.spec().label).into_owned())
                    .checked(checked)
                    .on_click(move |_, window, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                owner.run_invocation(invocation, window, cx);
                            });
                        }
                        if let Some(canvas) = canvas.upgrade() {
                            let focus = canvas.read(cx).focus.clone();
                            window.defer(cx, move |window, cx| window.focus(&focus, cx));
                        }
                    }),
            );
        }
        if state.editor.original_points()
            || state.editor.document().is_some_and(|d| {
                d.context()
                    .selections
                    .iter()
                    .any(|r| r.kind == zipzi_editor_core::ObjectKind::Mesh)
            })
        {
            menu = menu.separator();
            let checked = state.editor.original_points();
            let owner = owner.downgrade();
            let canvas = canvas.downgrade();
            menu = menu.item(
                PopupMenuItem::new(t!("editor.edit_original_points"))
                    .checked(checked)
                    .on_click(move |_, window, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                if !owner.editor.viewport_binding_valid(binding) {
                                    return;
                                }
                                let result =
                                    owner.editor.execute(EditorCommand::ToggleOriginalPoints);
                                owner.complete_edit(result, cx);
                            });
                        }
                        if let Some(canvas) = canvas.upgrade() {
                            let focus = canvas.read(cx).focus.clone();
                            window.defer(cx, move |window, cx| window.focus(&focus, cx));
                        }
                    }),
            );
        }
        if !state.editor.manual_active() {
            menu = crate::authoring::object_deletion::append_menu(
                menu,
                menu_owner.clone(),
                state.editor.command_for_viewport(
                    CommandId::DeleteObject,
                    binding,
                    zipzi_editor_core::CommandSource::ContextMenu,
                ),
                &state.editor.document().unwrap().context().selections,
                cx,
            );
        }
        menu
    }
}
