use super::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{DocumentId, ExportCanvas, TransferFormat};

pub(crate) struct ExportForm {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    revision: u64,
    format: TransferFormat,
    canvas: ExportCanvas,
    inputs: [Entity<InputState>; 8],
    dialog: FormDialogState,
    invalid: Option<usize>,
    _subscriptions: Vec<Subscription>,
}
impl ExportForm {
    pub fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        format: TransferFormat,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let dialog_open = window.has_active_dialog(cx);
        let view = entity.read(cx);
        anyhow::ensure!(
            view.editor.active_id() == Some(document) && !view.modal && !dialog_open,
            "Document changed"
        );
        let canvas = view.editor.export_canvas(document)?;
        let revision = view.editor.document().unwrap().model_generation();
        let values = [
            canvas.settings.pixels_per_unit,
            canvas.width,
            canvas.height,
            canvas.origin_x,
            canvas.origin_y,
            canvas.settings.padding_fraction * 100.,
            canvas.settings.padding_pixels,
            canvas.settings.minimum_extent,
        ];
        let form = cx.new(|cx| {
            let inputs = values.map(|value| {
                cx.new(|cx| InputState::new(window, cx).default_value(value.to_string()))
            });
            let mut subscriptions = Vec::new();
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event: &InputEvent, window, cx| {
                        if this.dialog.closed() {
                            return;
                        }
                        match event {
                            InputEvent::Change => {
                                this.dialog.edited();
                                this.validate(cx);
                                cx.notify();
                            }
                            _ => form_dialog::input_enter::<Self>(event, window, cx),
                        }
                    },
                ));
            }
            let mut state = Self {
                owner,
                document,
                revision,
                format,
                canvas,
                inputs,
                dialog: FormDialogState::new(cx),
                invalid: None,
                _subscriptions: subscriptions,
            };
            state.validate(cx);
            state
        });
        let focus = form.read(cx).inputs[0].read(cx).focus_handle(cx);
        form_dialog::open(
            form,
            FormDialogOptions {
                title: t!("transfer.settings").into_owned(),
                submit_label: t!("transfer.continue").into(),
                footer_id: "export-footer",
                cancel_id: "export-cancel",
                submit_id: "export-continue",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
    fn draft(&self, cx: &App) -> anyhow::Result<ExportCanvas> {
        let mut draft = self.canvas;
        for i in 0..8 {
            if !self.active(i) {
                continue;
            }
            let value: f32 = self.inputs[i]
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("{}", t!("transfer.invalid_number")))?;
            anyhow::ensure!(value.is_finite(), "{}", t!("transfer.invalid_number"));
            match i {
                0 => draft.settings.pixels_per_unit = value,
                1 => draft.width = value,
                2 => draft.height = value,
                3 => draft.origin_x = value,
                4 => draft.origin_y = value,
                5 => draft.settings.padding_fraction = value / 100.,
                6 => draft.settings.padding_pixels = value,
                7 => draft.settings.minimum_extent = value,
                _ => unreachable!(),
            }
        }
        draft.validated()?;
        Ok(draft)
    }
    fn active(&self, i: usize) -> bool {
        i == 0 || (self.canvas.manual && (1..=4).contains(&i)) || (!self.canvas.manual && i >= 5)
    }
    fn validate(&mut self, cx: &App) {
        self.invalid = (0..8).find(|i| {
            if !self.active(*i) {
                return false;
            }
            self.inputs[*i]
                .read(cx)
                .value()
                .trim()
                .parse::<f32>()
                .map_or(true, |v| {
                    !v.is_finite()
                        || if *i <= 2 {
                            v <= 0.
                        } else if *i >= 5 {
                            v < 0.
                        } else {
                            false
                        }
                })
        });
        self.dialog.set_validation(self.draft(cx).is_ok(), true);
    }
}
impl FormDialogView for ExportForm {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error.to_string()
    }
    fn focus_invalid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.invalid {
            window.focus(&self.inputs[i].read(cx).focus_handle(cx), cx);
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let canvas = self.draft(cx)?;
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let snapshot = owner.update(cx, |view, cx| {
            let snapshot = view
                .editor
                .prepare_export(self.document, self.revision, canvas)?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(snapshot)
        })?;
        let document = self.document;
        let format = self.format;
        window.defer(cx, move |window, cx| {
            owner.update(cx, |view, cx| {
                view.pick_export(document, format, snapshot, window, cx)
            })
        });
        Ok(())
    }
}
impl Render for ExportForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = [
            t!("transfer.ppu"),
            t!("transfer.width"),
            t!("transfer.height"),
            t!("transfer.origin_x"),
            t!("transfer.origin_y"),
            t!("transfer.padding_fraction"),
            t!("transfer.padding_pixels"),
            t!("transfer.minimum_extent"),
        ];
        let mut content = div()
            .id("export-settings")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .max_h(crate::ui::compact::scaled(440., cx))
            .overflow_y_scroll()
            .child(
                Checkbox::new("export-manual")
                    .label(t!("transfer.manual").into_owned())
                    .checked(self.canvas.manual)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.canvas.manual = *value;
                        this.dialog.edited();
                        this.validate(cx);
                        cx.notify();
                    })),
            );
        let ids = [
            "export-ppu",
            "export-width",
            "export-height",
            "export-origin-x",
            "export-origin-y",
            "export-padding-fraction",
            "export-padding-pixels",
            "export-minimum-extent",
        ];
        for (i, label) in labels.into_iter().enumerate() {
            if !self.active(i) {
                continue;
            }
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(label.clone().into_owned()))
                    .child(
                        div()
                            .id(ids[i])
                            .test_support()
                            .w(crate::ui::compact::scaled(160., cx))
                            .child(
                                crate::ui::compact::input(Input::new(&self.inputs[i]))
                                    .id(("export-value", i))
                                    .aria_label(label.into_owned()),
                            ),
                    ),
            );
        }
        if let Some(error) = &self.dialog.error {
            content = content.child(div().text_color(cx.theme().danger).child(error.clone()));
        }
        content
    }
}
