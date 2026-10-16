//! Canvas object picking menu and document-scoped actions.
use super::*;
use gpui_kit::component::menu::PopupMenu;

impl CanvasView {
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
        let state = owner.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.workspace() != WorkspaceKind::Modeling
        {
            return menu;
        }
        let point = canvas.read(cx).local(window.mouse_position());
        let candidates = state
            .editor
            .pick_candidates(point, state.frame.as_ref().map(|f| f.canvas.version));
        let Some(binding) = canvas.read(cx).binding(&state.editor) else {
            return menu;
        };
        let visibility = state.editor.deformer_visibility();
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
                            || owner.editor.workspace() != WorkspaceKind::Modeling
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
                editor_core::CommandSource::ContextMenu,
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
        menu
    }
}
