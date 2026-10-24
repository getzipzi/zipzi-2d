//! Shared palette gestures. Core plans own collection rules and placement;
//! this view owns hit testing, feedback, hover expansion and scrolling.
use crate::{EditorView, authoring::AuthoringContext};
mod destination;
mod reparent;
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{prelude::*, *};
use std::{cell::Cell, rc::Rc, time::Duration};
use zipzi_editor_core::{
    DocumentId, ObjectKind, ObjectRef, PaletteMove, PaletteMovePlan,
    PalettePosition as DropPosition, PaletteProjection,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SortItem {
    Tree { deformer: bool, object: ObjectRef },
    TreeRoot { deformer: bool },
    Parameter(String),
    Resource(ObjectRef),
    ResourceRoot { motion: bool },
    Group(Option<String>),
}
impl SortItem {
    fn reference(&self) -> Option<ObjectRef> {
        match self {
            Self::Tree { object, .. } | Self::Resource(object) => Some(object.clone()),
            Self::Parameter(id) => Some(ObjectRef::new(ObjectKind::Parameter, id)),
            Self::Group(Some(id)) => Some(ObjectRef::new(ObjectKind::ParameterGroup, id)),
            _ => None,
        }
    }
    fn resource_family(&self) -> Option<bool> {
        match self {
            Self::Resource(o) => Some(matches!(
                o.kind,
                ObjectKind::MotionGroup | ObjectKind::MotionRegistration
            )),
            Self::ResourceRoot { motion } => Some(*motion),
            _ => None,
        }
    }
    fn resource(&self) -> bool {
        matches!(self, Self::Resource(_) | Self::ResourceRoot { .. })
    }
    fn projection(&self) -> PaletteProjection {
        match self {
            Self::Tree { deformer: true, .. } | Self::TreeRoot { deformer: true } => {
                PaletteProjection::Deformation
            }
            Self::Tree { .. } | Self::TreeRoot { .. } => PaletteProjection::Organization,
            _ => PaletteProjection::Parameters,
        }
    }
}

fn drop_position(
    source: &SortItem,
    target: &SortItem,
    pointer: Point<Pixels>,
    bounds: Bounds<Pixels>,
) -> DropPosition {
    if matches!(
        target,
        SortItem::TreeRoot { .. } | SortItem::Group(None) | SortItem::ResourceRoot { .. }
    ) {
        return DropPosition::Inside;
    }
    let container = matches!(target, SortItem::Resource(o) if o.kind == ObjectKind::MotionGroup && matches!(source, SortItem::Resource(s) if s.kind == ObjectKind::MotionRegistration))
        || matches!(target, SortItem::Group(Some(_)))
        || matches!(target, SortItem::Tree { deformer: false, object } if object.kind == ObjectKind::Part)
        || matches!(target, SortItem::Tree { deformer: true, object } if object.kind == ObjectKind::Deformer);
    if container
        && pointer.y >= bounds.top() + bounds.size.height * 0.25
        && pointer.y <= bounds.bottom() - bounds.size.height * 0.25
    {
        DropPosition::Inside
    } else if pointer.y < bounds.center().y {
        DropPosition::Before
    } else {
        DropPosition::After
    }
}

#[derive(Clone, PartialEq, Eq)]
struct PaletteDrag {
    document: DocumentId,
    source: SortItem,
}

enum SortPlan {
    Palette(PaletteMovePlan),
    Resource(zipzi_editor_core::AnimationLibraryPlan),
}
impl SortPlan {
    fn is_noop(&self) -> bool {
        match self {
            Self::Palette(p) => p.is_noop(),
            Self::Resource(p) => p.is_noop(),
        }
    }
    fn requires_preservation(&self) -> bool {
        matches!(self, Self::Palette(p) if p.requires_preservation())
    }
}
struct DragPreview {
    label: SharedString,
    reason: Option<String>,
}
impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .text_xs()
            .rounded_md()
            .shadow_md()
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .child(self.label.clone())
            .when_some(self.reason.clone(), |element, reason| {
                element.child(
                    div()
                        .id("palette-drop-reason")
                        .test_support()
                        .text_color(cx.theme().muted_foreground)
                        .child(reason),
                )
            })
    }
}

