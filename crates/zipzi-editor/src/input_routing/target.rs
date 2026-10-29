use crate::Editor;
use zipzi_editor_core::{
    Command, CommandId, CommandScope, CommandSource, CommandTarget, TimelineBinding,
    ViewportBinding,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalTarget {
    Panel(zipzi_editor_core::PanelKind),
    Viewport(ViewportBinding),
    Timeline(TimelineBinding),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InputTarget {
    pub local: LocalTarget,
    pub idle_escape: Option<CommandId>,
}
impl InputTarget {
    pub fn new(local: LocalTarget) -> Self {
        Self {
            local,
            idle_escape: None,
        }
    }
}

pub(crate) fn command_at(editor: &Editor, id: CommandId, target: Option<LocalTarget>) -> Command {
    match (id.spec().scope, target) {
        (CommandScope::Objects, Some(LocalTarget::Viewport(binding))) => {
            editor.command_for_viewport(id, binding, CommandSource::Shortcut)
        }
        (CommandScope::Objects, Some(LocalTarget::Panel(panel)))
            if matches!(
                panel,
                zipzi_editor_core::PanelKind::PartsTree | zipzi_editor_core::PanelKind::Deformers
            ) =>
        {
            Command {
                id,
                source: CommandSource::Shortcut,
                target: editor
                    .active_id()
                    .map(|document| CommandTarget::Hierarchy {
                        document,
                        projection: if panel == zipzi_editor_core::PanelKind::Deformers {
                            zipzi_editor_core::PaletteProjection::Deformation
                        } else {
                            zipzi_editor_core::PaletteProjection::Organization
                        },
                    })
                    .unwrap_or(CommandTarget::Unavailable),
            }
        }
        (CommandScope::Objects, Some(LocalTarget::Panel(panel))) if object_panel(panel) => {
            Command {
                id,
                source: CommandSource::Shortcut,
                target: editor
                    .active_id()
                    .map(CommandTarget::Document)
                    .unwrap_or(CommandTarget::Unavailable),
            }
        }
        (CommandScope::Objects, _) => Command {
            id,
            source: CommandSource::Shortcut,
            target: CommandTarget::Unavailable,
        },
        (CommandScope::Viewport, Some(LocalTarget::Viewport(binding))) => {
            editor.command_for_viewport(id, binding, CommandSource::Shortcut)
        }
        (CommandScope::Timeline, Some(LocalTarget::Timeline(binding))) => {
            editor.command_for_timeline(id, binding, CommandSource::Shortcut)
        }
        (CommandScope::Viewport | CommandScope::Timeline, _) => Command {
            id,
            source: CommandSource::Shortcut,
            target: CommandTarget::Unavailable,
        },
        (CommandScope::Document, Some(LocalTarget::Viewport(binding))) => Command {
            id,
            source: CommandSource::Shortcut,
            target: CommandTarget::Document(binding.document),
        },
        _ => editor.command(id).with_source(CommandSource::Shortcut),
    }
}

pub(super) fn object_panel(panel: zipzi_editor_core::PanelKind) -> bool {
    use zipzi_editor_core::PanelKind::*;
    matches!(
        panel,
        PartsTree
            | Deformers
            | DrawOrder
            | Parameters
            | Properties
            | Keyforms
            | BlendShapes
            | Glue
            | Pose
            | Resources
            | AnimationMotions
            | AnimationExpressions
            | AnimationPhysics
            | AnimationPose
    )
}
