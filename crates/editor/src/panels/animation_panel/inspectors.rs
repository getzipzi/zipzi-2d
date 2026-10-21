//! Read-only inspectors. Resource definitions come from the document; runtime
//! values come exclusively from the same presented snapshot as the preview.
use super::*;
pub(crate) use crate::properties::part_name;
use editor_core::Document;
use kasane_core::document::PosePartRef;
use kasane_sdk::MotionSnapshot;

pub(crate) use crate::ui::property_details::{field, heading};

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
