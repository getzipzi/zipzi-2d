//! Detect foreign publications and describe them for ordinary editor feedback.
use crate::t;
use std::{collections::HashMap, path::PathBuf};
use zipzi_editor_core::{Editor, RuntimeHost};
use zipzi_runtime::{EventCursor, EventKind, SessionId};

pub(crate) struct ExternalChanges {
    cursors: HashMap<SessionId, EventCursor>,
}

struct AppliedProject {
    session: SessionId,
    path: Option<PathBuf>,
}

impl ExternalChanges {
    pub(crate) fn new(runtime: &RuntimeHost) -> Self {
        let runtime = runtime.read();
        Self {
            cursors: runtime
                .list_sessions()
                .into_iter()
                .filter_map(|info| {
                    runtime
                        .event_cursor(info.id)
                        .ok()
                        .map(|cursor| (info.id, cursor))
                })
                .collect(),
        }
    }

    pub(crate) fn poll(&mut self, editor: &Editor) -> Option<String> {
        let projects = external_publications(&editor.runtime_host(), &mut self.cursors);
        if projects.is_empty() {
            return None;
        }
        let active = editor
            .active_id()
            .and_then(|id| editor.runtime_session_id(id));
        if projects
            .iter()
            .all(|project| Some(project.session) == active)
        {
            return Some(t!("status.external_applied").into_owned());
        }
        let project = if projects.len() == 1 {
            let target = &projects[0];
            target
                .path
                .as_ref()
                .and_then(|path| path.parent())
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .or_else(|| {
                    editor
                        .documents()
                        .into_iter()
                        .find(|doc| editor.runtime_session_id(doc.id) == Some(target.session))
                        .and_then(|doc| doc.title)
                })
                .unwrap_or_else(|| {
                    t!(
                        "status.external_untitled_project",
                        id = target.session.serial()
                    )
                    .into_owned()
                })
        } else {
            t!("status.external_project_count", count = projects.len()).into_owned()
        };
        Some(t!("status.external_applied_project", project = project).into_owned())
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
