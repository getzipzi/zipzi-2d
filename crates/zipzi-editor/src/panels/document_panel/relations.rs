//! Compact relation rows and the shared inspector identity header.
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Icon, Sizable,
    button::{Button, ButtonVariants},
    tooltip::Tooltip,
};
use zipzi_editor_core::Document;

impl DocumentPanel {
    pub(super) fn relations(
        &mut self,
        doc: &Document,
        document: DocumentId,
        cx: &Context<Self>,
    ) -> AnyElement {
        let root = div()
            .id("Relations-panel")
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(compact::rem(compact::BODY))
            .track_focus(&self.focus);
        let inspected = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).editor.active_inspection())
            .and_then(|inspection| inspection.object());
        let Some(inspected) = inspected.as_ref() else {
            return root
                .child(
                    div()
                        .p(compact::rem(compact::PADDING))
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("editor.choose_object")),
                )
                .into_any_element();
        };
        if self.location.as_ref() == Some(&(document, inspected.clone())) {
            self.scroll.scroll_to_item(0);
            self.location = None;
        } else if self
            .location
            .as_ref()
            .is_some_and(|(id, _)| *id != document)
        {
            self.location = None;
        }
        let index = doc.object_index();
        let mut rows = div()
            .id("Relations-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        for incoming in [false, true] {
            let relations: Vec<_> = if incoming {
                index.incoming(inspected).collect()
            } else {
                index.outgoing(inspected).collect()
            };
            for relation in relations {
                let target = if incoming {
                    &relation.source
                } else {
                    &relation.target
                };
                rows = rows.child(self.relation_row(
                    doc,
                    target,
                    relation.kind.label(incoming).into(),
                    &relation.detail,
                    document,
                    cx,
                ));
            }
        }
        let paths = index.influences_for(inspected);
        let influences = format!("{} · {}", t!("editor.indirect_influences"), paths.len());
        rows = rows.child(
            Button::new("relations-indirect-toggle")
                .ghost()
                .xsmall()
                .h(compact::rem(compact::ROW))
                .w_full()
                .px(compact::rem(compact::PADDING))
                .justify_start()
                .accessibility_label(influences.clone())
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(compact::rem(compact::PADDING))
                        .child(
                            Icon::new(if self.indirect {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .xsmall(),
                        )
                        .child(div().min_w_0().truncate().child(influences)),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.indirect = !this.indirect;
                    cx.notify();
                })),
        );
        if self.indirect {
            for path in paths {
                for (i, target) in path.path.iter().enumerate() {
                    rows = rows.child(self.relation_row(
                        doc,
                        target,
                        format!("{} →", i + 1),
                        "",
                        document,
                        cx,
                    ));
                }
            }
        }
        root.child(crate::ui::object_identity::render(
            "relations",
            doc,
            inspected,
            cx,
        ))
        .child(compact::scroll_frame(rows, &self.scroll))
        .into_any_element()
    }

    fn relation_row(
        &mut self,
        doc: &Document,
        target: &ObjectRef,
        label: String,
        detail: &str,
        document: DocumentId,
        cx: &Context<Self>,
    ) -> AnyElement {
        let index = doc.object_index();
        let name = index.name(target).to_owned();
        let kind = (crate::properties::provider(target.kind).type_name)(doc, target);
        let info = index.get(target);
        let diagnostic = info.and_then(|info| info.diagnostic.as_ref());
        let disabled = info.is_some_and(|info| info.enabled == Some(false));
        let mut tooltip = format!("{label}: {name} · {kind}");
        for text in [
            Some(detail),
            disabled
                .then(|| t!("editor.object_disabled").into_owned())
                .as_deref(),
            diagnostic.map(String::as_str),
        ]
        .into_iter()
        .flatten()
        .filter(|text| !text.is_empty())
        {
            tooltip.push('\n');
            tooltip.push_str(text);
        }
        let target = target.clone();
        self.row_sequence += 1;
        div()
            .id(SharedString::from(format!(
                "relation-row-{}",
                self.row_sequence
            )))
            .test_support()
            .w_full()
            .h(compact::rem(compact::ROW))
            .min_w_0()
            .px(compact::rem(compact::PADDING))
            .flex()
            .items_center()
            .gap(compact::rem(compact::PADDING))
            .overflow_hidden()
            .whitespace_nowrap()
            .hover(|style| style.bg(cx.theme().muted))
            .cursor_pointer()
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(
                div()
                    .w(compact::rem(110.))
                    .flex_shrink_0()
                    .truncate()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(disabled, |name| {
                        name.text_color(cx.theme().muted_foreground)
                    })
                    .child(name),
            )
            .child(
                div()
                    .max_w(compact::rem(80.))
                    .flex_shrink_0()
                    .truncate()
                    .text_size(compact::rem(compact::CAPTION))
                    .text_color(cx.theme().muted_foreground)
                    .child(kind),
            )
            .when(diagnostic.is_some(), |row| {
                row.child(
                    Icon::new(IconName::TriangleAlert)
                        .xsmall()
                        .text_color(cx.theme().yellow),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.run(
                    document,
                    EditorCommand::Navigate(target.clone()),
                    window,
                    cx,
                )
            }))
            .into_any_element()
    }
}
