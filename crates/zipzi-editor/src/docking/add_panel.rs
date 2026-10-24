//! Targeted panel creation. Capture workspace and node identity when the menu opens.
use crate::{EditorView, t};
use gpui_kit::base::Placement;
use gpui_kit::component::{
    WindowExt,
    dock::{InsertTarget, NodeId},
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit::*;
use zipzi_editor_core::{PANELS, WorkspaceKind};

pub(crate) fn menu(
    mut menu: PopupMenu,
    owner: WeakEntity<EditorView>,
    node: NodeId,
    allow_tabs: bool,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let Some(view) = owner.upgrade() else {
        return menu;
    };
    let workspace = view.read(cx).editor.workspace();
    if allow_tabs {
        let owner = owner.clone();
        menu = menu.submenu(t!("dock.add_panel"), window, cx, move |menu, _, cx| {
            items(
                menu,
                owner.clone(),
                workspace,
                InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                cx,
            )
        });
    }
    for (label, placement) in [
        (t!("dock.add_right"), Placement::Right),
        (t!("dock.add_below"), Placement::Bottom),
    ] {
        let owner = owner.clone();
        menu = menu.submenu(label, window, cx, move |menu, _, cx| {
            items(
                menu,
                owner.clone(),
                workspace,
                InsertTarget::Split {
                    node,
                    placement,
                    size: None,
                },
                cx,
            )
        });
    }
    menu
}

fn items(
    mut menu: PopupMenu,
    owner: WeakEntity<EditorView>,
    workspace: WorkspaceKind,
    target: InsertTarget,
    cx: &App,
) -> PopupMenu {
    let Some(view) = owner.upgrade() else {
        return menu;
    };
    let mut entries: Vec<_> = PANELS
        .iter()
        .filter(|spec| spec.available(workspace) && spec.command.is_some())
        .map(|spec| {
            (
                spec,
                view.read(cx)
                    .dock
                    .as_ref()
                    .is_some_and(|dock| dock.contains(spec.kind, cx)),
            )
        })
        .collect();
    // Keep available choices first; the checked entries explain why another
    // instance cannot be created and remain in catalog order below them.
    entries.sort_by_key(|(_, present)| *present);
    for (spec, present) in entries {
        let kind = spec.kind;
        let owner = owner.clone();
        let label = t!(spec.command.unwrap().spec().label).into_owned();
        menu = menu.item(
            PopupMenuItem::new(if present {
                format!("{label} ({})", t!("dock.already_open"))
            } else {
                label
            })
            .icon(super::skin::panel_icon(spec.name))
            .checked(present)
            .disabled(present)
            .on_click(move |_, window, cx| {
                _ = owner.update(cx, |view, cx| {
                    if view.editor.workspace() != workspace
                        || view.modal
                        || window.has_active_dialog(cx)
                    {
                        return;
                    }
                    if let Some(dock) = &view.dock {
                        if !dock.contains(kind, cx) {
                            dock.show(kind, Some(target), window, cx);
                        }
                    }
                    cx.notify();
                });
            }),
        );
    }
    menu
}
