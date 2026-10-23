//! Authoring identities and presented Pose values share one navigable list.
use super::*;
use crate::{
    authoring::{AuthoringContext, business},
    panels::toolbar::PanelToolbar,
    ui::compact,
};
use editor_core::{BusinessAction as A, ObjectKind as K, ObjectRef as R};
use gpui_kit::{assets::IconName, component::menu::ContextMenuExt};
#[derive(Clone)]
struct Row {
    object: R,
    parent: Option<R>,
    title: String,
    depth: usize,
    children: bool,
    value: Option<(String, String)>,
}
impl AnimationPanel {
    pub(crate) fn business_movement_object(&self) -> Option<R> {
        self.business_focus.clone()
    }
    pub(super) fn pose_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut toolbar = PanelToolbar::new("pose-toolbar");
        let Some(owner) = self.owner.upgrade() else {
            return toolbar.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let view = owner.read(cx);
        let context = AuthoringContext::capture(&view.editor);
        let selected = view.editor.active_inspection().and_then(|i| i.object());
        for kind in [K::PoseGroup, K::PoseEntry, K::PoseLink] {
            let action = A::Create(kind, selected.clone());
            let availability = business::availability(&view.editor, &action);
            let disabled = view.modal || dialog || availability.as_ref().is_none_or(|a| !a.enabled);
            let hint = availability
                .and_then(|a| a.reason)
                .unwrap_or_else(|| business::create_label(kind));
            let owner = self.owner.clone();
            toolbar = toolbar.action(move |_| {
                Button::new(match kind {
                    K::PoseGroup => "pose-new-group",
                    K::PoseEntry => "pose-new-entry",
                    _ => "pose-new-link",
                })
                .icon(if kind == K::PoseGroup {
                    IconName::FolderPlus
                } else {
                    IconName::Plus
                })
                .ghost()
                .accessibility_label(business::create_label(kind))
                .tooltip(hint.clone())
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = owner.update(cx, |v, cx| {
                            v.run_business_action(context, action.clone(), window, cx)
                        });
                    }
                })
            });
        }
        toolbar.into_any_element()
    }
    pub(super) fn render_pose_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let view = owner.read(cx);
        let Some(document) = view.editor.active_id() else {
            return Empty.into_any_element();
        };
        let Some(doc) = view.editor.document() else {
            return Empty.into_any_element();
        };
        let Some(pose) = doc.pose_definition() else {
            return div()
                .id("animation-pose-empty")
                .test_support()
                .p_2()
                .child(t!("workspace.no_pose"))
                .into_any_element();
        };
        let selected = view.editor.active_inspection().and_then(|i| i.object());
        let context = AuthoringContext::capture(&view.editor);
        let disabled = view.modal || dialog || view.editor.is_busy();
        let mut rows = vec![Row {
            object: R::new(K::Pose, &pose.id),
            parent: None,
            title: t!("properties.type_Pose").into_owned(),
            depth: 0,
            children: false,
            value: None,
        }];
        let snapshot = view.displayed_animation();
        let value = |reference: &kasane_core::document::PosePartRef| match reference {
            kasane_core::document::PosePartRef::Resolved { part_id } => snapshot
                .and_then(|s| s.part_opacities.get(part_id))
                .map_or_else(|| "—".into(), |v| format!("{v:.4}")),
            _ => "—".into(),
        };
        let reveal = self
            .location
            .as_ref()
            .is_some_and(|(id, _)| *id == document);
        if let Some((_, object)) = self.location.take().filter(|(id, _)| *id == document) {
            for group in &pose.groups {
                if group.id == object.id
                    || group.entries.iter().any(|e| {
                        e.id == object.id || e.links.iter().flatten().any(|l| l.id == object.id)
                    })
                {
                    self.business_collapsed.remove(&group.id);
                    for entry in &group.entries {
                        if entry.id == object.id
                            || entry.links.iter().flatten().any(|l| l.id == object.id)
                        {
                            self.business_collapsed.remove(&entry.id);
                        }
                    }
                }
            }
            self.business_focus = Some(object);
        }
        for (i, group) in pose.groups.iter().enumerate() {
            let group_ref = R::new(K::PoseGroup, &group.id);
            rows.push(Row {
                object: group_ref.clone(),
                parent: Some(rows[0].object.clone()),
                title: if group.name.is_empty() {
                    t!("workspace.pose_group", index = i + 1).into_owned()
                } else {
                    group.name.clone()
                },
                depth: 0,
                children: true,
                value: None,
            });
            if self.business_collapsed.contains(&group.id) {
                continue;
            }
            for entry in &group.entries {
                let object = R::new(K::PoseEntry, &entry.id);
                rows.push(Row {
                    object: object.clone(),
                    parent: Some(group_ref.clone()),
                    title: crate::properties::part_name(doc, &entry.part),
                    depth: 1,
                    children: entry.links.as_ref().is_some_and(|ls| !ls.is_empty()),
                    value: Some((entry.id.clone(), value(&entry.part))),
                });
                if self.business_collapsed.contains(&entry.id) {
                    continue;
                }
                for link in entry.links.iter().flatten() {
                    rows.push(Row {
                        object: R::new(K::PoseLink, &link.id),
                        parent: Some(object.clone()),
                        title: t!(
                            "workspace.pose_link",
                            part = crate::properties::part_name(doc, &link.target)
                        )
                        .into_owned(),
                        depth: 2,
                        children: false,
                        value: Some((link.id.clone(), value(&link.target))),
                    });
                }
            }
        }
        let focused = self
            .business_focus
            .as_ref()
            .or(selected.as_ref())
            .and_then(|o| rows.iter().position(|r| &r.object == o));
        if let Some(i) = focused.filter(|_| reveal) {
            self.business_scroll.scroll_to_item(i, ScrollStrategy::Top);
        }
        let choices = rows.clone();
        let owner = self.owner.clone();
        let focus = self.focus.clone();
        let collapsed = self.business_collapsed.clone();
        div()
            .id("AnimationPose")
            .test_support()
            .size_full()
            .track_focus(&self.focus)
            .key_context("KasaneDocument KasanePalette")
            .flex()
            .flex_col()
            .text_size(compact::rem(compact::BODY))
            .child(
                div()
                    .id("animation-pose-fade")
                    .test_support()
                    .aria_label(pose.fade_in.map_or_else(
                        || t!("workspace.resource_default").into_owned(),
                        |f| format!("{f:.3} s"),
                    ))
                    .px_2()
                    .child(format!(
                        "{} · {}",
                        t!("workspace.pose_fade"),
                        pose.fade_in.map_or_else(
                            || t!("workspace.resource_default").into_owned(),
                            |f| format!("{f:.3} s")
                        )
                    )),
            )
            .child(
                uniform_list(
                    "pose-rows",
                    rows.len(),
                    cx.processor(move |panel, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let row = &rows[i];
                                let object = row.object.clone();
                                let row_owner = owner.clone();
                                let row_focus = focus.clone();
                                let fold = object.id.clone();
                                div()
                                    .id(SharedString::from(format!("pose-row-{}", object.id)))
                                    .test_support()
                                    .role(Role::TreeItem)
                                    .aria_label(row.title.clone())
                                    .aria_selected(selected.as_ref() == Some(&object))
                                    .h(compact::rem(compact::ROW))
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .pl(compact::rem(row.depth as f32 * 12. + 4.))
                                    .when(selected.as_ref() == Some(&object), |r| {
                                        r.bg(cx.theme().secondary)
                                    })
                                    .when(!row.children, |r| {
                                        r.child(div().w(compact::rem(22.)).flex_shrink_0())
                                    })
                                    .when(row.children, |r| {
                                        r.child(
                                            Button::new(SharedString::from(format!(
                                                "pose-fold-{}",
                                                object.id
                                            )))
                                            .ghost()
                                            .icon(if collapsed.contains(&object.id) {
                                                IconName::ChevronRight
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .disabled(disabled)
                                            .on_click(cx.listener(move |panel, _, _, cx| {
                                                if !panel.business_collapsed.remove(&fold) {
                                                    panel.business_collapsed.insert(fold.clone());
                                                }
                                                cx.notify();
                                            })),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_ellipsis()
                                            .overflow_hidden()
                                            .child(row.title.clone()),
                                    )
                                    .when_some(row.value.clone(), |r, (id, value)| {
                                        r.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "animation-pose-part-{id}"
                                                )))
                                                .test_support()
                                                .aria_label(value.clone())
                                                .pr_2()
                                                .child(value),
                                        )
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |panel, _, window, cx| {
                                            if !disabled {
                                                panel.business_focus = Some(object.clone());
                                                if let Some(context) = context {
                                                    let _ = row_owner.update(cx, |v, cx| {
                                                        if AuthoringContext::capture(&v.editor)
                                                            == Some(context)
                                                            && v.prepare_property_inputs(
                                                                document, window, cx,
                                                            )
                                                        {
                                                            window.focus(&row_focus, cx);
                                                            let result = v
                                                                .inspect_authoring_object(
                                                                    document,
                                                                    object.clone(),
                                                                    false,
                                                                    window,
                                                                    cx,
                                                                );
                                                            v.complete_edit(result, cx);
                                                        }
                                                    });
                                                }
                                            }
                                        }),
                                    )
                                    .context_menu(business::menu(
                                        panel.owner.clone(),
                                        row.object.clone(),
                                    ))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.business_scroll)
                .flex_1()
                .min_h_0(),
            )
            .on_key_down(cx.listener(move |panel, event: &KeyDownEvent, window, cx| {
                if disabled || !panel.focus.is_focused(window) || choices.is_empty() {
                    return;
                }
                let at = panel
                    .business_focus
                    .as_ref()
                    .and_then(|o| choices.iter().position(|r| &r.object == o))
                    .or(focused)
                    .unwrap_or(0);
                let next = match event.keystroke.key.as_str() {
                    "down" => (at + 1).min(choices.len() - 1),
                    "up" => at.saturating_sub(1),
                    "enter" => at,
                    "left" => {
                        let r = &choices[at];
                        if r.children && !panel.business_collapsed.contains(&r.object.id) {
                            panel.business_collapsed.insert(r.object.id.clone());
                            at
                        } else {
                            r.parent
                                .as_ref()
                                .and_then(|p| choices.iter().position(|r| &r.object == p))
                                .unwrap_or(at)
                        }
                    }
                    "right" => {
                        panel.business_collapsed.remove(&choices[at].object.id);
                        at
                    }
                    _ => return,
                };
                panel
                    .business_scroll
                    .scroll_to_item(next, ScrollStrategy::Nearest);
                let object = choices[next].object.clone();
                panel.business_focus = Some(object.clone());
                let owner = panel.owner.clone();
                if let Some(context) = context {
                    let _ = owner.update(cx, |v, cx| {
                        if AuthoringContext::capture(&v.editor) == Some(context)
                            && v.prepare_property_inputs(document, window, cx)
                        {
                            let result =
                                v.inspect_authoring_object(document, object, false, window, cx);
                            v.complete_edit(result, cx);
                        }
                    });
                }
                cx.notify();
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}
