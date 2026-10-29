//! Retained, source-attributed problems. Reading or displaying feedback never clears them.
use crate::{t, ui::compact};
use gpui_kit::{
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        popover::Popover,
    },
    prelude::*,
    *,
};
use std::{collections::HashSet, path::PathBuf};
use uuid::Uuid;
use zipzi_editor_core::DocumentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProblemSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProblemOrigin {
    Operation(Option<DocumentId>),
    Load {
        document: Option<DocumentId>,
        source: PathBuf,
    },
    Rendering(Option<DocumentId>),
}

#[derive(Clone, Debug)]
pub(crate) struct ProblemReport {
    pub id: Uuid,
    pub origin: ProblemOrigin,
    pub severity: ProblemSeverity,
    pub title: String,
    pub path: Option<PathBuf>,
    pub messages: Vec<String>,
}

impl ProblemReport {
    pub fn new(
        origin: ProblemOrigin,
        severity: ProblemSeverity,
        title: String,
        path: Option<PathBuf>,
        mut messages: Vec<String>,
    ) -> Self {
        let mut seen = HashSet::new();
        messages.retain(|message| !message.trim().is_empty() && seen.insert(message.clone()));
        Self {
            id: Uuid::new_v4(),
            origin,
            severity,
            title,
            path,
            messages,
        }
    }
}

#[derive(Default)]
pub(crate) struct Problems {
    reports: Vec<ProblemReport>,
    list: Option<Entity<ProblemList>>,
    popup_open: bool,
    // A dismissed renderer fault stays acknowledged until it changes or resolves.
    renderer_faults: Vec<(Option<DocumentId>, String)>,
}

impl Problems {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self::default())
    }

    pub fn report(&mut self, report: ProblemReport, cx: &mut Context<Self>) {
        if report.messages.is_empty()
            || self.reports.iter().any(|old| {
                old.origin == report.origin
                    && old.severity == report.severity
                    && old.title == report.title
                    && old.path == report.path
                    && old.messages == report.messages
            })
        {
            return;
        }
        self.reports.push(report);
        cx.notify();
    }

    pub fn sync_renderer(
        &mut self,
        document: Option<DocumentId>,
        fault: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let previous = self
            .renderer_faults
            .iter()
            .position(|(id, _)| *id == document);
        if previous.and_then(|index| self.renderer_faults.get(index).map(|(_, text)| text))
            == fault.as_ref()
        {
            return;
        }
        if let Some(index) = previous {
            self.renderer_faults.remove(index);
        }
        self.reports
            .retain(|report| report.origin != ProblemOrigin::Rendering(document));
        if let Some(fault) = fault {
            self.renderer_faults.push((document, fault.clone()));
            self.reports.push(ProblemReport::new(
                ProblemOrigin::Rendering(document),
                ProblemSeverity::Error,
                t!("status.render_failed").into_owned(),
                None,
                vec![fault],
            ));
        }
        cx.notify();
    }

    pub fn dismiss(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.reports.retain(|report| report.id != id);
        if self.reports.is_empty() {
            self.popup_open = false;
        }
        cx.notify();
    }

    pub fn dismiss_all(&mut self, cx: &mut Context<Self>) {
        self.reports.clear();
        self.popup_open = false;
        cx.notify();
    }

    pub fn counts(&self) -> (usize, usize) {
        self.reports
            .iter()
            .fold((0, 0), |(errors, warnings), report| match report.severity {
                ProblemSeverity::Error => (errors + report.messages.len(), warnings),
                ProblemSeverity::Warning => (errors, warnings + report.messages.len()),
            })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn reports(&self) -> &[ProblemReport] {
        &self.reports
    }
}

impl Render for Problems {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (errors, warnings) = self.counts();
        if errors + warnings == 0 {
            return div().into_any_element();
        }
        let summary = t!(
            "status.problem_counts",
            errors = errors,
            warnings = warnings
        )
        .into_owned();
        let source = cx.weak_entity();
        let list = self
            .list
            .get_or_insert_with(|| cx.new(|cx| ProblemList::new(source.clone(), cx)))
            .clone();
        Popover::new("status-problems-popover")
            .anchor(Anchor::BottomRight)
            .open(self.popup_open)
            .on_open_change(move |open, _, cx| {
                let _ = source.update(cx, |problems, cx| {
                    problems.popup_open = *open;
                    cx.notify();
                });
            })
            .p_0()
            .trigger(
                compact::button(Button::new("app-status-problems"))
                    .ghost()
                    .h(compact::rem(16.5))
                    .px(compact::rem(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(compact::rem(6.))
                            .when(errors > 0, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(compact::rem(3.))
                                        .text_color(cx.theme().danger)
                                        .child(Icon::new(IconName::CircleAlert).xsmall())
                                        .child(errors.to_string()),
                                )
                            })
                            .when(warnings > 0, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(compact::rem(3.))
                                        .text_color(cx.theme().warning)
                                        .child(Icon::new(IconName::TriangleAlert).xsmall())
                                        .child(warnings.to_string()),
                                )
                            }),
                    )
                    .accessibility_label(summary.clone())
                    .tooltip(summary),
            )
            .content(move |_, _, _| list.clone())
            .into_any_element()
    }
}

