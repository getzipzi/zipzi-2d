//! Animation editing UI. Geometry and input use the same cached, clipped scene.
mod controls;
mod drawing;
mod input;
mod inspector;
mod navigation;
mod transport;

use crate::{EditorView, t, ui::compact};
use editor_core::{
    AnimationPreviewMode, DocumentId, TimelineCandidateIdentity, TimelineCategory, TimelineChannel,
    TimelineCommand, TimelineDiagnostic, TimelineDisplayMode, TimelineFilter, TimelineKeyRef,
    TimelinePasteTarget, TimelineProjection, TimelineRow, TimelineRowId, TimelineTrack,
    TimelineViewState, WorkspaceKind, timeline_time,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc};

const ROW: f32 = 24.;
const LEFT: f32 = 248.;
const INSPECTOR: f32 = 220.;
const RULER: f32 = 24.;
const COMPACT_WIDTH: f32 = 900.;
const COMPACT_LEFT: f32 = 160.;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    document: DocumentId,
    workspace: u64,
    model: u64,
    clip: String,
    mode: AnimationPreviewMode,
}

pub struct Timeline {
    ui_scale: f32,
    #[cfg(test)]
    renders: u64,
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    tracks: Entity<drawing::TracksLayer>,
    search: Entity<InputState>,
    search_placeholder: String,
    context: Option<Target>,
    graph: Rc<Cell<Bounds<Pixels>>>,
    ruler: Rc<Cell<Bounds<Pixels>>>,
    body_height: f32,
    gesture: Option<input::Gesture>,
    edge_scroll: Option<Task<()>>,
    pointer: Point<Pixels>,
    pan_tool: bool,
    number_edit: Option<inspector::NumberEdit>,
    fields: HashMap<inspector::Field, inspector::FieldControl>,
    slider: Option<inspector::ChannelSlider>,
    scene: Rc<std::cell::RefCell<drawing::SceneCache>>,
    value_range: (f64, f64),
    range_selection: Vec<TimelineRowId>,
    normalized: bool,
    seconds: bool,
    show_properties: bool,
    compact: bool,
    header_compact: bool,
    header_tiny: bool,
    inspector_open: Option<bool>,
    error: Option<String>,
    _search_events: Subscription,
    _observe: Subscription,
    _activation: Subscription,
}

pub(crate) fn init(cx: &mut App) {
    input::bind_keys(cx);
}

impl Timeline {
    fn row_height(&self) -> f32 {
        ROW * self.ui_scale
    }

