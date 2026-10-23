//! Pointer observation updates native menu context without moving keyboard focus.
use super::*;
use gpui_kit::component::WindowExt;

impl EditorView {
    pub(crate) fn pointer_observer(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let inside = self.input_router.inside.clone();
        let menu_target = self.input_router.menu_target.clone();
        let owner = cx.weak_entity();
        let regions = self.resolved_regions(window, cx);
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let entered = inside.clone();
                let regions = regions.clone();
                let menu_target = menu_target.clone();
                let owner = owner.clone();
                let update_menu_target = Rc::new(move |window: &mut Window, cx: &mut App| {
                    entered.set(true);
                    if let Some((target, _)) =
                        regions.iter().find(|(_, region)| region.hovered(window))
                    {
                        if menu_target.replace(Some(target.local)) != Some(target.local) {
                            let owner = owner.clone();
                            window.defer(cx, move |window, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    let modal = view.modal || window.has_active_dialog(cx);
                                    view.refresh_menus(modal, window, cx);
                                });
                            });
                        }
                    }
                });
                let moving = update_menu_target.clone();
                window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        moving(window, cx);
                    }
                });
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        update_menu_target(window, cx);
                    }
                });
                let exited = inside.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, _| {
                    if phase == DispatchPhase::Capture {
                        exited.set(false);
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}
