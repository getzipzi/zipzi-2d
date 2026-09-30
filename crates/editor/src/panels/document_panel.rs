//! Independent Dock identities backed by the same document projection and context.
use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ObjectIndex, ObjectKind, ObjectRef, RelationKind};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocumentPanelKind {
    DrawOrder,
    Relations,
    Keyforms,
    BlendShapes,
    Glue,
    Motions,
    Expressions,
    Physics,
    Pose,
    Resources,
}
impl DocumentPanelKind {
    pub const ALL: [Self; 10] = [
        Self::DrawOrder,
        Self::Relations,
        Self::Keyforms,
        Self::BlendShapes,
        Self::Glue,
        Self::Motions,
        Self::Expressions,
        Self::Physics,
        Self::Pose,
        Self::Resources,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::DrawOrder => "DrawOrder",
            Self::Relations => "Relations",
            Self::Keyforms => "Keyforms",
            Self::BlendShapes => "BlendShapes",
            Self::Glue => "Glue",
            Self::Motions => "Motions",
            Self::Expressions => "Expressions",
            Self::Physics => "Physics",
            Self::Pose => "Pose",
            Self::Resources => "Resources",
        }
    }
    pub fn title(self) -> SharedString {
        match self {
            Self::DrawOrder => t!("editor.draw_order"),
            Self::Relations => t!("editor.relations"),
            Self::Keyforms => t!("editor.keyforms"),
            Self::BlendShapes => t!("editor.blend_shapes"),
            Self::Glue => t!("editor.glue"),
            Self::Motions => t!("editor.motions"),
            Self::Expressions => t!("editor.expressions"),
            Self::Physics => t!("editor.physics"),
            Self::Pose => t!("editor.pose"),
            Self::Resources => t!("editor.resources"),
        }
        .into()
    }
    fn includes(self, kind: ObjectKind) -> bool {
        use ObjectKind as K;
        match self {
            Self::BlendShapes => matches!(
                kind,
                K::BlendBinding | K::BlendKeyTable | K::BlendConstraint
            ),
            Self::Glue => kind == K::Glue,
            Self::Motions => matches!(kind, K::MotionGroup | K::Motion | K::MotionTrack),
            Self::Expressions => kind == K::Expression,
            Self::Physics => matches!(kind, K::Physics | K::PhysicsGroup),
            Self::Pose => matches!(kind, K::Pose | K::PoseGroup | K::PoseEntry),
            Self::Resources => matches!(kind, K::Texture | K::Attachment),
            _ => false,
        }
    }
}
pub struct DocumentPanel {
    owner: WeakEntity<EditorView>,
    pub kind: DocumentPanelKind,
    focus: FocusHandle,
    _observe: Subscription,
    _activation: Subscription,
    indirect: bool,
    bounds: Bounds<Pixels>,
    dragging: Option<(DocumentId, ObjectRef)>,
    bounds_binding: Option<(DocumentId, ObjectRef)>,
    row_sequence: usize,
}
impl DocumentPanel {
    pub fn new(
        owner: WeakEntity<EditorView>,
        kind: DocumentPanelKind,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            _activation: cx.observe_window_activation(window, |this: &mut Self, window, _| {
                if !window.is_window_active() {
                    this.dragging = None;
                }
            }),
            owner,
            kind,
            indirect: false,
            bounds: Bounds::default(),
            dragging: None,
            bounds_binding: None,
            row_sequence: 0,
        })
    }
    pub fn cancel_drag(&mut self) {
        self.dragging = None;
    }
    fn run(&self, document: DocumentId, command: EditorCommand, window: &mut Window, cx: &mut App) {
        if window.has_active_dialog(cx) {
            return;
        }
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |owner, cx| {
                if owner.modal
                    || owner.editor.is_busy()
                    || owner.editor.active_id() != Some(document)
                {
                    return;
                }
                let opens_binding = matches!(&command, EditorCommand::OpenBinding(_))
                    || matches!(&command,EditorCommand::Navigate(r) if r.kind.is_binding());
                let result = owner.editor.execute_for(document, command);
                if opens_binding && result.is_ok() {
                    owner.reveal_keyforms(window, cx);
                }
                owner.complete_edit(result, cx);
            });
        }
    }
    fn row(
        &mut self,
        index: &ObjectIndex,
        r: &ObjectRef,
        detail: String,
        document: DocumentId,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let r = r.clone();
        self.row_sequence += 1;
        let sequence = self.row_sequence;
        let theme = cx.theme();
        let info = index.get(&r);
        let name = index.name(&r).to_owned();
        let diagnostic = info.and_then(|i| i.diagnostic.as_ref()).cloned();
        let state = if info.is_some_and(|i| i.enabled == Some(false)) {
            t!("editor.object_disabled").into_owned()
        } else {
            String::new()
        };
        div()
            .id(SharedString::from(format!(
                "{}-{:?}-{}-{sequence}-{detail}",
                self.kind.name(),
                r.kind,
                r.id
            )))
            .test_support()
            .w_full()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .bg(if selected {
                theme.primary.opacity(0.15)
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(theme.muted))
            .cursor_pointer()
            .child(format!("{name} · {:?}", r.kind))
            .when(!detail.is_empty(), |s| {
                s.child(div().text_color(theme.muted_foreground).child(detail))
            })
            .when(!state.is_empty(), |s| {
                s.child(div().text_color(theme.muted_foreground).child(state))
            })
            .when_some(diagnostic, |s, d| {
                s.child(div().text_color(theme.yellow).child(d))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.run(document, EditorCommand::Navigate(r.clone()), window, cx)
            }))
            .into_any_element()
    }
    fn keyforms(
        &mut self,
        index: Arc<ObjectIndex>,
        context: editor_core::PanelContext,
        parameters: Vec<editor_core::ParameterValue>,
        document: DocumentId,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().size_full().min_h_0().flex().flex_col().gap_1().p_1();
        let Some(binding) = context
            .active_binding
            .as_ref()
            .and_then(|r| index.binding(r))
            .cloned()
        else {
            body = body.child(t!("editor.choose_binding"));
            for b in &index.bindings {
                if context.selection.as_ref() == Some(&b.target) {
                    body =
                        body.child(self.row(&index, &b.reference, b.layout(), document, false, cx));
                }
            }
            return body.into_any_element();
        };
        let b = binding.clone();
        body = body
            .child(format!(
                "{} · {}",
                index.name(&b.target),
                if b.blend { "BlendShape" } else { "Keyform" }
            ))
            .child(b.layout())
            .child(
                b.axes
                    .iter()
                    .map(|a| {
                        format!(
                            "{}: {:?}",
                            index.name(&ObjectRef::new(ObjectKind::Parameter, &a.parameter_id)),
                            a.keys
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" × "),
            );
        if !b.writable() {
            return body
                .child(t!("editor.legacy_binding_read_only"))
                .into_any_element();
        }
        if b.axes.is_empty() || b.axes.len() > 2 {
            return body.child(t!("editor.no_items")).into_any_element();
        }
        let axes: Vec<_> = b
            .axes
            .iter()
            .filter_map(|a| parameters.iter().find(|p| p.id == a.parameter_id).cloned())
            .collect();
        if axes.len() != b.axes.len() {
            return body
                .child(t!("editor.missing_reference"))
                .into_any_element();
        }
        let coordinate: Vec<_> = axes.iter().map(|p| p.value).collect();
        let stored = b.points.iter().any(|p| p == &coordinate);
        body = body.child(format!(
            "{}: {:?} · {}",
            t!("editor.preview_coordinates"),
            coordinate,
            if stored {
                t!("editor.stored_keyform")
            } else {
                t!("editor.interpolated_preview")
            }
        ));
        let fraction = |v: f32, p: &editor_core::ParameterValue| {
            if p.maximum > p.minimum {
                ((v - p.minimum) / (p.maximum - p.minimum)).clamp(0., 1.)
            } else {
                0.
            }
        };
        let mut grid = Vec::new();
        let theme = cx.theme().clone();
        for key in &b.axes[0].keys {
            grid.push(
                div()
                    .absolute()
                    .left(relative(fraction(*key, &axes[0])))
                    .top_0()
                    .w(px(1.))
                    .h_full()
                    .bg(theme.border)
                    .into_any_element(),
            );
        }
        if axes.len() == 2 {
            for key in &b.axes[1].keys {
                grid.push(
                    div()
                        .absolute()
                        .top(relative(1. - fraction(*key, &axes[1])))
                        .left_0()
                        .h(px(1.))
                        .w_full()
                        .bg(theme.border)
                        .into_any_element(),
                );
            }
        }
        for (i, point) in b.points.iter().enumerate() {
            let x = fraction(point[0], &axes[0]);
            let y = if axes.len() == 2 {
                1. - fraction(point[1], &axes[1])
            } else {
                0.5
            };
            let active = &coordinate == point;
            let reference = b.reference.clone();
            grid.push(
                div()
                    .id(SharedString::from(format!("keyform-{i}")))
                    .test_support()
                    .absolute()
                    .left(relative(x))
                    .top(relative(y))
                    .ml(-px(5.))
                    .mt(-px(5.))
                    .size(px(10.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.green)
                    .bg(if active { theme.green } else { theme.muted })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            if !disabled {
                                this.run(
                                    document,
                                    EditorCommand::PreviewKeyform {
                                        binding: reference.clone(),
                                        point: i,
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
                    .into_any_element(),
            );
        }
        let x = fraction(axes[0].value, &axes[0]);
        let y = if axes.len() == 2 {
            1. - fraction(axes[1].value, &axes[1])
        } else {
            0.5
        };
        let reference = b.reference.clone();
        let measured = reference.clone();
        let panel = cx.entity();
        let pad = div()
            .id("binding-keyform-pad")
            .test_support()
            .relative()
            .mx_2()
            .my_1()
            .flex_1()
            .min_h(px(38.))
            .bg(theme.muted)
            .border_1()
            .border_color(theme.border)
            .on_prepaint(move |rect, _, cx| {
                panel.update(cx, |this, _| {
                    this.bounds = rect;
                    this.bounds_binding = Some((document, measured.clone()));
                });
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if disabled {
                        return;
                    }
                    this.dragging = Some((document, reference.clone()));
                    this.move_preview(event.position, window, cx);
                }),
            )
            .children(grid)
            .child(
                div()
                    .id("keyform-preview-coordinate")
                    .test_support()
                    .absolute()
                    .left(relative(x))
                    .top(relative(y))
                    .ml(-px(7.))
                    .mt(-px(7.))
                    .size(px(14.))
                    .rounded_full()
                    .border_2()
                    .border_color(if stored { theme.green } else { theme.red }),
            );
        body = body.child(pad);
        body.into_any_element()
    }
    fn move_preview(&mut self, point: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((document, reference)) = self.dragging.clone() else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            return;
        };
        if self.bounds_binding.as_ref() != Some(&(document, reference.clone()))
            || state.editor.active_id() != Some(document)
            || doc.context().active_binding.as_ref() != Some(&reference)
            || self.bounds.size.width <= px(0.)
            || self.bounds.size.height <= px(0.)
        {
            self.dragging = None;
            return;
        }
        let index = doc.object_index();
        let Some(b) = index.binding(&reference) else {
            return;
        };
        let parameters = doc.parameters();
        let mut values = Vec::new();
        for (i, a) in b.axes.iter().enumerate().take(2) {
            let Some(p) = parameters.iter().find(|p| p.id == a.parameter_id) else {
                return;
            };
            let f = if i == 0 {
                (point.x - self.bounds.left()).as_f32() / self.bounds.size.width.as_f32()
            } else {
                (self.bounds.bottom() - point.y).as_f32() / self.bounds.size.height.as_f32()
            };
            values.push((
                a.parameter_id.clone(),
                p.minimum + (p.maximum - p.minimum) * f.clamp(0., 1.),
            ));
        }
        self.run(
            document,
            EditorCommand::SetPreviewParameters { values },
            window,
            cx,
        );
    }
}
impl EventEmitter<PanelEvent> for DocumentPanel {}
impl Focusable for DocumentPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for DocumentPanel {
    fn panel_name(&self) -> &'static str {
        self.kind.name()
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for DocumentPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.kind.title()
    }
}
impl Render for DocumentPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.row_sequence = 0;
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            return div()
                .p_3()
                .child(t!("editor.no_document"))
                .into_any_element();
        };
        let Some(document) = state.editor.active_id() else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let context = doc.context().clone();
        let parameters = doc.parameters();
        let disabled = state.modal || state.editor.is_busy();
        let mut body = div()
            .id(SharedString::from(format!("{}-body", self.kind.name())))
            .flex()
            .flex_col()
            .w_full();
        if self.kind == DocumentPanelKind::Keyforms {
            body = body
                .h_full()
                .child(self.keyforms(index, context, parameters, document, disabled, cx));
        } else if self.kind == DocumentPanelKind::Relations {
            body = body.child(
                div()
                    .id("relation-back")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .child(t!("editor.navigate_back"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run(document, EditorCommand::NavigateBack, window, cx)
                    })),
            );
            if let Some(r) = context.inspected.as_ref() {
                body = body.child(
                    div()
                        .p_2()
                        .child(format!("{} · {:?}", index.name(r), r.kind)),
                );
                for incoming in [false, true] {
                    let relations: Vec<_> = if incoming {
                        index.incoming(r).collect()
                    } else {
                        index.outgoing(r).collect()
                    };
                    for e in relations {
                        let target = if incoming { &e.source } else { &e.target };
                        body = body.child(self.row(
                            &index,
                            target,
                            format!("{} {}", e.kind.label(incoming), e.detail),
                            document,
                            false,
                            cx,
                        ));
                    }
                }
                let paths = index.influences_for(r);
                body = body.child(
                    div()
                        .id("relations-indirect-toggle")
                        .p_2()
                        .cursor_pointer()
                        .child(format!(
                            "{} · {}",
                            t!("editor.indirect_influences"),
                            paths.len()
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.indirect = !this.indirect;
                            cx.notify();
                        })),
                );
                if self.indirect {
                    for path in paths {
                        for (i, target) in path.path.iter().enumerate() {
                            body = body.child(self.row(
                                &index,
                                target,
                                format!("{} →", i + 1),
                                document,
                                false,
                                cx,
                            ));
                        }
                    }
                }
            } else {
                body = body.child(t!("editor.choose_object"));
            }
        } else if self.kind == DocumentPanelKind::DrawOrder {
            body = body.child(div().p_2().child(t!("editor.draw_order_direction")));
            // The scene cache is evaluated by core/preview, independent of panel visibility.
            if let Some(frame) = state.editor.evaluated_frame() {
                use kasane_core::evaluation::RenderCommand;
                let orders: std::collections::HashMap<_, _> = frame
                    .drawables
                    .iter()
                    .map(|d| (&d.id, d.draw_order))
                    .collect();
                for (i, command) in frame.render_plan.iter().enumerate().rev() {
                    let (r, detail) = match command {
                        RenderCommand::DrawMesh { mesh_id } => (
                            ObjectRef::new(ObjectKind::Mesh, mesh_id),
                            format!(
                                "#{i} · {} · draw order {}",
                                index
                                    .draw_group(mesh_id)
                                    .map(|r| index.name(&r).to_owned())
                                    .unwrap_or_default(),
                                orders.get(mesh_id).copied().unwrap_or(0)
                            ),
                        ),
                        RenderCommand::BeginOffscreen { offscreen_id } => (
                            ObjectRef::new(ObjectKind::Offscreen, offscreen_id),
                            format!("#{i} · begin composition"),
                        ),
                        RenderCommand::EndOffscreen { offscreen_id } => (
                            ObjectRef::new(ObjectKind::Offscreen, offscreen_id),
                            format!("#{i} · composite"),
                        ),
                    };
                    body = body.child(self.row(
                        &index,
                        &r,
                        detail,
                        document,
                        context.selection.as_ref() == Some(&r),
                        cx,
                    ));
                }
            }
            for x in &index.objects {
                if matches!(x.reference.kind, ObjectKind::Mesh | ObjectKind::Offscreen)
                    && x.enabled == Some(false)
                {
                    body = body.child(self.row(
                        &index,
                        &x.reference,
                        t!("editor.object_disabled").into_owned(),
                        document,
                        context.selection.as_ref() == Some(&x.reference),
                        cx,
                    ));
                }
            }
        } else {
            let references: Vec<_> = index
                .objects
                .iter()
                .filter(|x| self.kind.includes(x.reference.kind))
                .map(|x| x.reference.clone())
                .collect();
            let allowed: std::collections::HashSet<_> = references.iter().cloned().collect();
            fn visit(
                r: &ObjectRef,
                index: &ObjectIndex,
                allowed: &std::collections::HashSet<ObjectRef>,
                path: &mut Vec<ObjectRef>,
                out: &mut Vec<(ObjectRef, usize)>,
            ) {
                if path.contains(r) {
                    return;
                }
                out.push((r.clone(), path.len()));
                path.push(r.clone());
                for e in index
                    .outgoing(r)
                    .filter(|e| e.kind == RelationKind::Contains && allowed.contains(&e.target))
                {
                    visit(&e.target, index, allowed, path, out);
                }
                path.pop();
            }
            let mut rows = Vec::new();
            for r in &references {
                if !index
                    .incoming(r)
                    .any(|e| e.kind == RelationKind::Contains && allowed.contains(&e.source))
                {
                    visit(r, &index, &allowed, &mut Vec::new(), &mut rows);
                }
            }
            for r in &references {
                if !rows.iter().any(|(row, _)| row == r) {
                    visit(r, &index, &allowed, &mut Vec::new(), &mut rows);
                }
            }
            for (r, depth) in rows {
                let uses = index
                    .incoming(&r)
                    .filter(|e| e.kind != RelationKind::Contains)
                    .count();
                let detail = if matches!(
                    r.kind,
                    ObjectKind::Texture
                        | ObjectKind::Attachment
                        | ObjectKind::BlendKeyTable
                        | ObjectKind::BlendConstraint
                ) {
                    if uses == 0 {
                        t!("editor.unused_object").into_owned()
                    } else {
                        format!("{}: {}", t!("editor.used_by"), uses)
                    }
                } else {
                    index
                        .outgoing(&r)
                        .filter(|e| e.kind != RelationKind::Contains)
                        .map(|e| {
                            format!(
                                "{} {} {}",
                                e.kind.label(false),
                                index.name(&e.target),
                                e.detail
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
                let row = self.row(
                    &index,
                    &r,
                    detail,
                    document,
                    context.inspected.as_ref() == Some(&r),
                    cx,
                );
                body = body.child(div().pl(px(depth as f32 * 12.)).child(row));
            }
            if references.is_empty() {
                body = body.child(div().p_3().child(t!("editor.no_items")));
            }
        }
        let panel = cx.entity();
        div()
            .id(SharedString::from(format!("{}-panel", self.kind.name())))
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs()
            .track_focus(&self.focus)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let moving = panel.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                moving.update(cx, |p, cx| {
                                    if event.pressed_button == Some(MouseButton::Left) {
                                        p.move_preview(event.position, window, cx);
                                    } else {
                                        p.dragging = None;
                                    }
                                });
                            }
                        });
                        let releasing = panel.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                            if event.button == MouseButton::Left {
                                releasing.update(cx, |p, _| p.dragging = None);
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(body)
            .into_any_element()
    }
}

/// Readable scalar/nested configuration rows for the shared property inspector.
pub fn property_rows(value: &serde_json::Value) -> Vec<(String, String)> {
    fn walk(value: &serde_json::Value, path: String, rows: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (name, value) in fields {
                    walk(
                        value,
                        if path.is_empty() {
                            name.clone()
                        } else {
                            format!("{path}.{name}")
                        },
                        rows,
                    );
                }
            }
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    rows.push((path, "[]".into()));
                } else {
                    for (i, item) in items.iter().enumerate() {
                        walk(item, format!("{path}[{i}]"), rows);
                    }
                }
            }
            serde_json::Value::String(text) => rows.push((path, text.clone())),
            _ => rows.push((path, value.to_string())),
        }
    }
    let mut rows = Vec::new();
    walk(value, String::new(), &mut rows);
    rows
}
