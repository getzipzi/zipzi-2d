use crate::{EditorView, t};
use editor_core::{CommandId, WorkspaceKind};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{DockPlacement, PanelId},
    menu::DropdownMenu,
};
use gpui_kit::{prelude::*, *};

/// Isolate control hover redraws from document panel observers.
pub struct TopBar {
    owner: WeakEntity<EditorView>,
    _observe: Subscription,
}

impl TopBar {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
        })
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let modal = owner.read(cx).modal || window.has_active_dialog(cx);
        render_top_bar(&owner, modal, window, cx).into_any_element()
    }
}

fn render_top_bar(
    owner: &Entity<EditorView>,
    modal: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    div()
        .id("workspace-top-bar")
        .test_support()
        .h(px(38.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .bg(cx.theme().background)
        .border_b_1()
        .border_color(cx.theme().border)
        // Equal flexible sides keep the tools centered on the window rather
        // than on the space left over after the mode switcher.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(render_workspace_switcher(owner, modal, window, cx)),
        )
        .child(super::toolbar::render_toolbar(owner, modal, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_end()
                .child(div().mr_2().child(render_more_menu(owner, modal))),
        )
}

fn render_more_menu(owner: &Entity<EditorView>, modal: bool) -> impl IntoElement {
    Button::new("workspace-layout-menu")
        .icon(IconName::Ellipsis)
        .small()
        .ghost()
        .tooltip(t!("dock.layout_actions"))
        .disabled(modal)
        .dropdown_menu({
            let owner = owner.downgrade();
            move |mut menu, window, cx| {
                let node = owner.upgrade().and_then(|view| {
                    let dock = view.read(cx).dock.as_ref()?;
                    dock.area
                        .read(cx)
                        .layout(DockPlacement::Center)
                        .and_then(|tree| {
                            tree.find_panel_node(PanelId::from(dock.canvas.entity_id()))
                        })
                });
                if let Some(node) = node {
                    menu = crate::docking::add_panel::menu(
                        menu,
                        owner.clone(),
                        node,
                        false,
                        window,
                        cx,
                    )
                    .separator();
                }
                menu.menu(
                    t!("menu.reset_layout"),
                    Box::new(crate::actions::RunCommand {
                        id: CommandId::ResetLayout,
                    }),
                )
            }
        })
        .anchor(Anchor::TopRight)
}

fn render_workspace_switcher(
    owner: &Entity<EditorView>,
    modal: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    use gpui_kit::base::{Tab, Tabs, spring};

    // A 196px control with a 1px frame, 2px inset and 2px segment gap.
    const SEGMENT_WIDTH: f32 = 93.;
    const SEGMENT_STEP: f32 = SEGMENT_WIDTH + 2.;
    let view = owner.read(cx);
    let workspace = view.editor.workspace();
    let modes = [
        (
            WorkspaceKind::Modeling,
            CommandId::ModelingWorkspace,
            "workspace-modeling",
        ),
        (
            WorkspaceKind::Animation,
            CommandId::AnimationWorkspace,
            "workspace-animation",
        ),
    ]
    .map(|(kind, command, id)| {
        let enabled = !modal && view.editor.command_available(view.editor.command(command));
        (kind, command, id, enabled)
    });
    let disabled = modes.iter().all(|(_, _, _, enabled)| !enabled);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let surface = cx.theme().tab_bar;
    let border = cx.theme().border;
    let dark = cx.theme().is_dark();
    let indicator_left = spring(
        "workspace-mode-indicator",
        px(2. + workspace.index() as f32 * SEGMENT_STEP),
        cx.theme().motion_tokens().spring_move,
        window,
        cx,
    );

    div()
        .id("workspace-switcher")
        .test_support()
        .flex()
        .flex_shrink_0()
        .items_center()
        .px_2()
        .child(
            Tabs::new("workspace-mode-tabs")
                .relative()
                .flex()
                .items_center()
                .w(px(196.))
                .h(px(28.))
                .p(px(2.))
                .gap(px(2.))
                .rounded(px(6.))
                .border_1()
                .border_color(border)
                .bg(surface)
                .when(disabled, |tabs| tabs.opacity(0.5))
                .child(
                    div()
                        .absolute()
                        .left(indicator_left)
                        .top(px(2.))
                        .w(px(SEGMENT_WIDTH))
                        .h(px(22.))
                        .rounded(px(4.))
                        .bg(primary.opacity(if dark { 0.18 } else { 0.09 }))
                        .border_1()
                        .border_color(primary.opacity(if dark { 0.4 } else { 0.25 })),
                )
                .children(modes.into_iter().map(|(kind, command, id, enabled)| {
                    let selected = workspace == kind;
                    let owner = owner.clone();
                    Tab::new(id)
                        .relative()
                        .flex_1()
                        .h_full()
                        .rounded(px(4.))
                        .text_xs()
                        .text_color(if selected { primary } else { muted })
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .selected(selected)
                        .disabled(!enabled)
                        .accessibility_label(t!(command.spec().label))
                        .set_position(kind.index() + 1, WorkspaceKind::ALL.len())
                        .when(!selected && enabled, |tab| {
                            tab.cursor_pointer().hover(|style| {
                                style.bg(foreground.opacity(0.06)).text_color(foreground)
                            })
                        })
                        .child(t!(command.spec().label))
                        .on_click(move |_, window, cx| {
                            owner.update(cx, |view, cx| view.run_command(command, window, cx));
                        })
                })),
        )
}
