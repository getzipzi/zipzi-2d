//! Stable business commands. Platform input, dialogs and focus do not enter core.
use crate::scene::{MAX_ZOOM, MIN_ZOOM};
use crate::{DocumentId, Editor, EditorCommand};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandId {
    #[serde(rename = "workspace.modeling")]
    ModelingWorkspace,
    #[serde(rename = "workspace.animation")]
    AnimationWorkspace,
    #[serde(rename = "animation.play-pause")]
    PlayPause,
    #[serde(rename = "animation.rewind")]
    RewindAnimation,
    #[serde(rename = "panel.animation-info")]
    AnimationInfo,
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
    #[serde(rename = "parameter.create")]
    CreateParameter,
    #[serde(rename = "parameter.edit")]
    EditParameter,
    #[serde(rename = "parameter.delete")]
    DeleteParameter,
    #[serde(rename = "parameter-group.create")]
    CreateParameterGroup,
    #[serde(rename = "parameter-group.edit")]
    EditParameterGroup,
    #[serde(rename = "parameter-group.delete")]
    DeleteParameterGroup,
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
    #[serde(rename = "tool.select")]
    SelectTool,
    #[serde(rename = "tool.hand")]
    HandTool,
    #[serde(rename = "tool.glue")]
    GlueTool,
    #[serde(rename = "tool.generate-mesh")]
    GenerateMesh,
    #[serde(rename = "view.deformers")]
    ToggleDeformers,
    #[serde(rename = "view.ancestor-deformers")]
    ToggleAncestorDeformers,
    #[serde(rename = "tool.cancel")]
    CancelGesture,
    #[serde(rename = "panel.mesh-tree", alias = "panel.layers")]
    MeshTree,
    #[serde(rename = "panel.deformers")]
    Deformers,
    #[serde(rename = "panel.parameters")]
    Parameters,
    #[serde(rename = "panel.draw-order")]
    DrawOrder,
    #[serde(rename = "panel.relations")]
    Relations,
    #[serde(rename = "panel.keyforms")]
    Keyforms,
    #[serde(rename = "panel.blend-shapes")]
    BlendShapes,
    #[serde(rename = "panel.glue")]
    Glue,
    #[serde(rename = "panel.motions")]
    Motions,
    #[serde(rename = "panel.expressions")]
    Expressions,
    #[serde(rename = "panel.physics")]
    Physics,
    #[serde(rename = "panel.pose")]
    Pose,
    #[serde(rename = "panel.resources")]
    Resources,
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
    (
        ModelingWorkspace,
        "workspace.modeling",
        Workspace,
        false,
        []
    ),
    (
        AnimationWorkspace,
        "workspace.animation",
        Workspace,
        false,
        []
    ),
    (
        PlayPause,
        "workspace.play_pause",
        Document,
        false,
        ["space"]
    ),
    (RewindAnimation, "workspace.rewind", Document, false, []),
    (AnimationInfo, "workspace.info", Workspace, false, []),
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
    (CreateParameter, "parameter.new", Document, false, []),
    (EditParameter, "parameter.edit", Document, false, []),
    (DeleteParameter, "parameter.delete", Document, false, []),
    (
        CreateParameterGroup,
        "parameter.new_group",
        Document,
        false,
        []
    ),
    (
        EditParameterGroup,
        "parameter.edit_group",
        Document,
        false,
        []
    ),
    (
        DeleteParameterGroup,
        "parameter.delete_group",
        Document,
        false,
        []
    ),
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
    (SelectTool, "editor.tool_select", Canvas, false, ["v"]),
    (
        GenerateMesh,
        "editor.tool_generate_mesh",
        Canvas,
        false,
        ["shift-m"]
    ),
    (GlueTool, "editor.tool_glue", Canvas, false, ["g"]),
    (HandTool, "editor.tool_hand", Canvas, false, ["h"]),
    (ToggleDeformers, "editor.show_deformers", Canvas, false, []),
    (
        ToggleAncestorDeformers,
        "editor.show_ancestor_deformers",
        Canvas,
        false,
        []
    ),
    (
        CancelGesture,
        "menu.cancel_gesture",
        Canvas,
        false,
        ["escape"]
    ),
    (MeshTree, "editor.mesh_tree", Workspace, false, []),
    (Deformers, "editor.deformer_tree", Workspace, false, []),
    (Parameters, "editor.parameters", Workspace, false, []),
    (Properties, "editor.properties", Workspace, false, []),
    (DrawOrder, "editor.draw_order", Workspace, false, []),
    (Relations, "editor.relations", Workspace, false, []),
    (Keyforms, "editor.keyforms", Workspace, false, []),
    (BlendShapes, "editor.blend_shapes", Workspace, false, []),
    (Glue, "editor.glue", Workspace, false, []),
    (Motions, "editor.motions", Workspace, false, []),
    (Expressions, "editor.expressions", Workspace, false, []),
    (Physics, "editor.physics", Workspace, false, []),
    (Pose, "editor.pose", Workspace, false, []),
    (Resources, "editor.resources", Workspace, false, []),
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
        let modeling = self.workspace() == crate::WorkspaceKind::Modeling;
        match command.id {
            ModelingWorkspace | AnimationWorkspace => !self.exiting(),
            PlayPause => {
                !modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && self.animation_playable()
            }
            RewindAnimation => !modeling && target && !self.is_busy() && !self.exiting(),
            AnimationInfo => !modeling,
            Motions => true,
            New | Open | ImportModel3 | ImportPsd => !self.exiting(),
            Quit => true,
            Previous | Next => self.document_count() > 1,
            MeshTree | Deformers | Parameters | Properties | DrawOrder | Relations | Keyforms
            | BlendShapes | Glue | Expressions | Physics | Pose | Resources => modeling,
            ResetLayout | ResetShortcuts | ShowShortcuts => true,
            Close => target,
            Save | SaveAs => target && !self.is_busy(),
            GenerateMesh => {
                modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && !self.gesture_active()
                    && self
                        .document()
                        .is_some_and(|d| d.mesh_generation_target().is_ok())
            }
            CreateParameter | CreateParameterGroup => {
                modeling && target && !self.is_busy() && !self.exiting()
            }
            EditParameter | DeleteParameter | EditParameterGroup | DeleteParameterGroup => {
                modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && self.document().is_some_and(|doc| {
                        doc.context().inspected.as_ref().is_some_and(|object| {
                            object.kind
                                == if matches!(command.id, EditParameter | DeleteParameter) {
                                    crate::ObjectKind::Parameter
                                } else {
                                    crate::ObjectKind::ParameterGroup
                                }
                                && doc.object_index().get(object).is_some()
                                && doc.ensure_editable(std::slice::from_ref(object)).is_ok()
                        })
                    })
            }
            Undo => target && self.can_undo(),
            Redo => target && self.can_redo(),
            CommitProperty | CancelProperty => {
                target
                    && !self.is_busy()
                    && !self.gesture_active()
                    && self.document().is_some_and(|d| d.has_pending_edit())
            }
            ZoomIn => {
                target && !self.is_busy() && !self.gesture_active() && self.view().zoom < MAX_ZOOM
            }
            ZoomOut => {
                target && !self.is_busy() && !self.gesture_active() && self.view().zoom > MIN_ZOOM
            }
            ResetView => target && !self.is_busy() && !self.gesture_active(),
            CancelGesture => target && !self.is_busy(),
            HandTool => target && !self.is_busy(),
            _ => modeling && target && !self.is_busy(),
        }
    }
    pub fn dispatch(&mut self, command: Command) -> Result<CommandEffect> {
        ensure!(
            self.command_available(command),
            "Command is unavailable for this document"
        );
        use CommandId::*;
        match command.id {
            PlayPause => self.toggle_animation()?,
            RewindAnimation => self.rewind_animation()?,
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
            SelectTool | GlueTool | HandTool => {
                let tool = match command.id {
                    SelectTool => crate::ToolId::Select,
                    GlueTool => crate::ToolId::Glue,
                    _ => crate::ToolId::Hand,
                };
                self.execute_for(command.document.unwrap(), EditorCommand::SetTool(tool))?;
            }
            ToggleDeformers => {
                self.execute_for(command.document.unwrap(), EditorCommand::ToggleDeformers)?
            }
            ToggleAncestorDeformers => self.execute_for(
                command.document.unwrap(),
                EditorCommand::ToggleAncestorDeformers,
            )?,
            CancelGesture => self.cancel_gesture(),
            _ => return Ok(CommandEffect::Host(command)),
        }
        Ok(CommandEffect::Done)
    }
}