pub(super) struct SortState {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    active: Option<PaletteDrag>,
    context: Option<AuthoringContext>,
    sources: Vec<ObjectRef>,
    // Keep one hovered plan, not a plan-sized document snapshot per visited row.
    cached: Option<(SortItem, DropPosition, Result<SortPlan, String>)>,
    preview: Option<WeakEntity<DragPreview>>,
    hover: Option<(SortItem, DropPosition)>,
    hover_serial: u64,
    scroll: Option<(UniformListScrollHandle, i32, usize)>,
    scroll_task: Option<Task<()>>,
    plain_scroll: Option<(ScrollHandle, i32)>,
    _activation: Subscription,
    #[cfg(test)]
    pub(super) plans_built: usize,
}
impl SortState {
    pub fn new(
        owner: WeakEntity<EditorView>,
        focus: FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self {
            owner,
            focus,
            active: None,
            context: None,
            sources: Vec::new(),
            cached: None,
            preview: None,
            hover: None,
            hover_serial: 0,
            scroll: None,
            scroll_task: None,
            plain_scroll: None,
            #[cfg(test)]
            plans_built: 0,
            _activation: cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if !window.is_window_active() {
                    this.cancel(window, cx);
                }
            }),
        })
    }
    fn allowed(&self, document: DocumentId, window: &mut Window, cx: &mut App) -> bool {
        !window.has_active_dialog(cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let owner = owner.read(cx);
                owner.editor.active_id() == Some(document)
                    && !owner.modal
                    && !owner.editor.is_busy()
                    && !owner.editor.exiting()
            })
    }
    fn begin(
        &mut self,
        drag: PaletteDrag,
        preview: WeakEntity<DragPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear(cx);
        if !self.allowed(drag.document, window, cx) {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if !owner.update(cx, |owner, cx| {
            owner.prepare_property_inputs(drag.document, window, cx)
        }) {
            return;
        }
        self.focus.focus(window, cx);
        let editor = &owner.read(cx).editor;
        let Some(source) = drag.source.reference() else {
            return;
        };
        self.sources = match &drag.source {
            SortItem::Tree { .. } => editor
                .document()
                .map(|document| &document.context().selections)
                .filter(|selection| selection.contains(&source))
                .cloned()
                .unwrap_or_else(|| vec![source]),
            _ => vec![source],
        };
        self.context = AuthoringContext::capture(editor);
        self.preview = Some(preview);
        self.active = Some(drag);
        cx.notify();
    }
    fn current(&self, drag: &PaletteDrag, window: &mut Window, cx: &mut App) -> bool {
        self.active.as_ref() == Some(drag)
            && self.allowed(drag.document, window, cx)
            && self.owner.upgrade().is_some_and(|owner| {
                AuthoringContext::capture(&owner.read(cx).editor) == self.context
            })
    }
    fn can_target(
        &mut self,
        drag: &PaletteDrag,
        target: &SortItem,
        position: DropPosition,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if !self.current(drag, window, cx) {
            return false;
        }
        if drag.source.projection() != target.projection()
            || drag.source.resource_family() != target.resource_family()
        {
            self.cached = Some((
                target.clone(),
                position,
                Err(crate::t!("authoring.move_collection").into_owned()),
            ));
            return false;
        }
        if self
            .cached
            .as_ref()
            .is_none_or(|(item, at, _)| item != target || *at != position)
        {
            #[cfg(test)]
            {
                self.plans_built += 1;
            }
            let owner = self.owner.upgrade().unwrap();
            let doc = owner.read(cx).editor.document().unwrap();
            let result = if drag.source.resource() {
                doc.plan_animation_resource_move(
                    &self.sources[0],
                    target.reference().as_ref(),
                    position,
                )
                .map(SortPlan::Resource)
            } else {
                doc.plan_palette_move(PaletteMove {
                    projection: drag.source.projection(),
                    sources: self.sources.clone(),
                    target: target.reference(),
                    position,
                })
                .map(SortPlan::Palette)
            }
            .map_err(|error| move_error(&error));
            self.cached = Some((target.clone(), position, result));
        }
        self.cached
            .as_ref()
            .is_some_and(|(_, _, result)| result.as_ref().is_ok_and(|plan| !plan.is_noop()))
    }
    fn feedback(&self, reason: Option<String>, cx: &mut App) {
        if let Some(preview) = &self.preview {
            let _ = preview.update(cx, |preview, cx| {
                if preview.reason != reason {
                    preview.reason = reason;
                    cx.notify();
                }
            });
        }
    }
    fn clear(&mut self, cx: &mut Context<Self>) {
        let changed = self.active.take().is_some() || self.hover.is_some();
        self.hover = None;
        self.hover_serial += 1;
        self.context = None;
        self.sources.clear();
        self.cached = None;
        self.preview = None;
        self.scroll = None;
        self.scroll_task = None;
        self.plain_scroll = None;
        if changed {
            cx.notify();
        }
    }
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_some() {
            cx.stop_active_drag(window);
            self.clear(cx);
        }
    }
    pub fn reconcile(
        &mut self,
        document: Option<DocumentId>,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = &self.active {
            if !cx.has_active_drag() {
                self.clear(cx);
            } else if !enabled || document != Some(drag.document) || !self.current(drag, window, cx)
            {
                self.cancel(window, cx);
            }
        }
    }
    fn drop(
        &mut self,
        drag: &PaletteDrag,
        target: &SortItem,
        position: DropPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_target(drag, target, position, window, cx) {
            self.clear(cx);
            return;
        }
        let Some(context) = self.context else {
            return;
        };
        if drag.source.resource() {
            let source = self.sources[0].clone();
            self.clear(cx);
            if let Some(owner) = self.owner.upgrade() {
                owner.update(cx, |state, cx| {
                    state.run_animation_action(
                        context,
                        crate::authoring::animation::AnimationAction::Move(
                            source,
                            target.reference(),
                            position,
                        ),
                        window,
                        cx,
                    )
                });
            }
            return;
        }
        let request = PaletteMove {
            projection: drag.source.projection(),
            sources: self.sources.clone(),
            target: target.reference(),
            position,
        };
        let preservation = self.cached.as_ref().is_some_and(|(_, _, result)| {
            result
                .as_ref()
                .is_ok_and(|plan| plan.requires_preservation())
        });
        self.clear(cx);
        if preservation {
            let owner = self.owner.clone();
            let document = drag.document;
            window.defer(cx, move |window, cx| {
                reparent::open(
                    owner,
                    document,
                    context,
                    request.sources.clone(),
                    Some(request),
                    window,
                    cx,
                );
            });
            return;
        }
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |owner, cx| {
                let reveal = request.sources.last().cloned();
                let projection = request.projection;
                let result = owner.execute_authoring(context, window, cx, |editor, document| {
                    // Replan on release, then let the SDK revalidate the version.
                    let plan = editor.document().unwrap().plan_palette_move(request)?;
                    editor.execute_palette_move(document, &plan)
                });
                match result {
                    Ok(result) => {
                        if let Some(object) = reveal {
                            let followup = owner.present_authoring_result(
                                drag.document,
                                &result,
                                crate::authoring::AuthoringPresentation::Moved {
                                    object,
                                    projection,
                                },
                                window,
                                cx,
                            );
                            owner.complete_edit(followup.map(|_| ()), cx);
                        }
                        owner.redraw(cx);
                    }
                    Err(error) => {
                        owner.set_edit_error(crate::authoring::authoring_error(&error), cx)
                    }
                }
            });
        }
    }
}

