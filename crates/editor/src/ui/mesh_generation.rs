use super::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use crate::{EditorView, t};
use editor_core::{CommandId, DocumentId, EditorCommand, MeshGridOptions, ObjectRef};
use gpui_kit::component::{
    ActiveTheme, Sizable, WindowExt,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{prelude::*, *};

pub(crate) struct MeshGenerationForm {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    revision: u64,
    target: ObjectRef,
    name: String,
    square: bool,
    inputs: [Entity<InputState>; 2],
    dialog: FormDialogState,
    validation: anyhow::Result<MeshGridOptions>,
    _subscriptions: Vec<Subscription>,
}

impl MeshGenerationForm {
    pub fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.stale")))?;
        let dialog_open = window.has_active_dialog(cx);
        let state = entity.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(document)
                && !state.modal
                && !dialog_open
                && state
                    .editor
                    .command_available(state.editor.command(CommandId::GenerateMesh)),
            t!("mesh_generation.stale")
        );
        let doc = state.editor.document().unwrap();
        let target = doc.mesh_generation_target()?;
        let name = doc.object_index().name(&target).to_owned();
        let revision = doc.mesh_generation_revision();
        let square = doc.mesh_grid_requires_square(&target);
        let form = cx.new(|cx| {
            let defaults = MeshGridOptions::default();
            let inputs = [defaults.columns, defaults.rows]
                .map(|n| cx.new(|cx| InputState::new(window, cx).default_value(n.to_string())));
            let mut subscriptions = Vec::new();
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.dialog.edited();
                            this.refresh(cx);
                            cx.notify();
                        }
                        form_dialog::input_enter(event, window, cx);
                    },
                ));
            }
            subscriptions.push(cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.dialog.closed() {
                    this.refresh(cx);
                    cx.notify();
                }
            }));
            Self {
                owner,
                document,
                revision,
                target,
                name,
                square,
                inputs,
                dialog: FormDialogState::new(cx),
                validation: Ok(defaults),
                _subscriptions: subscriptions,
            }
        });
        form.update(cx, |form, cx| form.refresh(cx));
        let options = FormDialogOptions {
            title: t!("mesh_generation.title").into_owned(),
            submit_label: t!("mesh_generation.generate").into(),
            footer_id: "mesh-generation-footer",
            cancel_id: "mesh-generation-cancel",
            submit_id: "mesh-generation-submit",
            initial_focus: form.read(cx).inputs[0].read(cx).focus_handle(cx),
        };
        form_dialog::open(form, options, window, cx);
        Ok(())
    }

    fn validate(&self, cx: &App) -> anyhow::Result<MeshGridOptions> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.stale")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document)
                && !state.modal
                && !state.editor.is_busy()
                && !state.editor.exiting(),
            t!("mesh_generation.stale")
        );
        let doc = state.editor.document().unwrap();
        anyhow::ensure!(
            doc.mesh_generation_revision() == self.revision && !doc.has_pending_edit(),
            t!("mesh_generation.stale")
        );
        anyhow::ensure!(
            !doc.object_state(&self.target).locked(),
            t!("mesh_generation.locked")
        );
        let parse = |i: usize| {
            self.inputs[i]
                .read(cx)
                .value()
                .trim()
                .parse::<usize>()
                .map_err(|_| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))
        };
        let options = MeshGridOptions {
            columns: parse(0)?,
            rows: parse(1)?,
        };
        options
            .validate()
            .map_err(|_| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))?;
        anyhow::ensure!(
            !self.square || options.columns == options.rows,
            t!("mesh_generation.square_required")
        );
        doc.validate_mesh_grid(&self.target, options)?;
        Ok(options)
    }

    fn refresh(&mut self, cx: &App) {
        self.validation = self.validate(cx);
        self.dialog.set_validation(self.validation.is_ok(), true);
    }
}

impl FormDialogView for MeshGenerationForm {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error.to_string()
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let options = self.validate(cx)?;
        self.owner.upgrade().unwrap().update(cx, |owner, cx| {
            owner.editor.execute_for(
                self.document,
                EditorCommand::GenerateMeshGrid {
                    object: self.target.clone(),
                    options,
                    expected_revision: self.revision,
                },
            )?;
            owner.edit_error = None;
            owner.redraw(cx);
            Ok(())
        })
    }
}

impl Render for MeshGenerationForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("mesh-generation-form")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .child(self.name.clone())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("mesh_generation.description")),
            );
        for (i, label) in [t!("mesh_generation.columns"), t!("mesh_generation.rows")]
            .into_iter()
            .enumerate()
        {
            body = body.child(
                div().text_xs().child(label.clone()).child(
                    Input::new(&self.inputs[i])
                        .id(if i == 0 {
                            "mesh-generation-columns"
                        } else {
                            "mesh-generation-rows"
                        })
                        .small()
                        .aria_label(label),
                ),
            );
        }
        if let Ok(options) = self.validation {
            body = body.child(
                div()
                    .id("mesh-generation-counts")
                    .test_support()
                    .text_xs()
                    .child(t!(
                        "mesh_generation.counts",
                        vertices = options.vertex_count().unwrap(),
                        triangles = options.columns * options.rows * 2
                    )),
            );
        }
        body.when(self.square, |body| {
            body.child(div().text_xs().child(t!("mesh_generation.square_required")))
        })
        .when_some(
            self.dialog
                .error
                .clone()
                .or_else(|| self.validation.as_ref().err().map(ToString::to_string)),
            |body, error| {
                body.child(
                    div()
                        .id("mesh-generation-error")
                        .test_support()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            },
        )
    }
}
