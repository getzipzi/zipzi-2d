//! Stable business commands. Platform input, dialogs and focus do not enter core.
use crate::{DocumentId, Editor, EditorCommand};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandId {
    #[serde(rename = "file.new")]
    New,
    #[serde(rename = "file.open")]
    Open,
    #[serde(rename = "file.import-model3")]
    ImportModel3,
    #[serde(rename = "file.import-psd")]
    ImportPsd,
    #[serde(rename = "file.save")]
    Save,
    #[serde(rename = "file.save-as")]
    SaveAs,
    #[serde(rename = "document.close")]
    Close,
    #[serde(rename = "workspace.quit")]
    Quit,
    #[serde(rename = "document.previous")]
    Previous,
    #[serde(rename = "document.next")]
    Next,
    #[serde(rename = "edit.undo")]
    Undo,
    #[serde(rename = "edit.redo")]
    Redo,
    #[serde(rename = "property.commit")]
    CommitProperty,
    #[serde(rename = "property.cancel")]
    CancelProperty,
    #[serde(rename = "view.zoom-in")]
    ZoomIn,
    #[serde(rename = "view.zoom-out")]
    ZoomOut,
    #[serde(rename = "view.reset")]
    ResetView,
    #[serde(rename = "tool.vertices")]
    Vertices,
    #[serde(rename = "tool.cancel")]
    CancelGesture,
    #[serde(rename = "panel.layers")]
    Layers,
    #[serde(rename = "panel.properties")]
    Properties,
    #[serde(rename = "layout.reset")]
    ResetLayout,
    #[serde(rename = "shortcuts.reset")]
    ResetShortcuts,
    #[serde(rename = "shortcuts.show")]
    ShowShortcuts,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandScope {
    Workspace,
    Document,
    Canvas,
    Property,
}
#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub id: CommandId,
    pub label: &'static str,
    pub scope: CommandScope,
    pub repeat: bool,
    pub keys: &'static [&'static str],
}
macro_rules! specs { ($(($id:ident,$label:literal,$scope:ident,$repeat:literal,[$($key:literal),*])),* $(,)?) => {
    pub const COMMANDS: &[CommandSpec] = &[$(CommandSpec { id: CommandId::$id, label:$label, scope:CommandScope::$scope, repeat:$repeat, keys:&[$($key),*] }),*];
}; }
specs![
    (New, "menu.new", Workspace, false, ["cmd-n"]),
    (Open, "menu.open", Workspace, false, ["cmd-o"]),
    (ImportModel3, "menu.import_model3", Workspace, false, []),
    (ImportPsd, "menu.import_psd", Workspace, false, []),
    (Save, "menu.save", Workspace, false, ["cmd-s"]),
    (SaveAs, "menu.save_as", Workspace, false, ["cmd-shift-s"]),
    (Close, "menu.close_document", Workspace, false, ["cmd-w"]),
    (Quit, "app.quit", Workspace, false, ["cmd-q"]),
    (
        Previous,
        "menu.previous_document",
        Workspace,
        false,
        ["ctrl-shift-tab"]
    ),
    (Next, "menu.next_document", Workspace, false, ["ctrl-tab"]),
    (Undo, "menu.undo", Document, false, ["cmd-z"]),
    (Redo, "menu.redo", Document, false, ["cmd-shift-z"]),
    (
        CommitProperty,
        "menu.commit_property",
        Property,
        false,
        ["enter"]
    ),
    (
        CancelProperty,
        "menu.cancel_property",
        Property,
        false,
        ["escape"]
    ),
    (ZoomIn, "menu.zoom_in", Document, true, ["cmd-=", "cmd-+"]),
    (ZoomOut, "menu.zoom_out", Document, true, ["cmd--"]),
    (ResetView, "menu.reset_view", Document, false, ["cmd-0"]),
    (Vertices, "menu.vertices", Canvas, false, ["v"]),
    (
        CancelGesture,
        "menu.cancel_gesture",
        Canvas,
        false,
        ["escape"]
    ),
    (Layers, "editor.layers", Workspace, false, []),
    (Properties, "editor.properties", Workspace, false, []),
    (ResetLayout, "menu.reset_layout", Workspace, false, []),
    (ResetShortcuts, "menu.reset_shortcuts", Workspace, false, []),
    (ShowShortcuts, "menu.shortcuts", Workspace, false, []),
];
impl CommandId {
    pub fn spec(self) -> &'static CommandSpec {
        COMMANDS.iter().find(|s| s.id == self).unwrap()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub id: CommandId,
    pub document: Option<DocumentId>,
}
#[derive(Clone, Copy, Debug)]
pub enum CommandEffect {
    Done,
    Host(Command),
}
impl Editor {
    pub fn command(&self, id: CommandId) -> Command {
        Command {
            id,
            document: self.active_id(),
        }
    }
    pub fn command_available(&self, command: Command) -> bool {
        use CommandId::*;
        let target = command.document.is_some() && command.document == self.active_id();
        match command.id {
            New | Open | ImportModel3 | ImportPsd => !self.exiting(),
            Quit => true,
            Previous | Next => self.document_count() > 1,
            Layers | Properties | ResetLayout | ResetShortcuts | ShowShortcuts => true,
            Close => target,
            Undo => target && self.can_undo(),
            Redo => target && self.can_redo(),
            CommitProperty | CancelProperty => {
                target
                    && !self.is_busy()
                    && !self.gesture_active()
                    && self.document().is_some_and(|d| d.has_pending_edit())
            }
            ZoomIn => target && !self.is_busy() && !self.gesture_active() && self.view().zoom < 3.,
            ZoomOut => {
                target && !self.is_busy() && !self.gesture_active() && self.view().zoom > 0.25
            }
            ResetView => target && !self.is_busy() && !self.gesture_active(),
            CancelGesture => target && !self.is_busy(),
            _ => target && !self.is_busy(),
        }
    }
    pub fn dispatch(&mut self, command: Command) -> Result<CommandEffect> {
        ensure!(
            self.command_available(command),
            "Command is unavailable for this document"
        );
        use CommandId::*;
        match command.id {
            New => {
                self.new_document()?;
            }
            Previous | Next => {
                self.activate(self.adjacent(command.id == Next).unwrap())?;
            }
            Undo => self.execute_for(command.document.unwrap(), EditorCommand::Undo)?,
            Redo => self.execute_for(command.document.unwrap(), EditorCommand::Redo)?,
            CommitProperty => {
                self.execute_for(command.document.unwrap(), EditorCommand::FinishEdit)?
            }
            CancelProperty => {
                self.execute_for(command.document.unwrap(), EditorCommand::CancelEdit)?
            }
            ZoomIn => self.zoom_by(0.25),
            ZoomOut => self.zoom_by(-0.25),
            ResetView => self.reset_view(),
            Vertices => self.execute_for(command.document.unwrap(), EditorCommand::FinishEdit)?,
            CancelGesture => self.cancel_gesture(),
            _ => return Ok(CommandEffect::Host(command)),
        }
        Ok(CommandEffect::Done)
    }
}
