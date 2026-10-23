use crate::Editor;
use editor_core::{
    Command, CommandId, CommandScope, CommandSource, CommandTarget, TimelineBinding,
    ViewportBinding,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalTarget {
    Panel(editor_core::PanelKind),
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

pub(super) fn command_at(editor: &Editor, id: CommandId, target: Option<LocalTarget>) -> Command {
    match (id.spec().scope, target) {
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
