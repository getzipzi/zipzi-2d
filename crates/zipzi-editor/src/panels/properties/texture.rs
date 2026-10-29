use super::*;
use crate::authoring::{ActionAvailability, AuthoringContext, texture::Control};
use zipzi_editor_core::{EditingContext, ObjectKind};

pub(super) fn rows(
    doc: &zipzi_editor_core::Document,
    target: &PropertyTarget,
    loading: bool,
) -> Vec<Row> {
    if target.object.kind != ObjectKind::Texture || target.timeline.is_some() {
        return vec![];
    }
    let Ok(info) = doc.texture_info(&target.object.id) else {
        return vec![];
    };
    let mut rows = vec![];
    let status = if let Some(c) = &info.asset.content {
        format!("{} · {} × {}", t!("texture.ready"), c.width, c.height)
    } else if let Some(source) = &info.asset.unresolved_source {
        format!("{}: {source}", t!("texture.missing"))
    } else {
        t!("texture.empty").into_owned()
    };
    rows.push(Row::Detail(properties::DetailRow::Text {
        id: Some("texture-status".into()),
        value: status.into(),
    }));
    if let Some(c) = &info.asset.content {
        rows.push(Row::Detail(properties::DetailRow::Text {
            id: Some("texture-source".into()),
            value: c.source.clone().into(),
        }));
    }
    if target.context == EditingContext::Modeling {
        for control in [
            if loading {
                Control::Cancel
            } else {
                Control::Load
            },
            Control::Delete,
        ] {
            let label = if control == Control::Load {
                if info.asset.content.is_some() {
                    t!("texture.replace_content").into()
                } else if info.asset.unresolved_source.is_some() {
                    t!("texture.repair").into()
                } else {
                    control.label()
                }
            } else {
                control.label()
            };
            rows.push(Row::TextureAction {
                object: target.object.clone(),
                control,
                label,
            });
        }
    }
    let index = doc.object_index();
    rows.push(Row::Detail(properties::DetailRow::Text {
        id: None,
        value: format!("{}: {}", t!("editor.used_by"), info.users.len()).into(),
    }));
    for user in info.users {
        rows.push(Row::BindingNavigate {
            label: index.name(&user).to_owned().into(),
            object: user,
        });
    }
    rows
}
impl PropertiesPanel {
    pub(super) fn texture_action(
        &self,
        object: &ObjectRef,
        control: Control,
        label: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let view = owner.read(cx);
        let context = AuthoringContext::capture(&view.editor);
        let state = view
            .editor
            .document()
            .and_then(|d| d.resource_capabilities(&object.id).ok());
        let Some(state) = state else {
            return Empty.into_any_element();
        };
        // Referenced resources may be deleted with an explicitly reviewed
        // replacement. Native planning still validates every user and lock.
        let capability = if control == Control::Load {
            &state.domain.load_content
        } else {
            &state.domain.patch
        };
        let availability = if control == Control::Delete {
            view.editor.document().and_then(|d| {
                ActionAvailability::for_object(
                    d,
                    object,
                    zipzi_editor_core::AuthoringAction::Delete,
                )
            })
        } else {
            ActionAvailability::from_core(capability)
        };
        let Some(availability) = availability else {
            return Empty.into_any_element();
        };
        let owner = self.owner.clone();
        let asset = object.id.clone();
        compact::button(Button::new(SharedString::from(format!(
            "texture-{}-{asset}",
            control.key()
        ))))
        .ghost()
        .label(label.clone())
        .tooltip(availability.reason.unwrap_or(label.clone()))
        .disabled(
            !matches!(control, Control::Cancel | Control::Delete)
                && (self.disabled || !availability.enabled),
        )
        .w_full()
        .justify_start()
        .on_click(move |_, window, cx| {
            if let Some(context) = context {
                let _ = owner.update(cx, |v, cx| {
                    v.run_texture_control(context, Some(asset.clone()), control, window, cx)
                });
            }
        })
        .into_any_element()
    }
}
