//! Read-only inspectors. Resource definitions come from the document; runtime
//! values come exclusively from the same presented snapshot as the preview.
use super::*;
pub(crate) use crate::properties::part_name;
use editor_core::Document;
use kasane_core::document::PosePartRef;
use kasane_sdk::MotionSnapshot;

pub(crate) use crate::ui::property_details::{field, heading, property};

fn parameter_name(document: &Document, id: &str) -> String {
    document
        .animation_catalog()
        .parameters
        .iter()
        .find(|p| p.id == id)
        .map_or_else(
            || id.to_string(),
            |p| format!("{} · {}", p.name, p.runtime_id),
        )
}

fn frame_status(mut content: Stateful<Div>, snapshot: Option<&MotionSnapshot>) -> Stateful<Div> {
    content = content.child(t!("workspace.inspector_frame_hint"));
    content.child(if let Some(snapshot) = snapshot {
        t!(
            "workspace.presented_time",
            time = format!("{:.3}", snapshot.time)
        )
        .into_owned()
    } else {
        t!("workspace.no_presented_values").into_owned()
    })
}

pub(crate) fn physics(
    mut content: Stateful<Div>,
    document: &Document,
    snapshot: Option<&MotionSnapshot>,
    cx: &App,
) -> Stateful<Div> {
    let Some(physics) = document.physics_definition() else {
        return content.child(
            div()
                .id("animation-physics-empty")
                .test_support()
                .child(t!("workspace.no_physics")),
        );
    };
    content = frame_status(content, snapshot)
        .child(
            div()
                .id("animation-physics-value-hint")
                .text_color(cx.theme().muted_foreground)
                .child(t!("workspace.physics_values_label"))
                .tooltip(|window, cx| {
                    Tooltip::new(t!("workspace.physics_final_values").into_owned())
                        .build(window, cx)
                }),
        )
        .child(property(
            "animation-physics-fps",
            t!("workspace.frame_rate"),
            physics.data.meta.fps.map_or_else(
                || t!("workspace.resource_default").into_owned(),
                |fps| format!("{fps:.1} fps"),
            ),
            cx,
        ));
    for (index, rig) in physics.data.settings.iter().enumerate() {
        let name = physics
            .data
            .meta
            .dictionary
            .iter()
            .find(|entry| entry.id == rig.id)
            .map_or_else(|| rig.id.clone(), |entry| entry.name.clone());
        content = content.child(heading(name, cx)).child(
            div()
                .id(("animation-physics-group", index))
                .test_support()
                .child(t!(
                    "workspace.physics_counts",
                    inputs = rig.inputs.len(),
                    outputs = rig.outputs.len(),
                    particles = rig.vertices.len()
                )),
        );
        for (is_output, targets) in [
            (
                false,
                rig.inputs
                    .iter()
                    .map(|input| (&input.source.id, input.kind, input.weight, input.reflect))
                    .collect::<Vec<_>>(),
            ),
            (
                true,
                rig.outputs
                    .iter()
                    .map(|output| {
                        (
                            &output.destination.id,
                            output.kind,
                            output.weight,
                            output.reflect,
                        )
                    })
                    .collect(),
            ),
        ] {
            content = content.child(heading(
                if is_output {
                    t!("workspace.physics_outputs")
                } else {
                    t!("workspace.physics_inputs")
                },
                cx,
            ));
            for (entry, (runtime_id, axis, weight, reflect)) in targets.into_iter().enumerate() {
                let bound = physics.parameter_bindings.get(runtime_id).filter(|id| {
                    document
                        .animation_catalog()
                        .parameters
                        .iter()
                        .any(|p| &p.id == *id)
                });
                let target = bound.map_or_else(
                    || t!("workspace.unresolved_binding", target = runtime_id).into_owned(),
                    |id| parameter_name(document, id),
                );
                let value = bound
                    .and_then(|id| snapshot?.parameters.get(id))
                    .map_or_else(|| "—".to_string(), |value| format!("{value:.4}"));
                let id = format!(
                    "animation-physics-{}-{index}-{entry}",
                    if is_output { "output" } else { "input" }
                );
                content = content
                    .child(field(SharedString::from(id), target, value, cx))
                    .child(t!(
                        "workspace.physics_channel",
                        axis = format!("{axis:?}"),
                        weight = format!("{weight:.1}"),
                        reflect = if reflect {
                            t!("workspace.yes")
                        } else {
                            t!("workspace.no")
                        }
                    ));
            }
        }
    }
    content
}

pub(crate) fn pose(
    mut content: Stateful<Div>,
    document: &Document,
    snapshot: Option<&MotionSnapshot>,
    cx: &App,
) -> Stateful<Div> {
    let Some(pose) = document.pose_definition() else {
        return content.child(
            div()
                .id("animation-pose-empty")
                .test_support()
                .child(t!("workspace.no_pose")),
        );
    };
    content = frame_status(content, snapshot).child(field(
        "animation-pose-fade",
        t!("workspace.pose_fade"),
        pose.fade_in.map_or_else(
            || t!("workspace.resource_default").into_owned(),
            |fade| format!("{fade:.3} s"),
        ),
        cx,
    ));
    for (group, entries) in pose.groups.iter().enumerate() {
        content = content.child(heading(t!("workspace.pose_group", index = group + 1), cx));
        for (entry, part) in entries.iter().enumerate() {
            for (link, reference) in std::iter::once(&part.part)
                .chain(part.links.iter().flatten())
                .enumerate()
            {
                let label = part_name(document, reference);
                let value = match reference {
                    PosePartRef::Resolved { part_id } => {
                        snapshot.and_then(|snapshot| snapshot.part_opacities.get(part_id))
                    }
                    PosePartRef::Unresolved { .. } => None,
                }
                .map_or_else(|| "—".to_string(), |value| format!("{value:.4}"));
                let label = if link == 0 {
                    label
                } else {
                    t!("workspace.pose_link", part = label).into_owned()
                };
                content = content.child(field(
                    SharedString::from(format!("animation-pose-part-{group}-{entry}-{link}")),
                    label,
                    value,
                    cx,
                ));
            }
        }
    }
    content
}