pub(super) fn drag_source<E: StatefulInteractiveElement + FluentBuilder + Styled>(
    element: E,
    state: &Entity<SortState>,
    document: DocumentId,
    source: SortItem,
    label: String,
    enabled: bool,
) -> E {
    let state = state.clone();
    element.when(enabled, |element| {
        element.on_drag(
            PaletteDrag { document, source },
            move |drag, _, window, cx| {
                let preview = cx.new(|_| DragPreview {
                    label: label.clone().into(),
                    reason: None,
                });
                state.update(cx, |state, cx| {
                    state.begin(drag.clone(), preview.downgrade(), window, cx)
                });
                // GPUI installs the active drag after this callback returns.
                let pending = state.clone();
                window.defer(cx, move |window, cx| {
                    if pending.read(cx).active.is_none() {
                        cx.stop_active_drag(window);
                    } else {
                        cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
                    }
                });
                preview
            },
        )
    })
}

pub(super) type ExpandTarget = Rc<dyn Fn(&SortItem, &mut Window, &mut App)>;

pub(super) fn drop_target<
    E: StatefulInteractiveElement + Styled + ParentElement + FluentBuilder,
>(
    element: E,
    state: &Entity<SortState>,
    target: SortItem,
    expand: Option<ExpandTarget>,
    cx: &App,
) -> E {
    let hovered = state
        .read(cx)
        .hover
        .as_ref()
        .filter(|(item, _)| item == &target)
        .map(|(_, position)| *position);
    let checking = state.clone();
    let checking_target = target.clone();
    let moving = state.clone();
    let moving_target = target.clone();
    let dropping = state.clone();
    let expand_on_drop = expand.clone();
    let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
    let measuring = bounds.clone();
    let checking_bounds = bounds.clone();
    element
        .relative()
        .child(
            canvas(move |bounds, _, _| measuring.set(bounds), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .can_drop(move |value, window, cx| {
            value.downcast_ref::<PaletteDrag>().is_some_and(|drag| {
                checking.update(cx, |state, cx| {
                    let position = drop_position(
                        &drag.source,
                        &checking_target,
                        window.mouse_position(),
                        checking_bounds.get(),
                    );
                    checking_bounds.get().contains(&window.mouse_position())
                        && state.can_target(drag, &checking_target, position, window, cx)
                })
            })
        })
        .on_drag_move(move |event: &DragMoveEvent<PaletteDrag>, window, cx| {
            moving.update(cx, |state, cx| {
                let drag = event.drag(cx).clone();
                let position = drop_position(
                    &drag.source,
                    &moving_target,
                    event.event.position,
                    event.bounds,
                );
                let hover = if event.bounds.contains(&event.event.position)
                    && state.can_target(&drag, &moving_target, position, window, cx)
                {
                    Some((moving_target.clone(), position))
                } else {
                    None
                };
                if event.bounds.contains(&event.event.position) {
                    let reason = state
                        .cached
                        .as_ref()
                        .filter(|(item, at, _)| item == &moving_target && *at == position)
                        .and_then(|(_, _, result)| result.as_ref().err())
                        .cloned();
                    state.feedback(reason, cx);
                } else if state
                    .cached
                    .as_ref()
                    .is_some_and(|(item, _, _)| item == &moving_target)
                {
                    state.feedback(None, cx);
                }
                // Other rows receive move events too; only clear this row's own target.
                if hover.is_none()
                    && state
                        .hover
                        .as_ref()
                        .is_none_or(|(item, _)| item != &moving_target)
                {
                    return;
                }
                if state.hover != hover {
                    state.hover = hover.clone();
                    state.hover_serial += 1;
                    let hover_serial = state.hover_serial;
                    cx.notify();
                    if let (Some((item, DropPosition::Inside)), Some(expand)) =
                        (hover, expand.clone())
                    {
                        cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(600))
                                .await;
                            _ = this.update_in(cx, |state, window, cx| {
                                if cx.has_active_drag()
                                    && state.hover_serial == hover_serial
                                    && state.hover == Some((item.clone(), DropPosition::Inside))
                                {
                                    expand(&item, window, cx);
                                }
                            });
                        })
                        .detach();
                    }
                }
            });
        })
        .on_drop(move |drag: &PaletteDrag, window, cx| {
            // Scrolling can move another row under a stationary pointer.
            let position =
                drop_position(&drag.source, &target, window.mouse_position(), bounds.get());
            dropping.update(cx, |state, cx| {
                let allowed = state.can_target(drag, &target, position, window, cx);
                state.drop(drag, &target, position, window, cx);
                if allowed && position == DropPosition::Inside {
                    if let Some(expand) = &expand_on_drop {
                        expand(&target, window, cx);
                    }
                }
            })
        })
        .when_some(hovered, |element, position| {
            let color = cx.theme().primary;
            element.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .when(position == DropPosition::Inside, |line| {
                        line.top_0().bottom_0().border_1()
                    })
                    .when(position == DropPosition::Before, |line| {
                        line.top_0().h(px(2.)).bg(color)
                    })
                    .when(position == DropPosition::After, |line| {
                        line.bottom_0().h(px(2.)).bg(color)
                    })
                    .border_color(color),
            )
        })
}

