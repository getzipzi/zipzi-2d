//! Loading feedback refreshes worker phases without redrawing the editor.
use super::StatusBar;
use crate::{t, ui::compact};
use gpui_kit::{
    component::{ActiveTheme, Sizable, progress::Progress, tooltip::Tooltip},
    prelude::*,
    *,
};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;
use zipzi_editor_core::{LoadProgress, LoadStage};

pub(super) struct TaskActivity {
    id: Uuid,
    source: PathBuf,
    importing: bool,
    stage: LoadStage,
    operation: Option<String>,
    _refresh: Option<Task<()>>,
}

impl StatusBar {
    pub(crate) fn begin_load(
        &mut self,
        source: PathBuf,
        importing: bool,
        progress: LoadProgress,
        cx: &mut Context<Self>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let stage = progress.stage();
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let stage = progress.stage();
                let active = this.update(cx, |this, cx| {
                    let Some(load) = this.tasks.iter_mut().find(|load| load.id == id) else {
                        return false;
                    };
                    if load.stage != stage {
                        load.stage = stage;
                        cx.notify();
                    }
                    true
                });
                if !matches!(active, Ok(true)) {
                    break;
                }
            }
        });
        self.tasks.push(TaskActivity {
            id,
            source,
            importing,
            stage,
            operation: None,
            _refresh: Some(refresh),
        });
        cx.notify();
        id
    }

    pub(crate) fn begin_task(
        &mut self,
        operation: String,
        source: PathBuf,
        cx: &mut Context<Self>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.tasks.push(TaskActivity {
            id,
            source,
            importing: false,
            stage: LoadStage::Queued,
            operation: Some(operation),
            _refresh: None,
        });
        cx.notify();
        id
    }

    pub(crate) fn finish_task(&mut self, id: Uuid, cx: &mut Context<Self>) {
        // Dropping the entry also stops its refresh task, including on failure.
        self.tasks.retain(|load| load.id != id);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn load_states(&self) -> Vec<(PathBuf, LoadStage)> {
        self.tasks
            .iter()
            .filter(|task| task.operation.is_none())
            .map(|load| (load.source.clone(), load.stage))
            .collect()
    }
}

impl TaskActivity {
    fn stage_label(&self) -> String {
        t!(match self.stage {
            LoadStage::Queued => "project.load_queued",
            LoadStage::Reading if self.importing => "project.load_importing",
            LoadStage::Reading => "project.load_reading",
            LoadStage::PreparingView => "project.load_preparing_view",
        })
        .into_owned()
    }

    fn label(&self) -> String {
        if let Some(operation) = &self.operation {
            let name = self
                .source
                .file_name()
                .unwrap_or(self.source.as_os_str())
                .to_string_lossy();
            return format!("{operation} · {name}");
        }
        let action = if self.importing {
            t!("project.import")
        } else {
            t!("project.open")
        };
        let name = self
            .source
            .file_name()
            .unwrap_or(self.source.as_os_str())
            .to_string_lossy();
        t!(
            "project.load_file",
            action = action,
            name = name,
            stage = self.stage_label()
        )
        .into_owned()
    }
}

pub(super) fn render_task_activity(tasks: &[TaskActivity], cx: &App) -> AnyElement {
    let Some(current) = tasks.first() else {
        return div().into_any_element();
    };
    let details = tasks
        .iter()
        .map(|load| format!("{}\n{}", load.label(), load.source.display()))
        .collect::<Vec<_>>()
        .join("\n\n");
    div()
        .id("app-status-tasks")
        .test_support()
        .min_w_0()
        .flex()
        .items_center()
        .gap(compact::rem(6.))
        .text_color(cx.theme().muted_foreground)
        .tooltip(move |window, cx| Tooltip::new(details.clone()).build(window, cx))
        .child(
            div().w(compact::rem(48.)).flex_shrink_0().child(
                Progress::new("app-status-task-progress")
                    .loading(true)
                    .xsmall()
                    .accessibility_label(current.label()),
            ),
        )
        .child(div().min_w_0().truncate().child(current.label()))
        .when(tasks.len() > 1, |e| {
            e.child(
                div()
                    .flex_shrink_0()
                    .child(t!("project.load_others", count = tasks.len() - 1).into_owned()),
            )
        })
        .into_any_element()
}