    fn inspector_visible(&self) -> bool {
        self.inspector_open.unwrap_or(!self.compact)
    }

    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let scene = Rc::new(std::cell::RefCell::new(drawing::SceneCache::default()));
            let search =
                cx.new(|cx| InputState::new(window, cx).placeholder(t!("timeline.search")));
            Self {
                #[cfg(test)]
                renders: 0,
                ui_scale: compact::scale(cx),
                tracks: cx.new(|_| drawing::TracksLayer::new(scene.clone())),
                _search_events: cx.subscribe_in(
                    &search,
                    window,
                    |this, input, event, window, cx| {
                        if matches!(event, InputEvent::Change) && this.allowed(window, cx) {
                            let text = input.read(cx).value().to_string();
                            this.change_view(move |view| view.search = text, cx);
                        }
                    },
                ),
                _observe: cx.observe_in(
                    &owner.upgrade().unwrap(),
                    window,
                    |this, _, window, cx| {
                        this.sync(window, cx);
                        cx.notify();
                    },
                ),
                _activation: cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active()
                        && (this.gesture.is_some() || this.number_edit.is_some())
                    {
                        this.cancel(window, cx);
                    }
                }),
                owner,
                focus: cx.focus_handle(),
                search,
                search_placeholder: t!("timeline.search").into_owned(),
                context: None,
                graph: Rc::new(Cell::new(Bounds::default())),
                ruler: Rc::new(Cell::new(Bounds::default())),
                body_height: 180.,
                gesture: None,
                edge_scroll: None,
                pointer: Point::default(),
                pan_tool: false,
                number_edit: None,
                fields: HashMap::new(),
                slider: None,
                scene,
                value_range: (-1., 1.),
                range_selection: vec![],
                normalized: false,
                seconds: false,
                show_properties: false,
                compact: false,
                header_compact: false,
                header_tiny: false,
                inspector_open: None,
                error: None,
            }
        })
    }

    fn target(&self, cx: &App) -> Option<Target> {
        let owner = self.owner.upgrade()?;
        let view = owner.read(cx);
        let animation = view.editor.animation()?;
        (view.editor.workspace() == WorkspaceKind::Animation
            && !view.modal
            && !view.editor.is_busy()
            && !view.editor.exiting())
        .then(|| Target {
            document: view.editor.active_id().unwrap(),
            workspace: view.editor.workspace_revision(),
            model: animation.model_generation,
            clip: animation
                .motion
                .as_ref()
                .map(|motion| motion.clip_id.clone())
                .unwrap_or_default(),
            mode: animation.preview_mode,
        })
    }
    fn allowed(&self, window: &mut Window, cx: &mut App) -> bool {
        !window.has_active_dialog(cx) && self.target(cx).is_some()
    }
    fn editable(&self, window: &mut Window, cx: &mut App) -> bool {
        self.allowed(window, cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let view = owner.read(cx);
                !view.editor.animation().unwrap().playing
                    && view
                        .editor
                        .timeline_projection()
                        .is_some_and(|p| p.editable)
            })
    }
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scale = compact::scale(cx);
        if self.ui_scale != scale {
            self.cancel(window, cx);
            self.ui_scale = scale;
        }
        let placeholder = t!("timeline.search");
        if self.search_placeholder != placeholder.as_ref() {
            self.search_placeholder = placeholder.into_owned();
            self.search.update(cx, |input, cx| {
                input.set_placeholder(self.search_placeholder.clone(), window, cx)
            });
        }
        let target = self.target(cx);
        if target != self.context {
            if let Some(edit) = self.number_edit.take() {
                cx.stop_active_drag(window);
                if let Some(control) = self.fields.get(&edit.field) {
                    control
                        .number
                        .update(cx, |number, cx| number.reset_drag(cx));
                }
            }
            let same_clip = self
                .context
                .as_ref()
                .zip(target.as_ref())
                .is_some_and(|(old, new)| {
                    old.document == new.document
                        && old.workspace == new.workspace
                        && old.clip == new.clip
                        && old.mode == new.mode
                });
            self.gesture = None;
            self.edge_scroll = None;
            self.number_edit = None;
            if same_clip {
                let projection = self
                    .owner
                    .upgrade()
                    .and_then(|owner| owner.read(cx).editor.timeline_projection());
                self.fields.retain(|field, _| {
                    projection
                        .as_ref()
                        .is_some_and(|projection| field.exists(projection))
                });
            } else {
                self.fields.clear();
                self.slider = None;
                self.range_selection.clear();
                self.error = None;
            }
            self.scene.borrow_mut().scene = None;
            if target.as_ref().is_none_or(|target| target.clip.is_empty()) {
                self.tracks.update(cx, |tracks, cx| tracks.clear(cx));
            }
            self.context = target;
        }
        let active = self.owner.upgrade().and_then(|owner| {
            owner
                .read(cx)
                .editor
                .timeline_edit()
                .map(|edit| edit.identity.edit)
        });
        if self
            .number_edit
            .as_ref()
            .is_some_and(|edit| active != Some(edit.token.edit))
        {
            self.number_edit = None;
            window.focus(&self.focus, cx);
            cx.stop_active_drag(window);
            let numbers: Vec<_> = self
                .fields
                .values()
                .map(|field| field.number.clone())
                .collect();
            for number in numbers {
                number.update(cx, |number, cx| number.reset_drag(cx));
            }
        }
        if self
            .gesture
            .as_ref()
            .and_then(input::Gesture::token)
            .is_some_and(|token| active != Some(token.edit))
        {
            self.gesture = None;
            self.edge_scroll = None;
        }
        if window.has_active_dialog(cx) && (self.gesture.is_some() || self.number_edit.is_some()) {
            self.cancel(window, cx);
        }
        if let Some(owner) = self.owner.upgrade() {
            let search = owner
                .read(cx)
                .editor
                .timeline_view()
                .map(|view| view.search.clone())
                .unwrap_or_default();
            if !self.search.read(cx).focus_handle(cx).is_focused(window)
                && self.search.read(cx).value().as_ref() != search
            {
                self.search
                    .update(cx, |input, cx| input.set_value(search, window, cx));
            }
        }
    }
    fn change_view(&mut self, change: impl FnOnce(&mut TimelineViewState), cx: &mut Context<Self>) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let _ = self.owner.update(cx, |owner, cx| {
            let result = owner.editor.update_timeline_view(target.document, change);
            owner.complete_edit(result, cx);
        });
    }
    fn execute(&mut self, command: TimelineCommand, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable(window, cx) {
            return;
        }
        self.finish_number(cx);
        let Some(target) = self.target(cx) else {
            return;
        };
        let result = self.owner.update(cx, |owner, cx| {
            let result = owner.editor.execute_timeline(target.document, command);
            let error = result.as_ref().err().map(ToString::to_string);
            owner.complete_edit(result, cx);
            error
        });
        self.error = result.ok().flatten();
        cx.notify();
    }
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_active_drag(window);
        self.gesture = None;
        self.edge_scroll = None;
        self.number_edit = None;
        let numbers: Vec<_> = self
            .fields
            .values()
            .map(|field| field.number.clone())
            .collect();
        for number in numbers {
            number.update(cx, |number, cx| number.reset_drag(cx));
        }
        self.error = None;

        let _ = self.owner.update(cx, |owner, cx| {
            owner.editor.cancel_timeline_edit();
            owner.redraw(cx);
        });
        cx.notify();
    }
    fn fit(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let view = owner.read(cx);
        let Some(projection) = view.editor.timeline_projection() else {
            return;
        };
        let end = projection.display_end.max(
            view.editor
                .animation()
                .and_then(|a| a.result_playback_position())
                .map_or(0., |p| f64::from(p.period)),
        );
        self.range_selection.clear();
        self.change_view(|view| view.time_range = (0., end.max(0.001)), cx);
    }
    fn select_keys(
        &mut self,
        keys: Vec<TimelineKeyRef>,
        additive: bool,
        reveal: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(projection) = owner.read(cx).editor.timeline_projection() else {
            return;
        };
        if reveal && !additive {
            let Some(target) = self.target(cx) else {
                return;
            };
            let _ = self.owner.update(cx, |owner, cx| {
                let result = owner.editor.reveal_timeline_keys(target.document, &keys);
                owner.complete_edit(result, cx);
            });
            return;
        }
        self.change_view(
            move |view| {
                if !additive {
                    view.keys.clear();
                    view.events.clear();
                    view.channels.clear();
                }
                if reveal {
                    if let Some(key) = keys.first().and_then(|key| projection.key(key)) {
                        view.reveal_time(key.time);
                    }
                }
                for key in keys {
                    view.keys.insert(key.clone());
                    let mut parents = Vec::<&TimelineRow>::new();
                    for row in projection.rows.iter() {
                        while parents
                            .last()
                            .is_some_and(|parent| parent.depth >= row.depth)
                        {
                            parents.pop();
                        }
                        if row.keys.contains(&key) {
                            view.channels.insert(row.id.clone());
                            if reveal {
                                for parent in &parents {
                                    view.collapsed.remove(&parent.id);
                                }
                                view.reveal_row = Some(row.id.clone());
                            }
                        }
                        parents.push(row);
                    }
                }
            },
            cx,
        );
    }
}