pub(super) fn scroll_area(
    list: UniformList,
    state: &Entity<SortState>,
    scroll: &UniformListScrollHandle,
    count: usize,
) -> UniformList {
    let state = state.clone();
    let scroll = scroll.clone();
    list.on_drag_move(move |event: &DragMoveEvent<PaletteDrag>, window, cx| {
        state.update(cx, |state, cx| {
            let pointer = event.event.position;
            let direction = if !event.bounds.contains(&pointer) {
                0
            } else if pointer.y
                < event.bounds.top() + crate::ui::compact::scaled(crate::ui::compact::ROW, cx)
            {
                -1
            } else if pointer.y
                > event.bounds.bottom() - crate::ui::compact::scaled(crate::ui::compact::ROW, cx)
            {
                1
            } else {
                0
            };
            state.scroll = (direction != 0).then(|| (scroll.clone(), direction, count));
            if direction != 0 && state.scroll_task.is_none() {
                state.scroll_task = Some(cx.spawn_in(window, async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(60))
                            .await;
                        let running = this
                            .update_in(cx, |state, window, cx| {
                                if !cx.has_active_drag() || state.active.is_none() {
                                    state.scroll_task = None;
                                    return false;
                                }
                                let Some((scroll, direction, count)) = &state.scroll else {
                                    state.scroll_task = None;
                                    return false;
                                };
                                let top = (-scroll.0.borrow().base_handle.offset().y
                                    / crate::ui::compact::scaled(crate::ui::compact::ROW, cx))
                                .floor() as usize;
                                let index = (top as i32 + direction)
                                    .clamp(0, count.saturating_sub(1) as i32)
                                    as usize;
                                scroll.scroll_to_item_strict(index, ScrollStrategy::Top);
                                window.refresh();
                                true
                            })
                            .unwrap_or(false);
                        if !running {
                            break;
                        }
                    }
                }));
            }
        });
    })
}

