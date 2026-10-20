//! Session-only playback speed, using the shared Select control.
use super::*;
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectItem, SelectState},
};

#[derive(Clone)]
pub(super) struct Speed(f32);
impl SelectItem for Speed {
    type Value = f32;

    fn title(&self) -> SharedString {
        let value = self.0.to_string();
        format!("{}×", value.trim_start_matches('0')).into()
    }
    fn value(&self) -> &f32 {
        &self.0
    }
}

pub(super) struct Control {
    context: editor_core::EditingContextToken,
    state: Entity<SelectState<Vec<Speed>>>,
    _change: Subscription,
}

impl Timeline {
    pub(super) fn sync_playback_speed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let editor = &owner.read(cx).editor;
        let Some(context) = editor.editing_context_token() else {
            self.playback_speed = None;
            return;
        };
        let Some(animation) = editor.animation() else {
            return;
        };
        let speed = animation.playback_speed;
        if self
            .playback_speed
            .as_ref()
            .is_none_or(|control| control.context != context)
        {
            let choices: Vec<_> = [0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 3., 4.]
                .into_iter()
                .map(Speed)
                .collect();
            let selected = choices
                .iter()
                .position(|choice| choice.0 == speed)
                .map(IndexPath::new);
            let state = cx.new(|cx| SelectState::new(choices, selected, window, cx));
            let change = cx.subscribe_in(
                &state,
                window,
                move |this, source, event: &SelectEvent<Vec<Speed>>, window, cx| {
                    if !this.allowed(window, cx)
                        || this
                            .playback_speed
                            .as_ref()
                            .is_none_or(|control| &control.state != source)
                    {
                        return;
                    }
                    if let SelectEvent::Confirm(Some(speed)) = event {
                        let _ = this.owner.update(cx, |owner, cx| {
                            if !owner.editor.editing_context_token_valid(context) {
                                return;
                            }
                            let result = owner
                                .editor
                                .set_animation_playback_speed(context.document, *speed);
                            owner.animation_anchor = None;
                            owner.complete_edit(result, cx);
                        });
                    }
                },
            );
            self.playback_speed = Some(Control {
                context,
                state,
                _change: change,
            });
        }
        let control = self.playback_speed.as_ref().unwrap();
        if control.state.read(cx).selected_value() != Some(&speed) {
            control
                .state
                .update(cx, |state, cx| state.set_selected_value(&speed, window, cx));
        }
    }

    pub(super) fn render_playback_speed(&self, allowed: bool) -> AnyElement {
        let Some(control) = &self.playback_speed else {
            return Empty.into_any_element();
        };
        div()
            .id("timeline-playback-speed")
            .test_support()
            .flex_shrink_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w(compact::rem(56.))
            .h(compact::rem(compact::HEADER_CONTROL))
            .tooltip(|window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(
                    t!("timeline.playback_speed").into_owned(),
                )
                .build(window, cx)
            })
            .child(
                compact::select(Select::new(&control.state))
                    .h(compact::rem(compact::HEADER_CONTROL))
                    .text_size(compact::rem(compact::CAPTION))
                    .px(compact::rem(3.))
                    .w_full()
                    .menu_width(compact::rem(96.))
                    .accessibility_label(t!("timeline.playback_speed"))
                    .disabled(!allowed),
            )
            .into_any_element()
    }
}