impl EventEmitter<PanelEvent> for Timeline {}
impl Focusable for Timeline {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for Timeline {
    fn panel_name(&self) -> &'static str {
        "AnimationTimeline"
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !active {
            self.deactivate(window, cx);
        }
    }

    fn on_removed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.deactivate(window, cx);
    }
}
impl Panel for Timeline {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("timeline.title")
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Timeline {
    pub(crate) fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let token = self
            .gesture
            .as_ref()
            .and_then(input::Gesture::token)
            .or_else(|| self.number_edit.as_ref().map(|edit| edit.token));
        let seeking = matches!(self.gesture, Some(input::Gesture::Seek));
        if self.gesture.is_some() || self.number_edit.is_some() {
            cx.stop_active_drag(window);
        }
        self.gesture = None;
        self.edge_scroll = None;
        self.number_edit = None;
        for field in self.fields.values() {
            field.number.update(cx, |number, cx| number.reset_drag(cx));
        }
        if token.is_none() && !seeking {
            return;
        }
        let owner = self.owner.clone();
        let document = self.context.as_ref().map(|target| target.document);
        // Dock callbacks may run inside an EditorView update. Resolve the owner
        // after it is released, and never cancel a newer gesture or another tab.
        cx.defer(move |cx| {
            let _ = owner.update(cx, |owner, cx| {
                if owner.editor.active_id() != document {
                    return;
                }
                if token.is_some_and(|token| {
                    owner
                        .editor
                        .timeline_edit()
                        .is_some_and(|edit| edit.identity.edit == token.edit)
                }) {
                    owner.editor.cancel_timeline_edit();
                }
                if seeking {
                    owner.editor.pause_animation();
                    owner.animation_anchor = None;
                }
                owner.redraw(cx);
            });
        });
    }
}

