//! Animation editing UI. Geometry and input use the same cached, clipped scene.
mod controls;
mod drawing;
mod input;
mod inspector;
mod lifecycle;
use lifecycle::Target;
pub(crate) use lifecycle::TimelineOperation;
mod navigation;
mod transport;
mod view;

use crate::{EditorView, t, ui::compact};
use editor_core::{
    AnimationPreviewMode, DocumentId, TimelineCandidateIdentity, TimelineCategory, TimelineChannel,
    TimelineCommand, TimelineDiagnostic, TimelineDisplayMode, TimelineFilter, TimelineKeyRef,
    TimelineProjection, TimelineRow, TimelineRowId, TimelineTrack, TimelineViewState,
    timeline_time,
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

pub struct Timeline {
    pub(crate) id: editor_core::TimelineId,
    pub(crate) input_region: crate::input_routing::InputRegion,
    timeline_capture: std::cell::OnceCell<
        Rc<Cell<Option<(TimelineOperation, crate::workspace::status_bar::TimelineCaptureKind)>>>,
    >,
    timeline_pan_tool: std::cell::OnceCell<Rc<Cell<bool>>>,
    ui_scale: f32,
    #[cfg(any(test, feature = "test-support"))]
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
}

impl Timeline {
    fn current_capture(
        &self,
    ) -> Option<(TimelineOperation, crate::workspace::status_bar::TimelineCaptureKind)> {
        use crate::workspace::status_bar::TimelineCaptureKind;
        self.operation().and_then(|op| {
            if let Some(gesture) = &self.gesture {
                let kind = match gesture {
                    input::Gesture::Pan { .. } => TimelineCaptureKind::Pan,
                    input::Gesture::Seek(_) => TimelineCaptureKind::Seek,
                    input::Gesture::Marquee { .. } => TimelineCaptureKind::Marquee,
                    input::Gesture::Keys { .. } => TimelineCaptureKind::Keys,
                    input::Gesture::Handle { .. } => TimelineCaptureKind::Handle,
                    input::Gesture::Event { .. } => TimelineCaptureKind::Event,
                };
                return Some((op, kind));
            }
            if self.number_edit.is_some() {
                return Some((op, TimelineCaptureKind::Number));
            }
            None
        })
    }

    pub(super) fn sync_capture_hint(&self) {
        if let Some(cell) = self.timeline_capture.get() {
            cell.set(self.current_capture());
        }
        if let Some(cell) = self.timeline_pan_tool.get() {
            cell.set(self.pan_tool);
        }
    }

    pub(super) fn sync_status_bar(&self, cx: &mut App) {
        if let Some(owner) = self.owner.upgrade() {
            {
                let router = &owner.read(cx).input_router;
                let capture = self
                    .timeline_capture
                    .get_or_init(|| router.timeline_capture.clone());
                capture.set(self.current_capture());
                let pan_tool = self
                    .timeline_pan_tool
                    .get_or_init(|| router.timeline_pan_tool.clone());
                pan_tool.set(self.pan_tool);
            }
            crate::workspace::status_bar::StatusBar::sync_owner(&owner, cx);
        }
    }

    fn row_height(&self) -> f32 {
        ROW * self.ui_scale
    }

    fn inspector_visible(&self) -> bool {
        self.inspector_open.unwrap_or(!self.compact)
    }

    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let upgraded = owner.upgrade().unwrap();
            cx.on_release(|this: &mut Self, cx| {
                if let Some(cell) = this.timeline_capture.get() {
                    cell.set(None);
                }
                if let Some(cell) = this.timeline_pan_tool.get() {
                    cell.set(false);
                }
                let panel = this.id;
                let owner = this.owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |owner, cx| {
                        owner.editor.unregister_timeline(panel);
                        owner.redraw(cx);
                    });
                });
            })
            .detach();
            let scene = Rc::new(std::cell::RefCell::new(drawing::SceneCache::default()));
            let search =
                cx.new(|cx| InputState::new(window, cx).placeholder(t!("timeline.search")));
            let focus = cx.focus_handle();
            let timeline = cx.weak_entity();
            let input_region = crate::input_routing::InputRegion::new(focus.clone()).with_capture(
                move |editor, cx| {
                    let entity = timeline.upgrade()?;
                    let operation = entity.read(cx).operation()?;
                    let timeline = timeline.clone();
                    Some(crate::input_routing::CapturedInput::new(
                        crate::input_routing::LocalTarget::Timeline(operation.binding()),
                        operation.valid(editor),
                        crate::input_routing::CaptureCancelKey::Escape,
                        move |window, cx| {
                            timeline
                                .update(cx, |timeline, cx| {
                                    if timeline.operation() != Some(operation) {
                                        return None;
                                    }
                                    let cancellation = timeline.cancel_local(window, cx);
                                    Some(Box::new(move |owner: &mut EditorView| {
                                        cancellation.apply(owner)
                                    })
                                        as crate::input_routing::CoreCleanup)
                                })
                                .ok()
                                .flatten()
                        },
                    ))
                },
            );
            Self {
                id: editor_core::TimelineId::new(),
                input_region,
                timeline_capture: std::cell::OnceCell::new(),
                timeline_pan_tool: std::cell::OnceCell::new(),
                #[cfg(any(test, feature = "test-support"))]
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
                    &upgraded,
                    window,
                    |this, _, window, cx| {
                        this.sync(window, cx);
                        cx.notify();
                    },
                ),
                owner,
                focus,
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

#[cfg(any(test, feature = "test-support"))]
impl Timeline {
    pub(crate) fn renders(&self) -> u64 {
        self.renders
    }
    pub(crate) fn scene_paints(&self) -> u64 {
        self.scene.borrow().paints
    }
    pub(crate) fn scene_builds(&self) -> u64 {
        self.scene.borrow().builds
    }
    #[cfg(test)]
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
    #[cfg(test)]
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