/// Ordinary resource lists use the same drag owner and timer as virtual lists.
pub(super) fn scroll_div<E: StatefulInteractiveElement>(
    list: E,
    state: &Entity<SortState>,
    scroll: &ScrollHandle,
) -> E {
    let state = state.clone();
    let scroll = scroll.clone();
    list.on_drag_move(move |event: &DragMoveEvent<PaletteDrag>, window, cx| {
        state.update(cx, |state, cx| {
            let pointer = event.event.position;
            let edge = crate::ui::compact::scaled(crate::ui::compact::ROW, cx);
            let direction = if !event.bounds.contains(&pointer) {
                0
            } else if pointer.y < event.bounds.top() + edge {
                -1
            } else if pointer.y > event.bounds.bottom() - edge {
                1
            } else {
                0
            };
            state.plain_scroll = (direction != 0).then(|| (scroll.clone(), direction));
            if direction != 0 && state.scroll_task.is_none() {
                state.scroll_task = Some(cx.spawn_in(window, async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(40))
                            .await;
                        let running = this
                            .update_in(cx, |state, window, cx| {
                                if !cx.has_active_drag() || state.active.is_none() {
                                    state.scroll_task = None;
                                    return false;
                                }
                                let Some((scroll, direction)) = &state.plain_scroll else {
                                    state.scroll_task = None;
                                    return false;
                                };
                                let mut offset = scroll.offset();
                                offset.y -= crate::ui::compact::scaled(crate::ui::compact::ROW, cx)
                                    * *direction as f32;
                                scroll.set_offset(offset);
                                window.refresh();
                                true
                            })
                            .unwrap_or(false);
                        if !running {
                            break;
                        }
                    }
                }));
            }
        });
    })
}

pub(super) fn panel_actions(element: Stateful<Div>, state: &Entity<SortState>) -> Stateful<Div> {
    let keyboard = state.clone();
    let state = state.clone();
    element
        .capture_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && keyboard.read(cx).active.is_some() {
                keyboard.update(cx, |state, cx| state.cancel(window, cx));
                cx.stop_propagation();
            }
        })
        .capture_action(move |action: &crate::actions::RunCommand, window, cx| {
            if action.id == zipzi_editor_core::CommandId::CancelProperty
                && state.read(cx).active.is_some()
            {
                state.update(cx, |state, cx| state.cancel(window, cx));
                cx.stop_propagation();
            }
        })
}

pub(crate) fn open_resource_destination(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    context: AuthoringContext,
    source: ObjectRef,
    window: &mut Window,
    cx: &mut App,
) {
    destination::open(
        owner,
        document,
        context,
        PaletteProjection::Parameters,
        vec![source],
        window,
        cx,
    );
}