#[cfg(test)]
impl Timeline {
    #[allow(dead_code)] // Used by the real Metal acceptance harness.
    pub(crate) fn renders(&self) -> u64 {
        self.renders
    }
    pub(crate) fn scene_paints(&self) -> u64 {
        self.scene.borrow().paints
    }
    pub(crate) fn scene_builds(&self) -> u64 {
        self.scene.borrow().builds
    }
    pub(crate) fn handle_position(&self, key: &TimelineKeyRef) -> Option<Point<Pixels>> {
        self.scene
            .borrow()
            .scene
            .as_ref()?
            .markers
            .iter()
            .find_map(|marker| match &marker.hit {
                drawing::Hit::Handle {
                    end,
                    handle: kasane_sdk::MotionBezierHandle::Control1,
                } if end == key => Some(self.graph.get().origin + marker.position),
                _ => None,
            })
    }
    pub(crate) fn pointer_active(&self) -> bool {
        self.gesture.is_some()
    }
    pub(crate) fn key_position(&self, key: &TimelineKeyRef) -> Option<Point<Pixels>> {
        self.scene
            .borrow()
            .scene
            .as_ref()?
            .markers
            .iter()
            .find_map(|marker| match &marker.hit {
                drawing::Hit::Key(reference) if reference == key => {
                    Some(self.graph.get().origin + marker.position)
                }
                _ => None,
            })
    }
}

