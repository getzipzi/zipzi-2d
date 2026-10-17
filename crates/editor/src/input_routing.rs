//! Window input ownership. Regions are hit-tested against the current rendered
//! frame, including clipping and occlusion; hover never changes keyboard focus.
use crate::*;
use editor_core::{
    Command, CommandId, CommandScope, CommandSource, CommandTarget, TimelineBinding, ToolId,
    ViewportBinding, ViewportKind,
};
use gpui_kit::component::WindowExt;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Pointer editing can reserve Escape; viewport tools use the configured command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureCancelKey {
    Escape,
    CommandBinding,
}

type CaptureResolver = dyn Fn(&Editor, &App) -> Option<CapturedInput>;
type CancelCapture = dyn FnOnce(&mut Window, &mut App) -> Option<CoreCleanup>;
pub(crate) type CoreCleanup = Box<dyn FnOnce(&mut EditorView)>;

/// A snapshot, never another stored owner. Local cleanup compares the snapshot
/// with the owner's current operation before returning domain cleanup to the root.
pub(crate) struct CapturedInput {
    pub target: LocalTarget,
    valid: bool,
    cancel_key: CaptureCancelKey,
    cancel: Box<CancelCapture>,
}
impl CapturedInput {
    pub fn new(
        target: LocalTarget,
        valid: bool,
        cancel_key: CaptureCancelKey,
        cancel: impl FnOnce(&mut Window, &mut App) -> Option<CoreCleanup> + 'static,
    ) -> Self {
        Self {
            target,
            valid,
            cancel_key,
            cancel: Box::new(cancel),
        }
    }
    fn cancel(self, owner: &mut EditorView, window: &mut Window, cx: &mut App) {
        if let Some(cleanup) = (self.cancel)(window, cx) {
            cleanup(owner);
        }
    }
}

#[derive(Clone)]
pub(crate) struct InputRegion {
    hitbox: Rc<RefCell<Option<Hitbox>>>,
    focus: FocusHandle,
    capture: Option<Rc<CaptureResolver>>,
}
impl InputRegion {
    pub fn new(focus: FocusHandle) -> Self {
        Self {
            focus,
            hitbox: Default::default(),
            capture: None,
        }
    }
    pub fn with_capture(
        mut self,
        capture: impl Fn(&Editor, &App) -> Option<CapturedInput> + 'static,
    ) -> Self {
        self.capture = Some(Rc::new(capture));
        self
    }
    pub fn marker(&self) -> impl IntoElement {
        let region = self.clone();
        canvas(
            move |bounds, window, _| {
                *region.hitbox.borrow_mut() =
                    Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));
            },
            |_, _, _, _| (),
        )
        .absolute()
        .inset_0()
        .size_full()
    }
    pub(crate) fn hovered(&self, window: &Window) -> bool {
        self.hitbox
            .borrow()
            .as_ref()
            .is_some_and(|hitbox| hitbox.is_hovered_at(window.mouse_position(), window))
    }
    fn owns_keyboard(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    fn captured(&self, editor: &Editor, cx: &App) -> Option<CapturedInput> {
        self.capture
            .as_ref()
            .and_then(|capture| capture(editor, cx))
    }
}

