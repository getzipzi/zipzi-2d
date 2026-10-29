//! Independent transient feedback, task progress and retained operation problems.
use crate::{EditorView, t, workspace::status_bar::StatusBar};
use gpui_kit::{Context, Task};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zipzi_editor_core::{DocumentId, LoadProgress};

mod problems;
pub(crate) use problems::{ProblemOrigin, ProblemReport, ProblemSeverity, Problems};

pub(crate) struct Feedback {
    id: Uuid,
    _expiration: Option<Task<()>>,
}

impl EditorView {
    pub(crate) fn set_edit_error(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        self.set_operation_error(self.editor.active_id(), error, cx);
    }

    pub(crate) fn set_operation_error(
        &mut self,
        document: Option<DocumentId>,
        error: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let error = error.into();
        let title = self.problem_title(document, &t!("status.operation_failed"));
        let path = self
            .editor
            .documents()
            .into_iter()
            .find(|d| Some(d.id) == document)
            .and_then(|d| d.path);
        self.status_bar
            .read(cx)
            .problems
            .clone()
            .update(cx, |problems, cx| {
                problems.report(
                    ProblemReport::new(
                        ProblemOrigin::Operation(document),
                        ProblemSeverity::Error,
                        title,
                        path,
                        vec![error.clone()],
                    ),
                    cx,
                );
            });
        self.show_feedback(error, true, cx);
    }

    pub(crate) fn set_status_message(
        &mut self,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.show_feedback(message.into(), false, cx);
    }

    fn show_feedback(&mut self, text: String, error: bool, cx: &mut Context<Self>) {
        self.message = if error { String::new() } else { text.clone() };
        self.edit_error = error.then_some(text);
        self.feedback = Some(Feedback {
            id: Uuid::new_v4(),
            _expiration: None,
        });
        self.resume_feedback(cx);
        StatusBar::sync(&self.status_bar, self, cx);
        cx.notify();
    }

    fn pause_feedback(&mut self) {
        if let Some(feedback) = &mut self.feedback {
            feedback.id = Uuid::new_v4();
            feedback._expiration = None;
        }
    }

    fn resume_feedback(&mut self, cx: &mut Context<Self>) {
        if self.status_bar.read(cx).has_tasks() {
            return;
        }
        let Some(feedback) = &mut self.feedback else {
            return;
        };
        if feedback._expiration.is_some() {
            return;
        }
        let id = Uuid::new_v4();
        feedback.id = id;
        let timeout = cx.background_executor().timer(Self::STATUS_MESSAGE_TIMEOUT);
        feedback._expiration = Some(cx.spawn(async move |this, cx| {
            timeout.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .feedback
                    .as_ref()
                    .is_some_and(|feedback| feedback.id == id)
                {
                    this.clear_feedback(cx);
                }
            });
        }));
    }

    fn clear_feedback(&mut self, cx: &mut Context<Self>) {
        self.feedback = None;
        self.message.clear();
        self.edit_error = None;
        StatusBar::sync(&self.status_bar, self, cx);
        cx.notify();
    }

    pub(crate) fn clear_edit_error(&mut self, cx: &mut Context<Self>) {
        if self.edit_error.is_some() {
            self.clear_feedback(cx);
        }
    }

    pub(crate) fn clear_status_message(&mut self, message: &str, cx: &mut Context<Self>) {
        if self.message == message {
            self.clear_feedback(cx);
        }
    }

    pub(crate) fn replace_status_message(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        if self.message == old {
            self.message = new.to_owned();
            StatusBar::sync(&self.status_bar, self, cx);
            cx.notify();
        }
    }

    pub(crate) fn begin_load_status(
        &mut self,
        source: PathBuf,
        importing: bool,
        progress: LoadProgress,
        cx: &mut Context<Self>,
    ) -> Uuid {
        let id = self.status_bar.update(cx, |bar, cx| {
            bar.begin_load(source, importing, progress, cx)
        });
        self.pause_feedback();
        id
    }

    pub(crate) fn finish_load_status(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.status_bar
            .update(cx, |bar, cx| bar.finish_task(id, cx));
        self.resume_feedback(cx);
    }

    pub(crate) fn begin_operation_status(
        &mut self,
        label: String,
        source: PathBuf,
        cx: &mut Context<Self>,
    ) -> Uuid {
        let id = self
            .status_bar
            .update(cx, |bar, cx| bar.begin_task(label, source, cx));
        self.pause_feedback();
        id
    }

    pub(crate) fn record_load_warnings(
        &mut self,
        source: &Path,
        importing: bool,
        warnings: Vec<String>,
        cx: &mut Context<Self>,
    ) -> usize {
        let title = if importing {
            t!("status.import_warnings")
        } else {
            t!("status.open_warnings")
        };
        let report = ProblemReport::new(
            ProblemOrigin::Load {
                document: self.editor.active_id(),
                source: source.to_owned(),
            },
            ProblemSeverity::Warning,
            title.into_owned(),
            Some(source.to_owned()),
            warnings,
        );
        let count = report.messages.len();
        self.status_bar
            .read(cx)
            .problems
            .clone()
            .update(cx, |problems, cx| {
                problems.report(report, cx);
            });
        count
    }

    pub(crate) fn set_load_error(&mut self, source: &Path, error: String, cx: &mut Context<Self>) {
        let report = ProblemReport::new(
            ProblemOrigin::Load {
                document: None,
                source: source.to_owned(),
            },
            ProblemSeverity::Error,
            t!("status.load_failed").into_owned(),
            Some(source.to_owned()),
            vec![error.clone()],
        );
        self.status_bar
            .read(cx)
            .problems
            .clone()
            .update(cx, |problems, cx| {
                problems.report(report, cx);
            });
        self.show_feedback(error, true, cx);
    }

    pub(crate) fn record_operation_warnings(
        &mut self,
        document: Option<DocumentId>,
        title: &str,
        path: Option<PathBuf>,
        messages: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let title = self.problem_title(document, title);
        self.status_bar
            .read(cx)
            .problems
            .clone()
            .update(cx, |problems, cx| {
                problems.report(
                    ProblemReport::new(
                        ProblemOrigin::Operation(document),
                        ProblemSeverity::Warning,
                        title,
                        path,
                        messages,
                    ),
                    cx,
                );
            });
    }

    fn problem_title(&self, document: Option<DocumentId>, title: &str) -> String {
        self.editor
            .documents()
            .into_iter()
            .find(|d| Some(d.id) == document)
            .map(|d| {
                format!(
                    "{} · {title}",
                    d.title
                        .unwrap_or_else(|| t!("project.untitled").into_owned())
                )
            })
            .unwrap_or_else(|| title.to_owned())
    }
}