impl Timeline {
    // Header controls are siblings of the content now. Both regions must use
    // the same edit guards, commit-on-click behavior and keyboard actions.
    fn interactive_frame<E: StatefulInteractiveElement>(
        &self,
        frame: E,
        cx: &mut Context<Self>,
    ) -> E {
        let pointer_target = self.target(cx);
        let release_target = pointer_target.clone();
        frame
            .capture_any_mouse_down(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if this.target(cx) != pointer_target {
                    cx.stop_propagation();
                    return;
                }
                this.finish_number_outside(event.position, cx);
            }))
            .capture_any_mouse_up(cx.listener(move |this, _: &MouseUpEvent, _, cx| {
                let same = this
                    .target(cx)
                    .as_ref()
                    .zip(release_target.as_ref())
                    .is_some_and(|(a, b)| {
                        a.document == b.document
                            && a.workspace == b.workspace
                            && a.clip == b.clip
                            && a.mode == b.mode
                    });
                if !same {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(Self::escape))
            .capture_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    if !this.allowed(window, cx) {
                        cx.propagate();
                        return;
                    }
                    match action.id {
                        editor_core::CommandId::CancelProperty
                            if this.number_edit.is_some() || this.gesture.is_some() =>
                        {
                            this.cancel(window, cx);
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }
                        editor_core::CommandId::CommitProperty if this.number_edit.is_some() => {
                            this.finish_number(cx);
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }
                        _ => cx.propagate(),
                    }
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    if this.allowed(window, cx)
                        && (this.gesture.is_some() || this.number_edit.is_some())
                    {
                        this.cancel(window, cx);
                        window.focus(&this.focus, cx);
                        cx.stop_propagation();
                    } else {
                        cx.propagate();
                    }
                },
            ))
            .on_action(cx.listener(|this, _: &input::DeleteKeys, window, cx| {
                this.key_command("delete", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::CopyKeys, window, cx| {
                this.key_command("copy", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::PasteKeys, window, cx| {
                this.key_command("paste", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::FrameLeft, window, cx| {
                this.key_command("left", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::FrameRight, window, cx| {
                this.key_command("right", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::PreviousKey, window, cx| {
                this.key_command("previous", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::NextKey, window, cx| {
                this.key_command("next", window, cx)
            }))
            .on_action(cx.listener(|this, _: &input::Start, window, cx| {
                this.key_command("start", window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &input::End, window, cx| this.key_command("end", window, cx)),
            )
    }
}

impl crate::docking::header::PanelHeader for Timeline {
    fn header_filtered(&self, cx: &App) -> bool {
        !self.search.read(cx).value().is_empty()
    }

    fn header_min_width(&self) -> Pixels {
        px(120.)
    }

    fn header_resized(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let compact = width < compact::scaled(COMPACT_WIDTH, cx);
        let tiny = width < crate::ui::compact::scaled(260., cx);
        if self.header_compact != compact || self.header_tiny != tiny {
            self.header_tiny = tiny;
            self.header_compact = compact;
            cx.notify();
        }
    }

    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync(window, cx);
        let toolbar = self.toolbar(window, cx);
        self.interactive_frame(
            div()
                .id("timeline-header")
                .test_support()
                .flex()
                .items_center()
                .w_full()
                .h_full()
                .track_focus(&self.focus)
                .key_context("KasaneTimeline"),
            cx,
        )
        .child(toolbar)
        .into_any_element()
    }
}

impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.sync(window, cx);
        if self.graph.get().size.height > px(0.) {
            self.body_height = self.graph.get().size.height.as_f32();
        }
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let projection = owner.read(cx).editor.timeline_projection();
        let mut state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        let rows = owner
            .read(cx)
            .editor
            .timeline_visible_rows()
            .unwrap_or_else(|| Arc::from([]));
        state.scroll = state
            .scroll
            .min((rows.len() as f64 * self.row_height() as f64 - self.body_height as f64).max(0.));
        let time = owner.read(cx).editor.animation().map_or(0., |a| {
            if a.playing {
                a.presented
                    .as_ref()
                    .and_then(|frame| {
                        frame
                            .motion_positions
                            .iter()
                            .find(|p| a.motion.as_ref().is_some_and(|m| m.clip_id == p.motion_id))
                    })
                    .map_or(a.edit_time, |p| p.local_time)
            } else {
                a.edit_time
            }
        });
        let panel_entity = cx.weak_entity();
        let left_width = (if self.compact { COMPACT_LEFT } else { LEFT }) * self.ui_scale;
        let panel = div()
            .id("animation-timeline")
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs()
            .track_focus(&self.focus)
            .key_context("KasaneTimeline")
            .on_prepaint(move |bounds, window, cx| {
                let compact = bounds.size.width < compact::scaled(COMPACT_WIDTH, cx);
                if panel_entity
                    .upgrade()
                    .is_some_and(|panel| panel.read(cx).compact != compact)
                {
                    window.defer(cx, move |_, cx| {
                        let _ = panel_entity.update(cx, |panel, cx| {
                            if panel.compact != compact {
                                panel.compact = compact;
                                cx.notify();
                            }
                        });
                    });
                }
            });
        let mut panel = self.interactive_frame(panel, cx);
        let weak = cx.weak_entity();
        panel = panel.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let pressing = weak.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && !bounds.contains(&event.position)
                        {
                            let _ = pressing.update(cx, |this, cx| {
                                if this.allowed(window, cx) {
                                    this.finish_number_outside(event.position, cx);
                                }
                            });
                        }
                    });
                    let moving = weak.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ =
                                moving.update(cx, |this, cx| this.move_pointer(event, window, cx));
                        }
                    });
                    let releasing = weak.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture
                            && matches!(event.button, MouseButton::Left | MouseButton::Middle)
                        {
                            let _ =
                                releasing.update(cx, |this, cx| this.release_pointer(window, cx));
                        }
                    });
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        let Some(projection) = projection else {
            return panel
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(t!("timeline.empty")),
                )
                .into_any_element();
        };
        self.update_range(&projection, &state);
        if let Some(reveal) = &state.reveal_row {
            if let Some(index) = rows.iter().position(|row| &row.id == reveal) {
                let top = index as f64 * self.row_height() as f64;
                let scroll = if top < state.scroll {
                    top
                } else if top + self.row_height() as f64 > state.scroll + self.body_height as f64 {
                    (top + self.row_height() as f64 - self.body_height as f64).max(0.)
                } else {
                    state.scroll
                };
                let document = self.context.as_ref().map(|c| c.document);
                let weak = self.owner.clone();
                let reveal = reveal.clone();
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |owner, cx| {
                        if let Some(document) = document {
                            let _ = owner
                                .editor
                                .acknowledge_timeline_reveal(document, &reveal, scroll);
                            owner.redraw(cx);
                        }
                    });
                });
            }
        }
        let inspector = self
            .inspector_visible()
            .then(|| self.render_inspector(&projection, &state, f64::from(time), window, cx));
        let (inline_inspector, floating_inspector) = if self.compact {
            (None, inspector)
        } else {
            (inspector, None)
        };
        let ruler_bounds = self.ruler.clone();
        let mut left = div()
            .id("timeline-rows")
            .test_support()
            .w(px(left_width))
            .h_full()
            .relative()
            .overflow_hidden()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border);
        let first = (state.scroll as f32 / self.row_height()).floor() as usize;
        let count = (self.body_height / self.row_height()).ceil() as usize + 2;
        for (index, row) in rows.iter().enumerate().skip(first).take(count) {
            left = left.child(self.row(
                row,
                &state,
                &projection,
                f64::from(time),
                index as f32 * self.row_height() - state.scroll as f32,
                window,
                cx,
            ));
        }
        let range = self.value_range;
        let head = time;
        let seconds = self.seconds;
        let head_range = state.time_range;
        let marquee = self.gesture.as_ref().and_then(|gesture| match gesture {
            input::Gesture::Marquee {
                origin, current, ..
            } => Some((*origin, *current)),
            _ => None,
        });
        let primary = cx.theme().primary;
        let graph_bounds = self.graph.clone();
        self.tracks.update(cx, |tracks, cx| {
            tracks.configure(
                projection.clone(),
                state.clone(),
                rows.clone(),
                range,
                seconds,
                graph_bounds,
                cx,
            )
        });
        let ruler_projection = projection.clone();
        let ruler_state = state.clone();
        let graph = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("animation-seek")
                    .test_support()
                    .h(compact::rem(RULER))
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("timeline-ruler")
                            .test_support()
                            .size_full()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_seek))
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        ruler_bounds.set(bounds);
                                    },
                                    move |bounds, _, window, cx| {
                                        drawing::paint_ruler(
                                            bounds,
                                            &ruler_projection,
                                            &ruler_state,
                                            head,
                                            seconds,
                                            window,
                                            cx,
                                        );
                                    },
                                )
                                .size_full(),
                            ),
                    ),
            )
            .child(
                div()
                    .id("timeline-graph")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_graph))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::start_pan))
                    .when(self.pan_tool, |graph| graph.cursor(CursorStyle::ClosedHand))
                    .on_scroll_wheel(cx.listener(Self::wheel))
                    .child(
                        self.tracks
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    )
                    .child(
                        div()
                            .id("timeline-overlay")
                            .test_support()
                            .absolute()
                            .inset_0()
                            .size_full()
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, _| {
                                        drawing::paint_head(
                                            bounds, head_range, head, marquee, primary, window,
                                        )
                                    },
                                )
                                .size_full(),
                            ),
                    ),
            );
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .overflow_hidden()
            .child(
                div()
                    .w(px(left_width))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(compact::rem(RULER))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                crate::panels::toolbar::PanelToolbarSearch::new(&self.search)
                                    .id("timeline-search")
                                    .label(t!("timeline.search").into_owned()),
                            )
                            .child(
                                Button::new("timeline-filter")
                                    .small()
                                    .ghost()
                                    .icon(IconName::ListFilter)
                                    .selected(state.filter == TimelineFilter::Animated)
                                    .tooltip(if state.filter == TimelineFilter::Animated {
                                        t!("timeline.animated")
                                    } else {
                                        t!("timeline.all")
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.allowed(window, cx) {
                                            this.change_view(
                                                |view| {
                                                    view.filter =
                                                        if view.filter == TimelineFilter::Animated {
                                                            TimelineFilter::All
                                                        } else {
                                                            TimelineFilter::Animated
                                                        }
                                                },
                                                cx,
                                            );
                                        }
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .on_scroll_wheel(cx.listener(Self::wheel))
                            .child(left),
                    ),
            )
            .child(graph)
            .when_some(inline_inspector, |body, inspector| body.child(inspector));
        panel = panel
            .child(body)
            .when_some(floating_inspector, |panel, inspector| {
                panel.child(inspector)
            });
        panel.into_any_element()
    }
}
