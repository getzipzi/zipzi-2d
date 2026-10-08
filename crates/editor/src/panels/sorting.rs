//! Shared palette DnD using the GPUI Kit interaction APIs.
use crate::EditorView;
use editor_core::{
    DocumentId, EditorCommand, ObjectKind, ObjectRef, OrganizationDestination,
    ParameterDestination, ReorderPosition, TreeNode,
};
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{prelude::*, *};
use std::{cell::Cell, collections::HashMap, rc::Rc, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SortItem {
    Tree { deformer: bool, object: ObjectRef },
    TreeRoot,
    Parameter(String),
    Group(Option<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropPosition {
    Before,
    After,
    Inside,
}

fn drop_position(
    source: &SortItem,
    target: &SortItem,
    pointer: Point<Pixels>,
    bounds: Bounds<Pixels>,
) -> DropPosition {
    if matches!(
        (source, target),
        (SortItem::Group(Some(_)), SortItem::Group(None))
    ) {
        return DropPosition::Before;
    }
    if matches!(
        (source, target),
        (SortItem::Parameter(_), SortItem::Group(_))
            | (
                SortItem::Tree {
                    deformer: false,
                    ..
                },
                SortItem::TreeRoot
            )
    ) {
        DropPosition::Inside
    } else if matches!(target, SortItem::Tree { deformer: false, object } if object.kind == ObjectKind::Part)
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

struct DragPreview(SharedString);
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
            .child(self.0.clone())
    }
}

pub(super) struct SortState {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    active: Option<PaletteDrag>,
    parents: HashMap<SortItem, String>,
    hover: Option<(SortItem, DropPosition)>,
    revision: Option<u64>,
    scroll: Option<(UniformListScrollHandle, i32, usize)>,
    scroll_task: Option<Task<()>>,
    _activation: Subscription,
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
            parents: HashMap::new(),
            hover: None,
            revision: None,
            scroll: None,
            scroll_task: None,
            _activation: cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if !window.is_window_active() {
                    this.cancel(window, cx);
                }
            }),
        })
    }

    fn allowed(&self, document: DocumentId, window: &mut Window, cx: &mut App) -> bool {
        let dialog = window.has_active_dialog(cx);
        self.owner.upgrade().is_some_and(|owner| {
            let owner = owner.read(cx);
            owner.editor.active_id() == Some(document)
                && !owner.modal
                && !owner.editor.is_busy()
                && !dialog
        })
    }

    fn begin(&mut self, drag: PaletteDrag, window: &mut Window, cx: &mut Context<Self>) {
        self.clear(cx);
        if !self.allowed(drag.document, window, cx) {
            return;
        }
        self.focus.focus(window, cx);
        let owner = self.owner.upgrade().unwrap();
        let editor = &owner.read(cx).editor;
        self.revision = editor.tree_revision();
        match &drag.source {
            SortItem::Tree { deformer, .. } => {
                fn parents(
                    nodes: &[TreeNode],
                    parent: &str,
                    deformer: bool,
                    result: &mut HashMap<SortItem, String>,
                ) {
                    for node in nodes {
                        if node.diagnostic.is_none() {
                            if let Some(object) = node.reference() {
                                result
                                    .insert(SortItem::Tree { deformer, object }, parent.to_owned());
                            }
                        }
                        parents(&node.children, &node.id, deformer, result);
                    }
                }
                let tree = if *deformer {
                    editor.deformer_tree()
                } else {
                    editor.mesh_tree()
                };
                parents(&tree, "", *deformer, &mut self.parents);
                if !deformer {
                    self.parents.insert(SortItem::TreeRoot, String::new());
                }
            }
            _ => {
                if let Some(document) = editor.document() {
                    self.parents.insert(SortItem::Group(None), String::new());
                    for group in document.parameter_groups() {
                        self.parents.insert(
                            SortItem::Group(Some(group.id)),
                            group.parent_id.unwrap_or_default(),
                        );
                    }
                    for parameter in document.parameters() {
                        self.parents.insert(
                            SortItem::Parameter(parameter.id),
                            parameter.group_id.unwrap_or_default(),
                        );
                    }
                }
            }
        }
        self.active = Some(drag);
        cx.notify();
    }

    fn can_target(
        &self,
        drag: &PaletteDrag,
        target: &SortItem,
        position: DropPosition,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.active.as_ref() != Some(drag)
            || !self.allowed(drag.document, window, cx)
            || self
                .owner
                .upgrade()
                .is_none_or(|owner| owner.read(cx).editor.tree_revision() != self.revision)
            || drag.source == *target
            || !self.parents.contains_key(&drag.source)
            || !self.parents.contains_key(target)
        {
            return false;
        }
        match (&drag.source, target) {
            (SortItem::Tree { deformer: true, .. }, SortItem::Tree { deformer: true, .. }) => {
                self.parents.get(&drag.source) == self.parents.get(target)
            }
            (
                SortItem::Tree {
                    deformer: false,
                    object: source,
                },
                SortItem::Tree {
                    deformer: false,
                    object: target_object,
                },
            ) => {
                let mut parent = if position == DropPosition::Inside {
                    if target_object.kind != ObjectKind::Part {
                        return false;
                    }
                    target_object.id.as_str()
                } else {
                    let parent = self.parents[target].as_str();
                    if (parent.is_empty() || parent == "unowned")
                        && (source.kind == ObjectKind::Part)
                            != (target_object.kind == ObjectKind::Part)
                    {
                        return false;
                    }
                    parent
                };
                while !parent.is_empty() && parent != "unowned" {
                    if parent == source.id {
                        return false;
                    }
                    let key = SortItem::Tree {
                        deformer: false,
                        object: ObjectRef::new(ObjectKind::Part, parent),
                    };
                    let Some(next) = self.parents.get(&key) else {
                        return false;
                    };
                    parent = next;
                }
                true
            }
            (
                SortItem::Tree {
                    deformer: false, ..
                },
                SortItem::TreeRoot,
            ) => true,
            (SortItem::Parameter(_), SortItem::Parameter(_) | SortItem::Group(_)) => true,
            (SortItem::Group(Some(_)), SortItem::Group(Some(_))) => {
                self.parents.get(target).is_some_and(String::is_empty)
                    || self.parents.get(&drag.source) == self.parents.get(target)
            }
            (SortItem::Group(Some(_)), SortItem::Group(None)) => true,
            _ => false,
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let changed = self.active.take().is_some() || self.hover.is_some();
        self.hover = None;
        self.parents.clear();
        self.scroll = None;
        self.scroll_task = None;
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
            } else if !enabled
                || document != Some(drag.document)
                || !self.allowed(drag.document, window, cx)
                || self
                    .owner
                    .upgrade()
                    .is_none_or(|owner| owner.read(cx).editor.tree_revision() != self.revision)
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
            return;
        }
        let reorder = if position == DropPosition::Before {
            ReorderPosition::Before
        } else {
            ReorderPosition::After
        };
        let command = match (&drag.source, target) {
            (
                SortItem::Tree {
                    deformer: false,
                    object,
                },
                SortItem::TreeRoot,
            ) => EditorCommand::MoveOrganization {
                source: object.clone(),
                destination: OrganizationDestination::Part(None),
            },
            (
                SortItem::Tree {
                    deformer: false,
                    object,
                },
                SortItem::Tree { object: target, .. },
            ) => EditorCommand::MoveOrganization {
                source: object.clone(),
                destination: if position == DropPosition::Inside {
                    OrganizationDestination::Part(Some(target.id.clone()))
                } else {
                    OrganizationDestination::Sibling {
                        target: target.clone(),
                        position: reorder,
                    }
                },
            },
            (SortItem::Tree { deformer, object }, SortItem::Tree { object: target, .. }) => {
                EditorCommand::ReorderTree {
                    deformer: *deformer,
                    source: object.clone(),
                    target: target.clone(),
                    position: reorder,
                }
            }
            (SortItem::Parameter(id), SortItem::Parameter(target)) => {
                EditorCommand::MoveParameter {
                    id: id.clone(),
                    destination: ParameterDestination::Parameter {
                        id: target.clone(),
                        position: reorder,
                    },
                }
            }
            (SortItem::Parameter(id), SortItem::Group(group)) => EditorCommand::MoveParameter {
                id: id.clone(),
                destination: ParameterDestination::Group(group.clone()),
            },
            (SortItem::Group(Some(source)), SortItem::Group(None)) => {
                EditorCommand::MoveParameterGroupToRoot(source.clone())
            }
            (SortItem::Group(Some(source)), SortItem::Group(Some(target))) => {
                EditorCommand::ReorderParameterGroup {
                    source: source.clone(),
                    target: target.clone(),
                    position: reorder,
                }
            }
            _ => return,
        };
        self.clear(cx);
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |owner, cx| {
                let result = owner.editor.execute_for(drag.document, command);
                owner.complete_edit(result, cx);
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
                state.update(cx, |state, cx| state.begin(drag.clone(), window, cx));
                // GPUI installs the active drag after this callback returns.
                window.defer(cx, |window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
                });
                cx.new(|_| DragPreview(label.clone().into()))
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
                    state.can_target(drag, &checking_target, position, window, cx)
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
                    cx.notify();
                    if let (Some((item, DropPosition::Inside)), Some(expand)) =
                        (hover, expand.clone())
                    {
                        cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(500))
                                .await;
                            _ = this.update_in(cx, |state, window, cx| {
                                if cx.has_active_drag()
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
                state.drop(drag, &target, position, window, cx)
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
            } else if pointer.y < event.bounds.top() + px(26.) {
                -1
            } else if pointer.y > event.bounds.bottom() - px(26.) {
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
                                let top = (-scroll.0.borrow().base_handle.offset().y / px(26.))
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
            if action.id == editor_core::CommandId::CancelProperty
                && state.read(cx).active.is_some()
            {
                state.update(cx, |state, cx| state.cancel(window, cx));
                cx.stop_propagation();
            }
        })
}