struct ProblemList {
    source: WeakEntity<Problems>,
    _observe: Subscription,
}

impl ProblemList {
    fn new(source: WeakEntity<Problems>, cx: &mut Context<Self>) -> Self {
        Self {
            _observe: cx.observe(&source.upgrade().unwrap(), |_, _, cx| cx.notify()),
            source,
        }
    }
}

impl Render for ProblemList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(source) = self.source.upgrade() else {
            return div().into_any_element();
        };
        let reports = source.read(cx).reports.clone();
        let clear = self.source.clone();
        let width = compact::scaled(440., cx).min(window.viewport_size().width - px(24.));
        let height = compact::scaled(420., cx).min(window.viewport_size().height - px(100.));
        div()
            .id("status-problems-content")
            .test_support()
            .w(width)
            .max_h(height)
            .min_w_0()
            .flex()
            .flex_col()
            .text_size(compact::rem(compact::BODY))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .p(compact::rem(8.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t!("status.problems").into_owned()),
                    )
                    .child(
                        compact::button(Button::new("status-problems-clear"))
                            .ghost()
                            .label(t!("status.dismiss_all"))
                            .on_click(move |_, _, cx| {
                                let _ = clear.update(cx, |problems, cx| problems.dismiss_all(cx));
                            }),
                    ),
            )
            .child(
                div()
                    .id("status-problems-list")
                    .test_support()
                    .max_h((height - compact::scaled(42., cx)).max(px(40.)))
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(compact::rem(8.))
                    .children(
                        [ProblemSeverity::Error, ProblemSeverity::Warning]
                            .into_iter()
                            .flat_map(|severity| {
                                reports
                                    .iter()
                                    .rev()
                                    .filter(move |report| report.severity == severity)
                            })
                            .map(|report| render_report(report, &source, cx)),
                    ),
            )
            .into_any_element()
    }
}

fn render_report(report: &ProblemReport, source: &Entity<Problems>, cx: &App) -> impl IntoElement {
    let dismiss = source.downgrade();
    let id = report.id;
    let clipboard = format!(
        "{}\n{}\n{}",
        report.title,
        report
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        report.messages.join("\n")
    );
    let color = if report.severity == ProblemSeverity::Error {
        cx.theme().danger
    } else {
        cx.theme().warning
    };
    div()
        .id(SharedString::from(format!("status-problem-{id}")))
        .test_support()
        .min_w_0()
        .mb(compact::rem(8.))
        .flex()
        .flex_col()
        .gap(compact::rem(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(compact::rem(4.))
                .child(
                    Icon::new(if report.severity == ProblemSeverity::Error {
                        IconName::CircleAlert
                    } else {
                        IconName::TriangleAlert
                    })
                    .xsmall()
                    .text_color(color),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_weight(FontWeight::MEDIUM)
                        .child(report.title.clone()),
                )
                .child(
                    compact::button(Button::new(SharedString::from(format!(
                        "status-problem-copy-{id}"
                    ))))
                    .ghost()
                    .icon(IconName::Copy)
                    .tooltip(t!("status.copy_details"))
                    .accessibility_label(t!("status.copy_details"))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(clipboard.clone()));
                    }),
                )
                .child(
                    compact::button(Button::new(SharedString::from(format!(
                        "status-problem-dismiss-{id}"
                    ))))
                    .ghost()
                    .icon(IconName::Close)
                    .tooltip(t!("status.dismiss"))
                    .accessibility_label(t!("status.dismiss"))
                    .on_click(move |_, _, cx| {
                        let _ = dismiss.update(cx, |problems, cx| problems.dismiss(id, cx));
                    }),
                ),
        )
        .when_some(report.path.clone(), |row, path| {
            row.child(
                div()
                    .min_w_0()
                    .text_color(cx.theme().muted_foreground)
                    .child(path.display().to_string()),
            )
        })
        .children(report.messages.iter().enumerate().map(|(index, message)| {
            div()
                .id(SharedString::from(format!(
                    "status-problem-message-{id}-{index}"
                )))
                .test_support()
                .min_w_0()
                .whitespace_normal()
                .child(message.clone())
        }))
}
