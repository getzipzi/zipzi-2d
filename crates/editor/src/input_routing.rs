//! Window input ownership. Regions are hit-tested against the current rendered
//! frame, including clipping and occlusion; hover never changes keyboard focus.
use crate::*;
use editor_core::{
    Command, CommandId, CommandScope, CommandSource, CommandTarget, TimelineBinding,
    ViewportBinding,
};
use gpui_kit::component::WindowExt;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
pub(crate) struct InputRegion(Rc<RefCell<Option<Hitbox>>>);
impl InputRegion {
    pub fn marker(&self) -> impl IntoElement {
        let region = self.clone();
        canvas(
            move |bounds, window, _| {
                *region.0.borrow_mut() = Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));
            },
            |_, _, _, _| (),
        )
        .absolute()
        .inset_0()
        .size_full()
    }
    pub(crate) fn hovered(&self, window: &Window) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(|hitbox| hitbox.is_hovered_at(window.mouse_position(), window))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalTarget {
    Panel(editor_core::PanelKind),
    Viewport(ViewportBinding),
    Timeline(TimelineBinding),
}
#[derive(Default)]
pub(crate) struct InputRouter {
    inside: Rc<Cell<bool>>,
    pub menu_target: Rc<Cell<Option<LocalTarget>>>,
}
impl InputRouter {
    pub fn deactivate(&self) {
        self.inside.set(false);
        self.menu_target.set(None);
    }
}
impl EditorView {
    fn input_dock(&self) -> Option<&crate::docking::DockWorkspace> {
        #[cfg(any(test, feature = "test-support"))]
        if self.viewport_scene.is_some() {
            return None;
        }
        self.dock.as_ref()
    }
    pub(crate) fn hovered_target(&self, window: &Window, cx: &App) -> Option<LocalTarget> {
        if !self.input_router.inside.get() || !window.is_visible() {
            return None;
        }
        for id in self.editor.viewport_ids() {
            if !self.viewport_displayed(id, window, cx) {
                continue;
            }
            let Some(surface) = self.viewport_surface(id, cx) else {
                continue;
            };
            let surface = surface.read(cx);
            if surface.input_region.hovered(window) {
                return surface.binding(&self.editor).map(LocalTarget::Viewport);
            }
        }
        let dock = self.input_dock()?;
        if dock.displayed(editor_core::PanelKind::AnimationTimeline, cx)
            && dock.timeline.read(cx).input_region.hovered(window)
        {
            self.editor
                .timeline_binding(dock.timeline.read(cx).id)
                .map(LocalTarget::Timeline)
        } else {
            dock.input_regions
                .iter()
                .find(|(kind, region)| dock.displayed(**kind, cx) && region.hovered(window))
                .map(|(&kind, _)| LocalTarget::Panel(kind))
        }
    }
    fn captured_target(&self, cx: &App) -> Option<LocalTarget> {
        if let Some(binding) = self.editor.viewport_interaction_binding() {
            return Some(LocalTarget::Viewport(binding));
        }
        let dock = self.dock.as_ref()?;
        let timeline = dock.timeline.read(cx);
        timeline
            .capturing(&self.editor)
            .then(|| self.editor.timeline_binding(timeline.id))
            .flatten()
            .map(LocalTarget::Timeline)
    }
    fn command_at(&self, id: CommandId, target: Option<LocalTarget>) -> Command {
        match (id.spec().scope, target) {
            (CommandScope::Viewport, Some(LocalTarget::Viewport(binding))) => self
                .editor
                .command_for_viewport(id, binding, CommandSource::Shortcut),
            (CommandScope::Timeline, Some(LocalTarget::Timeline(binding))) => self
                .editor
                .command_for_timeline(id, binding, CommandSource::Shortcut),
            (CommandScope::Viewport | CommandScope::Timeline, _) => Command {
                id,
                source: CommandSource::Shortcut,
                target: CommandTarget::Unavailable,
            },
            _ => self.editor.command(id).with_source(CommandSource::Shortcut),
        }
    }
    /// Resolve once, then use that command for matching, repeats and execution.
    pub(crate) fn route_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal || window.has_active_dialog(cx) {
            return;
        }
        if event.prefer_character_input {
            return;
        }
        let contexts = window.context_stack();
        if contexts.iter().any(|c| {
            c.contains("PopupMenu")
                || c.contains("AppMenuBar")
                || c.contains("Select")
                || c.contains("SearchPanel")
        }) {
            return;
        }
        let text =
            self.has_focused_input(window, cx) || contexts.iter().any(|c| c.contains("Input"));
        let capture = self.captured_target(cx);
        if event.keystroke.key == "escape" && capture.is_some() {
            match capture.unwrap() {
                LocalTarget::Panel(_) => unreachable!("plain panels never capture editing"),
                LocalTarget::Viewport(binding) => self.run_invocation(
                    self.command_at(
                        CommandId::CancelGesture,
                        Some(LocalTarget::Viewport(binding)),
                    ),
                    window,
                    cx,
                ),
                LocalTarget::Timeline(_) => {
                    let timeline = self.dock.as_ref().unwrap().timeline.clone();
                    let (document, token) =
                        timeline.update(cx, |timeline, cx| timeline.cancel_local(window, cx));
                    if let (Some(document), Some(token)) = (document, token) {
                        self.editor.cancel_timeline_edit_for(document, token);
                    }
                    self.redraw(cx);
                }
            }
            cx.stop_propagation();
            return;
        }
        let target = capture.or_else(|| self.hovered_target(window, cx));
        let resolved = cx
            .global::<settings::Settings>()
            .bindings
            .iter()
            .find(|binding| {
                let scope = binding.command.spec().scope;
                let applicable = match scope {
                    CommandScope::Window => true,
                    CommandScope::Document => !text,
                    CommandScope::Viewport => {
                        !text && matches!(target, Some(LocalTarget::Viewport(_)))
                    }
                    CommandScope::Timeline => {
                        !text && matches!(target, Some(LocalTarget::Timeline(_)))
                    }
                    CommandScope::Property => false, // GPUI controls own semantic Enter/Escape.
                };
                applicable
                    && event
                        .keystroke
                        .should_match(&KeybindingKeystroke::new_with_mapper(
                            Keystroke::parse(&binding.key).expect("validated binding"),
                            false,
                            cx.keyboard_mapper().as_ref(),
                        ))
            })
            .map(|binding| self.command_at(binding.command, target));
        if let Some(command) = resolved {
            cx.stop_propagation(); // Disabled local commands still consume their key.
            if !event.is_held || command.id.spec().repeat {
                self.run_invocation(command, window, cx);
            }
        }
    }
    pub(crate) fn pointer_observer(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let inside = self.input_router.inside.clone();
        let menu_target = self.input_router.menu_target.clone();
        let owner = cx.weak_entity();
        let mut regions = Vec::new();
        for id in self.editor.viewport_ids() {
            if let Some(binding) = self.editor.viewport_binding(id) {
                if !self.viewport_displayed(id, window, cx) {
                    continue;
                }
                if let Some(surface) = self.viewport_surface(id, cx) {
                    regions.push((
                        LocalTarget::Viewport(binding),
                        surface.read(cx).input_region.clone(),
                    ));
                }
            }
        }
        if let Some(dock) = self.input_dock() {
            for (&kind, region) in &dock.input_regions {
                if dock.displayed(kind, cx) {
                    regions.push((LocalTarget::Panel(kind), region.clone()));
                }
            }
            let timeline = dock.timeline.read(cx);
            if let Some(binding) = self.editor.timeline_binding(timeline.id) {
                regions.push((
                    LocalTarget::Timeline(binding),
                    timeline.input_region.clone(),
                ));
            }
        }
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let entered = inside.clone();
                let regions = regions.clone();
                let menu_target = menu_target.clone();
                let owner = owner.clone();
                let update_pointer = Rc::new(move |window: &mut Window, cx: &mut App| {
                    entered.set(true);
                    if let Some((target, _)) =
                        regions.iter().find(|(_, region)| region.hovered(window))
                    {
                        if menu_target.replace(Some(*target)) != Some(*target) {
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
                let moving = update_pointer.clone();
                window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        moving(window, cx);
                    }
                });
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        update_pointer(window, cx);
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