impl EditorView {
    /// Palette commands are resolved from exact list focus, independently of
    /// the hovered viewport. Inputs and popup controls retain their own keys.
    pub(crate) fn handle_palette_command(
        &mut self,
        command: zipzi_editor_core::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_focused_input(window, cx) || cx.has_active_drag() {
            return;
        }
        let Some(dock) = &self.dock else {
            return;
        };
        let Some(document) = command
            .document()
            .filter(|id| Some(*id) == self.editor.active_id())
        else {
            return;
        };
        for kind in [
            zipzi_editor_core::PanelKind::AnimationPose,
            zipzi_editor_core::PanelKind::AnimationPhysics,
        ] {
            if dock.displayed(kind, cx)
                && dock.animation_inspectors[&kind]
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            {
                let object = if kind == zipzi_editor_core::PanelKind::AnimationPose {
                    dock.animation_inspectors[&kind]
                        .read(cx)
                        .business_movement_object()
                        .or_else(|| self.editor.active_inspection().and_then(|i| i.object()))
                } else {
                    self.editor.active_inspection().and_then(|i| i.object())
                };
                if let (Some(object), Some(context)) =
                    (object, AuthoringContext::capture(&self.editor))
                {
                    if command.id == zipzi_editor_core::CommandId::PaletteMoveTo {
                        if matches!(
                            object.kind,
                            zipzi_editor_core::ObjectKind::PoseEntry
                                | zipzi_editor_core::ObjectKind::PoseLink
                                | zipzi_editor_core::ObjectKind::PhysicsInput
                                | zipzi_editor_core::ObjectKind::PhysicsOutput
                        ) {
                            self.open_business_destination(context, object, false, window, cx);
                        }
                    } else {
                        let action = zipzi_editor_core::BusinessAction::Step(
                            object,
                            command.id == zipzi_editor_core::CommandId::PaletteMoveDown,
                        );
                        if crate::authoring::business::availability(&self.editor, &action)
                            .is_some_and(|a| a.enabled)
                        {
                            self.run_business_action(context, action, window, cx);
                        }
                    }
                }
                return;
            }
        }
        let mut resource_focus = None;
        for (kind, panel) in [
            (
                zipzi_editor_core::PanelKind::AnimationMotions,
                &dock.animation_motions,
            ),
            (
                zipzi_editor_core::PanelKind::AnimationExpressions,
                &dock.animation_expressions,
            ),
        ] {
            if dock.displayed(kind, cx) && panel.read(cx).focus_handle(cx).is_focused(window) {
                let object = panel.read(cx).movement_object();
                resource_focus = Some(object);
                break;
            }
        }
        if let Some(object) = resource_focus {
            if let (Some(object), Some(context)) = (object, AuthoringContext::capture(&self.editor))
            {
                if command.id == zipzi_editor_core::CommandId::PaletteMoveTo {
                    self.run_animation_action(
                        context,
                        crate::authoring::animation::AnimationAction::MoveTo(object),
                        window,
                        cx,
                    );
                } else {
                    self.run_animation_action(
                        context,
                        crate::authoring::animation::AnimationAction::Step(
                            object,
                            command.id == zipzi_editor_core::CommandId::PaletteMoveDown,
                        ),
                        window,
                        cx,
                    );
                }
            }
            return;
        }
        let doc = self.editor.document().unwrap();
        let (projection, object) = if dock.displayed(zipzi_editor_core::PanelKind::MeshTree, cx)
            && dock.mesh_tree.read(cx).focus_handle(cx).is_focused(window)
        {
            (
                PaletteProjection::Organization,
                dock.mesh_tree.read(cx).movement_object(),
            )
        } else if dock.displayed(zipzi_editor_core::PanelKind::Deformers, cx)
            && dock
                .deformer_tree
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        {
            (
                PaletteProjection::Deformation,
                dock.deformer_tree.read(cx).movement_object(),
            )
        } else if dock.displayed(zipzi_editor_core::PanelKind::Parameters, cx)
            && dock.parameters.read(cx).focus_handle(cx).is_focused(window)
        {
            (
                PaletteProjection::Parameters,
                dock.parameters.read(cx).movement_object(),
            )
        } else {
            return;
        };
        let Some(object) = object else {
            return;
        };
        let locate = object.clone();
        let sources = if doc.context().selections.contains(&object) {
            doc.context().selections.clone()
        } else {
            vec![object]
        };
        if command.id == zipzi_editor_core::CommandId::PaletteMoveTo {
            if !self.prepare_property_inputs(document, window, cx) {
                return;
            }
            let Some(context) = AuthoringContext::capture(&self.editor) else {
                return;
            };
            let owner = cx.weak_entity();
            window.defer(cx, move |window, cx| {
                destination::open(owner, document, context, projection, sources, window, cx)
            });
            return;
        }
        if !self.prepare_property_inputs(document, window, cx) {
            return;
        }
        let Some(context) = AuthoringContext::capture(&self.editor) else {
            return;
        };
        let result = self.execute_authoring(context, window, cx, |editor, document| {
            let plan = editor.document().unwrap().plan_palette_steps(
                projection,
                &sources,
                command.id == zipzi_editor_core::CommandId::PaletteMoveDown,
            )?;
            editor.execute_palette_move(document, &plan)
        });
        match result {
            Ok(result) => {
                self.clear_edit_error(cx);
                self.redraw(cx);
                if result.receipt.changed {
                    self.locate_palette_step(document, projection, locate, window, cx);
                }
            }
            Err(error) => self.set_edit_error(move_error(&error), cx),
        }
    }

