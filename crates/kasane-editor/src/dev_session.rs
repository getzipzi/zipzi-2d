//! Restore open project tabs across `scripts/dev.sh` process restarts.
//! Only file paths are recorded; project contents remain owned by their files.

use editor_core::{ConfigStore, Editor, LoadRequest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE: &str = "session.json";

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct OpenDocuments {
    paths: Vec<PathBuf>,
    active: Option<PathBuf>,
}

impl OpenDocuments {
    fn capture(editor: &Editor) -> Self {
        let active_id = editor.active_id();
        let documents = editor.documents();
        Self {
            active: documents
                .iter()
                .find(|document| Some(document.id) == active_id)
                .and_then(|document| document.path.clone()),
            paths: documents
                .into_iter()
                .filter_map(|document| document.path)
                .collect(),
        }
    }

    fn restore(self, editor: &mut Editor) -> anyhow::Result<()> {
        let mut first_error = None;
        for path in self.paths {
            let result = (|| {
                let Some(task) = editor.prepare_load(LoadRequest::Open(path))? else {
                    return Ok(());
                };
                editor.finish_load(task.run())?;
                Ok::<_, anyhow::Error>(())
            })();
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        if let Some(path) = self.active {
            if let Some(id) = editor
                .documents()
                .into_iter()
                .find(|document| document.path.as_ref() == Some(&path))
                .map(|document| document.id)
            {
                editor.activate(id)?;
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

pub(super) struct DevSession {
    store: ConfigStore,
    last: Option<OpenDocuments>,
}

impl DevSession {
    pub(super) fn from_env() -> Option<Self> {
        std::env::var_os("KASANE_DEV_SESSION_DIR").map(|root| Self {
            store: ConfigStore::new(PathBuf::from(root)),
            last: None,
        })
    }

    pub(super) fn restore(&mut self, editor: &mut Editor) -> anyhow::Result<()> {
        let saved = self.store.read::<OpenDocuments>(FILE)?;
        self.last = saved.clone();
        if let Some(saved) = saved {
            saved.restore(editor)?;
        }
        Ok(())
    }

    pub(super) fn sync(&mut self, editor: &Editor) -> anyhow::Result<()> {
        let current = OpenDocuments::capture(editor);
        if self.last.as_ref() != Some(&current) {
            self.store.write(FILE, &current)?;
            self.last = Some(current);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::Document;

    #[test]
    fn open_tabs_and_active_tab_survive_restart() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut editor = Editor::empty();
        let first = root.path().join("first.kasane");
        let second = root.path().join("second.kasane");
        for path in [&first, &second] {
            let mut document = Document::new_blank()?;
            document.save(path)?;
            let task = editor
                .prepare_load(LoadRequest::Open(path.clone()))?
                .unwrap();
            editor.finish_load(task.run())?;
        }
        let first_id = editor.documents()[0].id;
        editor.activate(first_id)?;
        let expected_paths: Vec<_> = editor.documents().into_iter().map(|d| d.path).collect();
        let mut session = DevSession {
            store: ConfigStore::new(root.path().join("dev-session")),
            last: None,
        };
        session.sync(&editor)?;

        let mut restored = Editor::empty();
        let mut restarted_session = DevSession {
            store: session.store,
            last: None,
        };
        restarted_session.restore(&mut restored)?;
        assert_eq!(restored.document_count(), 2);
        assert_eq!(restored.documents()[0].path, expected_paths[0]);
        assert_eq!(restored.documents()[1].path, expected_paths[1]);
        assert_eq!(restored.summary().unwrap().path, expected_paths[0]);

        for document in restored.documents() {
            restored.close_document(document.id, false)?;
        }
        restarted_session.sync(&restored)?;
        let mut next_restart = Editor::empty();
        restarted_session.restore(&mut next_restart)?;
        assert_eq!(next_restart.document_count(), 0);
        Ok(())
    }
}
