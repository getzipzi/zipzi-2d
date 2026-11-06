//! Detect foreign publications and describe them for ordinary editor feedback.
use crate::t;
use std::{collections::HashMap, path::PathBuf};
use zipzi_editor_core::{Editor, RuntimeHost};
use zipzi_runtime::{SessionKey, Version};

pub(crate) struct ExternalChanges {
    cursors: HashMap<SessionKey, Version>,
}

struct AppliedProject {
    session: SessionKey,
    path: Option<PathBuf>,
}

impl ExternalChanges {
    pub(crate) fn new(runtime: &RuntimeHost) -> Self {
        let client = runtime.client_id();
        let runtime = runtime.read();
        Self {
            cursors: runtime
                .session_keys(client)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|id| {
                    runtime
                        .subscribe(client, id)
                        .ok()
                        .map(|subscription| (id, subscription.cursor))
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
                .and_then(|path| path.file_stem())
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
                        id = target.session.to_string()
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
    cursors: &mut HashMap<SessionKey, Version>,
) -> Vec<AppliedProject> {
    let runtime = host.read();
    let client = host.client_id();
    let sessions = runtime.session_keys(client).unwrap_or_default();
    cursors.retain(|id, _| sessions.contains(id));
    let mut projects = Vec::new();
    for session in sessions {
        let Ok(subscription) = runtime.subscribe(client, session) else {
            continue;
        };
        let next = subscription.cursor;
        if let Some(cursor) = cursors.insert(session, next) {
            if cursor != next {
                if let Ok(events) = runtime.publication_events_since(client, cursor) {
                    if events.iter().any(|event| {
                        event.client != client
                            && event.publication.kind != zipzi_sdk::PublicationKind::Unchanged
                    }) {
                        projects.push(AppliedProject {
                            session,
                            path: runtime.session_read(client, session).ok().and_then(|sdk| {
                                sdk.project().map(|project| project.path().to_owned())
                            }),
                        });
                    }
                }
            }
        }
    }
    projects
}
