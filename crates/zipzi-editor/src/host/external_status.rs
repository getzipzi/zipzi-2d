//! Quiet host activity, independent of the Editor's timed interaction messages.
use crate::{t, ui::compact};
use gpui_kit::{component::ActiveTheme, prelude::*, *};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use zipzi_editor_core::RuntimeHost;
use zipzi_runtime::{EventCursor, EventKind, SessionId};

pub(crate) struct ExternalStatus {
    active_project: Option<SessionId>,
    project_titles: HashMap<SessionId, String>,
    applied_projects: Vec<AppliedProject>,
    clear_task: Option<Task<()>>,
    _publications: Task<()>,
}

struct AppliedProject {
    session: SessionId,
    path: Option<PathBuf>,
}

impl ExternalStatus {
    pub(crate) const NOTICE_DURATION: Duration = Duration::from_secs(3);

    pub(crate) fn new(runtime: RuntimeHost, cx: &mut App) -> Entity<Self> {
        let changes = runtime.subscribe_changes();
        // Start at the present boundary; reopening a view must not replay history.
        let mut cursors: HashMap<_, _> = {
            let runtime = runtime.read();
            runtime
                .list_sessions()
                .into_iter()
                .filter_map(|info| runtime.event_cursor(info.id).ok().map(|c| (info.id, c)))
                .collect()
        };
        cx.new(|cx: &mut Context<Self>| Self {
            active_project: None,
            project_titles: HashMap::new(),
            applied_projects: Vec::new(),
            clear_task: None,
            _publications: cx.spawn(async move |this, cx| {
                while changes.recv().await.is_ok() {
                    let projects = external_publications(&runtime, &mut cursors);
                    if !projects.is_empty()
                        && this
                            .update(cx, |this, cx| this.applied(projects, cx))
                            .is_err()
                    {
                        break;
                    }
                }
            }),
        })
    }

    pub(crate) fn set_project_context(
        &mut self,
        active: Option<SessionId>,
        titles: HashMap<SessionId, String>,
        cx: &mut Context<Self>,
    ) {
        if self.active_project != active || self.project_titles != titles {
            self.active_project = active;
            self.project_titles = titles;
            cx.notify();
        }
    }

    pub(crate) fn label(&self) -> String {
        if self.applied_projects.is_empty() {
            return String::new();
        }
        let others: Vec<_> = self
            .applied_projects
            .iter()
            .filter(|project| Some(project.session) != self.active_project)
            .collect();
        if others.is_empty() {
            return t!("status.external_applied").into_owned();
        }
        let project = if self.applied_projects.len() == 1 {
            let target = others[0];
            target
                .path
                .as_ref()
                .and_then(|path| path.parent())
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .or_else(|| self.project_titles.get(&target.session).cloned())
                .unwrap_or_else(|| {
                    t!(
                        "status.external_untitled_project",
                        id = target.session.serial()
                    )
                    .into_owned()
                })
        } else {
            t!(
                "status.external_project_count",
                count = self.applied_projects.len()
            )
            .into_owned()
        };
        // Put the target first so it survives truncation in a narrow side lane.
        t!("status.external_applied_project", project = project).into_owned()
    }

    fn applied(&mut self, projects: Vec<AppliedProject>, cx: &mut Context<Self>) {
        for project in projects {
            if let Some(previous) = self
                .applied_projects
                .iter_mut()
                .find(|old| old.session == project.session)
            {
                *previous = project;
            } else {
                self.applied_projects.push(project);
            }
        }
        // Repeated publications extend one notice without flashing or queuing.
        self.clear_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Self::NOTICE_DURATION).await;
            _ = this.update(cx, |this, cx| {
                this.applied_projects.clear();
                this.clear_task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn external_publications(
    host: &RuntimeHost,
    cursors: &mut HashMap<SessionId, EventCursor>,
) -> Vec<AppliedProject> {
    let runtime = host.read();
    let sessions = runtime.list_sessions();
    cursors.retain(|id, _| sessions.iter().any(|info| info.id == *id));
    let mut projects = Vec::new();
    for info in sessions {
        let Ok(next) = runtime.event_cursor(info.id) else {
            continue;
        };
        let cursor = cursors.entry(info.id).or_insert(EventCursor {
            sequence: 0,
            ..next
        });
        if *cursor != next {
            // A bounded log gap must not turn a version change into a guessed
            // external commit. Advance quietly and observe future publications.
            if let Ok(events) = runtime.events_since(*cursor) {
                if events.iter().any(|event| {
                    event
                        .client
                        .is_some_and(|client| client != host.client_id())
                        && matches!(&event.kind, EventKind::Published(receipt) if receipt.changed)
                }) {
                    projects.push(AppliedProject {
                        session: info.id,
                        path: info.path,
                    });
                }
            }
            *cursor = next;
        }
    }
    projects
}

impl Render for ExternalStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.applied_projects.is_empty() {
            return div().into_any_element();
        }
        let label = self.label();
        let tooltip = self
            .applied_projects
            .iter()
            .map(|project| {
                project
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .or_else(|| self.project_titles.get(&project.session).cloned())
                    .unwrap_or_else(|| {
                        t!(
                            "status.external_untitled_project",
                            id = project.session.serial()
                        )
                        .into_owned()
                    })
            })
            .collect::<Vec<_>>()
            .join("\n");
        div()
            .id("app-external-status")
            .test_support()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .justify_end()
            .pr(compact::rem(12.))
            .overflow_hidden()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .id("app-external-status-label")
                    .test_support()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                            .build(window, cx)
                    })
                    .child(label),
            )
            .into_any_element()
    }
}