    fn locate_palette_step(
        &self,
        document: DocumentId,
        projection: PaletteProjection,
        object: ObjectRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.defer_authoring_location(document, object, Some(projection), false, false, window, cx);
    }
}

/// The same core placement adapter serves explicit menu steps and pointer moves.
pub(super) fn move_menu(
    mut menu: gpui_kit::component::menu::PopupMenu,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    projection: PaletteProjection,
    object: ObjectRef,
    cx: &App,
) -> gpui_kit::component::menu::PopupMenu {
    use gpui_kit::component::menu::PopupMenuItem;
    let Some(entity) = owner.upgrade() else {
        return menu;
    };
    let state = entity.read(cx);
    if state.editor.active_id() != Some(document) {
        return menu;
    }
    let context = AuthoringContext::capture(&state.editor);
    let locate = object.clone();
    let sources = state
        .editor
        .document()
        .map(|doc| {
            if doc.context().selections.contains(&object) {
                doc.context().selections.clone()
            } else {
                vec![object]
            }
        })
        .unwrap_or_default();
    for (down, label) in [
        (false, crate::t!("authoring.move_up")),
        (true, crate::t!("authoring.move_down")),
    ] {
        let plan = state
            .editor
            .document()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))
            .and_then(|document| document.plan_palette_steps(projection, &sources, down));
        let enabled = plan.as_ref().is_ok_and(|plan| !plan.is_noop());
        let reason = plan.as_ref().err().map(|error| move_error(error));
        let label: SharedString = reason
            .map_or_else(|| label.to_string(), |reason| format!("{label} · {reason}"))
            .into();
        let sources = sources.clone();
        let locate = locate.clone();
        let owner = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(label)
                .action(Box::new(crate::actions::RunCommand::new(if down {
                    zipzi_editor_core::CommandId::PaletteMoveDown
                } else {
                    zipzi_editor_core::CommandId::PaletteMoveUp
                })))
                .disabled(!enabled)
                .on_click(move |_, window, cx| {
                    let (Some(context), Some(owner)) = (context, owner.upgrade()) else {
                        return;
                    };
                    owner.update(cx, |owner, cx| {
                        let result =
                            owner.execute_authoring(context, window, cx, |editor, document| {
                                let plan = editor
                                    .document()
                                    .unwrap()
                                    .plan_palette_steps(projection, &sources, down)?;
                                editor.execute_palette_move(document, &plan)
                            });
                        match result {
                            Ok(result) => {
                                owner.clear_edit_error(cx);
                                owner.redraw(cx);
                                if result.receipt.changed {
                                    owner.locate_palette_step(
                                        document,
                                        projection,
                                        locate.clone(),
                                        window,
                                        cx,
                                    );
                                }
                            }
                            Err(error) => owner.set_edit_error(move_error(&error), cx),
                        }
                    });
                }),
        );
    }
    {
        menu = menu.item(
            PopupMenuItem::new(crate::t!("authoring.move_to"))
                .action(Box::new(crate::actions::RunCommand::new(
                    zipzi_editor_core::CommandId::PaletteMoveTo,
                )))
                .disabled(context.is_none())
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let owner = owner.clone();
                        let sources = sources.clone();
                        window.defer(cx, move |window, cx| {
                            destination::open(
                                owner, document, context, projection, sources, window, cx,
                            )
                        });
                    }
                }),
        );
    }
    menu
}

fn move_error(error: &anyhow::Error) -> String {
    crate::authoring::authoring_error(error)
}