/// Unknown focus belongs to a control, including future controls. Only declared
/// editor surfaces opt in to ordinary hovered commands. Text keeps global keys.
#[derive(Clone, Copy)]
enum KeyboardOwner {
    Editor,
    Text,
    Control,
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
    pub viewport_capture: Rc<Cell<Option<(editor_core::ViewportInteractionToken, bool)>>>,
    pub timeline_capture: Rc<
        Cell<
            Option<(
                crate::panels::timeline::TimelineOperation,
                crate::workspace::status_bar::TimelineCaptureKind,
            )>,
        >,
    >,
    pub timeline_pan_tool: Rc<Cell<bool>>,
}
impl InputRouter {
    pub fn deactivate(&self) {
        self.inside.set(false);
        self.menu_target.set(None);
        self.viewport_capture.set(None);
        self.timeline_capture.set(None);
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
    fn input_regions(&self, window: &Window, cx: &App) -> Vec<(LocalTarget, InputRegion)> {
        let mut regions = Vec::new();
        for id in self.editor.viewport_ids() {
            if !self.viewport_displayed(id, window, cx) {
                continue;
            }
            if let Some(surface) = self.viewport_surface(id, cx) {
                let surface = surface.read(cx);
                if let Some(binding) = surface.binding(&self.editor) {
                    regions.push((LocalTarget::Viewport(binding), surface.input_region.clone()));
                }
            }
        }
        if let Some(dock) = self.input_dock() {
            for (&kind, region) in &dock.input_regions {
                if dock.displayed(kind, cx) {
                    regions.push((LocalTarget::Panel(kind), region.clone()));
                }
            }
            if dock.displayed(editor_core::PanelKind::AnimationTimeline, cx) {
                let timeline = dock.timeline.read(cx);
                if let Some(binding) = self.editor.timeline_binding(timeline.id) {
                    regions.push((
                        LocalTarget::Timeline(binding),
                        timeline.input_region.clone(),
                    ));
                }
            }
        }
        regions
    }
    #[cfg(test)]
    pub(crate) fn hovered_target(&self, window: &Window, cx: &App) -> Option<LocalTarget> {
        self.hovered_in(&self.input_regions(window, cx), window)
    }
    fn hovered_in(
        &self,
        regions: &[(LocalTarget, InputRegion)],
        window: &Window,
    ) -> Option<LocalTarget> {
        if !self.input_router.inside.get() || !window.is_visible() {
            return None;
        }
        regions
            .iter()
            .find(|(_, region)| region.hovered(window))
            .map(|(target, _)| *target)
    }
    pub(crate) fn cancel_input_captures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Include hidden instances during lifecycle cleanup; their core bindings
        // may already be invalid, but their local pointer state still needs release.
        let mut regions: Vec<_> = self
            .editor
            .viewport_ids()
            .into_iter()
            .filter_map(|id| self.viewport_surface(id, cx))
            .map(|surface| surface.read(cx).input_region.clone())
            .collect();
        if let Some(dock) = &self.dock {
            regions.push(dock.timeline.read(cx).input_region.clone());
        }
        let captures: Vec<_> = regions
            .iter()
            .filter_map(|region| region.captured(&self.editor, cx))
            .collect();
        for capture in captures {
            capture.cancel(self, window, cx);
        }
    }
    fn keyboard_owner(
        &self,
        regions: &[(LocalTarget, InputRegion)],
        window: &mut Window,
        cx: &mut App,
    ) -> KeyboardOwner {
        if self.modal
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
            || cx.has_active_drag()
        {
            return KeyboardOwner::Control;
        }
        if self.has_focused_input(window, cx) {
            return KeyboardOwner::Text;
        }
        if window.focused(cx).is_none()
            || self.focus.is_focused(window)
            || regions
                .iter()
                .any(|(_, region)| region.owns_keyboard(window))
        {
            KeyboardOwner::Editor
        } else {
            KeyboardOwner::Control
        }
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
            (CommandScope::Document, Some(LocalTarget::Viewport(binding))) => Command {
                id,
                source: CommandSource::Shortcut,
                target: CommandTarget::Document(binding.document),
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
        if event.prefer_character_input {
            return;
        }
        let regions = self.input_regions(window, cx);
        let keyboard = self.keyboard_owner(&regions, window, cx);
        if matches!(keyboard, KeyboardOwner::Control) {
            return;
        }
        let text = matches!(keyboard, KeyboardOwner::Text);
        let capture = regions
            .iter()
            .filter_map(|(_, region)| region.captured(&self.editor, cx))
            .find(|capture| capture.valid);
        if event.keystroke.key == "escape"
            && capture
                .as_ref()
                .is_some_and(|capture| capture.cancel_key == CaptureCancelKey::Escape)
        {
            capture.unwrap().cancel(self, window, cx);
            self.redraw(cx);
            cx.stop_propagation();
            return;
        }
        let target = capture
            .map(|capture| capture.target)
            .or_else(|| self.hovered_in(&regions, window))
            .or_else(|| {
                regions
                    .iter()
                    .find(|(_, region)| region.owns_keyboard(window))
                    .map(|(target, _)| *target)
            });
        let resolved = if event.keystroke.key == "escape"
            && !event.keystroke.modifiers.modified()
            && !text
            && matches!(
                target,
                Some(LocalTarget::Viewport(binding))
                    if binding.kind == ViewportKind::Modeling
                        && self.editor.active_tool() == ToolId::Select
                        && !self.editor.gesture_active_for(binding)
                        && self.editor.viewport_interaction_binding() != Some(binding)
            ) {
            Some(self.command_at(CommandId::Deselect, target))
        } else {
            cx.global::<settings::Settings>()
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
                .map(|binding| self.command_at(binding.command, target))
        };
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
        let regions = self.input_regions(window, cx);
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
