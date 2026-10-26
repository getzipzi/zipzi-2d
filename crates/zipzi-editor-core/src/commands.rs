//! Stable business commands. Platform input, dialogs and focus do not enter core.
use crate::scene::{MAX_ZOOM, MIN_ZOOM};
use crate::{DocumentId, Editor, EditorCommand};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
mod timeline;

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
    #[serde(rename = "animation.clear-expression")]
    ClearExpression,
    #[serde(rename = "animation.reset")]
    ResetAnimation,
    #[serde(rename = "panel.animation-timeline")]
    AnimationTimeline,
    #[serde(rename = "timeline.delete")]
    TimelineDelete,
    #[serde(rename = "timeline.copy")]
    TimelineCopy,
    #[serde(rename = "timeline.paste")]
    TimelinePaste,
    #[serde(rename = "timeline.previous-frame")]
    TimelinePreviousFrame,
    #[serde(rename = "timeline.next-frame")]
    TimelineNextFrame,
    #[serde(rename = "timeline.previous-key")]
    TimelinePreviousKey,
    #[serde(rename = "timeline.next-key")]
    TimelineNextKey,
    #[serde(rename = "timeline.start")]
    TimelineStart,
    #[serde(rename = "timeline.end")]
    TimelineEnd,
    #[serde(rename = "file.new")]
    New,
    #[serde(rename = "file.open")]
    Open,
    #[serde(rename = "file.import-model3")]
    ImportModel3,
    #[serde(rename = "file.import-psd")]
    ImportPsd,
    #[serde(rename = "file.import-cubism")]
    ImportCubism,
    #[serde(rename = "file.export-runtime")]
    ExportRuntime,
    #[serde(rename = "file.export-cubism")]
    ExportCubism,
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
    #[serde(rename = "edit.deselect")]
    Deselect,
    #[serde(rename = "edit.select-all")]
    SelectAll,
    #[serde(rename = "palette.move-to")]
    PaletteMoveTo,
    #[serde(rename = "palette.move-up")]
    PaletteMoveUp,
    #[serde(rename = "palette.move-down")]
    PaletteMoveDown,
    #[serde(rename = "scene.create-part")]
    CreatePart,
    #[serde(rename = "physics-group.create")]
    CreatePhysicsGroup,
    #[serde(rename = "pose-group.create")]
    CreatePoseGroup,
    #[serde(rename = "pose-entry.create")]
    CreatePoseEntry,
    #[serde(rename = "pose-link.create")]
    CreatePoseLink,
    #[serde(rename = "motion.create")]
    CreateMotion,
    #[serde(rename = "motion-group.create")]
    CreateMotionGroup,
    #[serde(rename = "expression.create")]
    CreateExpression,
    #[serde(rename = "expression-entry.create")]
    CreateExpressionEntry,
    #[serde(rename = "scene.create-mesh")]
    CreateMesh,
    #[serde(rename = "scene.create-warp")]
    CreateWarp,
    #[serde(rename = "scene.create-rotation")]
    CreateRotation,
    #[serde(rename = "scene.create-offscreen")]
    CreateOffscreen,
    #[serde(rename = "deformation.create-mesh")]
    CreateDeformationMesh,
    #[serde(rename = "deformation.create-warp")]
    CreateDeformationWarp,
    #[serde(rename = "deformation.create-rotation")]
    CreateDeformationRotation,
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
    #[serde(rename = "view.fit")]
    FitView,
    #[serde(rename = "view.isolate-selection")]
    ToggleViewportIsolation,
    #[serde(rename = "tool.select")]
    SelectTool,
    #[serde(rename = "tool.lasso")]
    LassoTool,
    #[serde(rename = "tool.hand")]
    HandTool,
    #[serde(rename = "tool.glue")]
    GlueTool,
    #[serde(rename = "tool.generate-mesh")]
    GenerateMesh,
    #[serde(rename = "tool.manual-mesh")]
    ManualMesh,
    #[serde(rename = "tool.manual-select")]
    ManualSelect,
    #[serde(rename = "tool.manual-create")]
    ManualCreate,
    #[serde(rename = "tool.manual-delete")]
    ManualDelete,
    #[serde(rename = "tool.manual-reconnect")]
    ManualReconnect,
    #[serde(rename = "tool.manual-remove-selection")]
    ManualRemoveSelection,
    #[serde(rename = "tool.manual-preview-result")]
    ManualPreviewResult,
    #[serde(rename = "view.deformers")]
    ToggleDeformers,
    #[serde(rename = "view.ancestor-deformers")]
    ToggleAncestorDeformers,
    #[serde(rename = "tool.cancel")]
    CancelGesture,
    #[serde(rename = "panel.parts-tree", alias = "panel.layers")]
    PartsTree,
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
    Window,
    Document,
    Viewport,
    Timeline,
    Property,
    Palette,
}
impl CommandScope {
    pub fn overlaps(self, other: Self) -> bool {
        use CommandScope::*;
        self == other
            || matches!(
                (self, other),
                (Window, _)
                    | (_, Window)
                    | (Document, Viewport | Timeline | Palette)
                    | (Viewport | Timeline | Palette, Document)
            )
    }
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
    (ModelingWorkspace, "workspace.modeling", Window, false, []),
    (AnimationWorkspace, "workspace.animation", Window, false, []),
    (
        PlayPause,
        "workspace.play_pause",
        Document,
        false,
        ["space"]
    ),
    (RewindAnimation, "workspace.rewind", Document, false, []),
    (
        ClearExpression,
        "workspace.clear_expression",
        Document,
        false,
        []
    ),
    (
        ResetAnimation,
        "workspace.reset_preview",
        Document,
        false,
        []
    ),
    (AnimationTimeline, "timeline.title", Window, false, []),
    (
        TimelineDelete,
        "timeline.delete",
        Timeline,
        false,
        ["backspace", "delete"]
    ),
    (
        TimelineCopy,
        "timeline.copy",
        Timeline,
        false,
        ["secondary-c"]
    ),
    (
        TimelinePaste,
        "timeline.paste",
        Timeline,
        false,
        ["secondary-v"]
    ),
    (
        TimelinePreviousFrame,
        "timeline.previous_frame",
        Timeline,
        true,
        ["left"]
    ),
    (
        TimelineNextFrame,
        "timeline.next_frame",
        Timeline,
        true,
        ["right"]
    ),
    (
        TimelinePreviousKey,
        "timeline.previous_key",
        Timeline,
        true,
        ["secondary-left"]
    ),
    (
        TimelineNextKey,
        "timeline.next_key",
        Timeline,
        true,
        ["secondary-right"]
    ),
    (
        TimelineStart,
        "timeline.jump_start",
        Timeline,
        false,
        ["shift-left"]
    ),
    (
        TimelineEnd,
        "timeline.jump_end",
        Timeline,
        false,
        ["shift-right"]
    ),
    (New, "menu.new", Window, false, ["secondary-n"]),
    (Open, "menu.open", Window, false, ["secondary-o"]),
    (ImportModel3, "menu.import_model3", Window, false, []),
    (ImportPsd, "menu.import_psd", Window, false, []),
    (ImportCubism, "menu.import_cubism", Window, false, []),
    (ExportRuntime, "menu.export_runtime", Document, false, []),
    (ExportCubism, "menu.export_cubism", Document, false, []),
    (Save, "menu.save", Window, false, ["secondary-s"]),
    (SaveAs, "menu.save_as", Window, false, ["secondary-shift-s"]),
    (Close, "menu.close_document", Window, false, ["secondary-w"]),
    (Quit, "app.quit", Window, false, ["secondary-q"]),
    (
        Previous,
        "menu.previous_document",
        Window,
        false,
        ["ctrl-shift-tab"]
    ),
    (Next, "menu.next_document", Window, false, ["ctrl-tab"]),
    (Undo, "menu.undo", Document, false, ["secondary-z"]),
    (Redo, "menu.redo", Document, false, ["secondary-shift-z"]),
    (Deselect, "menu.deselect", Document, false, ["secondary-d"]),
    (
        SelectAll,
        "menu.select_all",
        Viewport,
        false,
        ["secondary-a"]
    ),
    (
        PaletteMoveUp,
        "authoring.move_up",
        Palette,
        true,
        ["alt-up"]
    ),
    (
        PaletteMoveDown,
        "authoring.move_down",
        Palette,
        true,
        ["alt-down"]
    ),
    (PaletteMoveTo, "authoring.move_to", Palette, false, []),
    (CreatePart, "authoring.new_part", Document, false, []),
    (CreatePhysicsGroup, "physics.add_group", Document, false, []),
    (
        CreatePoseGroup,
        "authoring.new_pose_group",
        Document,
        false,
        []
    ),
    (
        CreatePoseEntry,
        "authoring.new_pose_entry",
        Document,
        false,
        []
    ),
    (
        CreatePoseLink,
        "authoring.new_pose_link",
        Document,
        false,
        []
    ),
    (CreateMotion, "timeline.new_motion", Document, false, []),
    (
        CreateMotionGroup,
        "authoring.new_motion_group",
        Document,
        false,
        []
    ),
    (
        CreateExpression,
        "authoring.new_expression",
        Document,
        false,
        []
    ),
    (
        CreateExpressionEntry,
        "authoring.new_expression_entry",
        Document,
        false,
        []
    ),
    (CreateMesh, "authoring.new_mesh", Document, false, []),
    (CreateWarp, "authoring.new_warp", Document, false, []),
    (
        CreateRotation,
        "authoring.new_rotation",
        Document,
        false,
        []
    ),
    (
        CreateOffscreen,
        "authoring.new_offscreen",
        Document,
        false,
        []
    ),
    (
        CreateDeformationMesh,
        "authoring.new_deformation_mesh",
        Document,
        false,
        []
    ),
    (
        CreateDeformationWarp,
        "authoring.new_deformation_warp",
        Document,
        false,
        []
    ),
    (
        CreateDeformationRotation,
        "authoring.new_deformation_rotation",
        Document,
        false,
        []
    ),
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
    (
        ZoomIn,
        "menu.zoom_in",
        Viewport,
        true,
        ["secondary-=", "secondary-+"]
    ),
    (ZoomOut, "menu.zoom_out", Viewport, true, ["secondary--"]),
    (
        ResetView,
        "menu.reset_view",
        Viewport,
        false,
        ["secondary-0"]
    ),
    (FitView, "menu.fit_view", Viewport, false, []),
    (
        ToggleViewportIsolation,
        "dock.isolate_selection",
        Viewport,
        false,
        ["/"]
    ),
    (SelectTool, "editor.tool_select", Viewport, false, ["v"]),
    (LassoTool, "editor.tool_lasso", Viewport, false, ["l"]),
    (
        GenerateMesh,
        "editor.tool_generate_mesh",
        Viewport,
        false,
        ["shift-m"]
    ),
    (ManualMesh, "editor.tool_manual_mesh", Viewport, false, []),
    (
        ManualSelect,
        "editor.tool_manual_select",
        Viewport,
        false,
        []
    ),
    (
        ManualCreate,
        "editor.tool_manual_create",
        Viewport,
        false,
        []
    ),
    (
        ManualDelete,
        "editor.tool_manual_delete",
        Viewport,
        false,
        []
    ),
    (
        ManualReconnect,
        "editor.tool_manual_reconnect",
        Viewport,
        false,
        []
    ),
    (
        ManualRemoveSelection,
        "editor.tool_manual_remove_selection",
        Viewport,
        false,
        ["backspace", "delete"]
    ),
    (
        ManualPreviewResult,
        "editor.tool_manual_preview_result",
        Viewport,
        false,
        []
    ),
    (GlueTool, "editor.tool_glue", Viewport, false, ["g"]),
    (HandTool, "editor.tool_hand", Viewport, false, ["h"]),
    (
        ToggleDeformers,
        "editor.show_deformers",
        Viewport,
        false,
        []
    ),
    (
        ToggleAncestorDeformers,
        "editor.show_ancestor_deformers",
        Viewport,
        false,
        []
    ),
    (
        CancelGesture,
        "menu.cancel_gesture",
        Viewport,
        false,
        ["escape"]
    ),
    (PartsTree, "editor.part_tree", Window, false, []),
    (Deformers, "editor.deformer_tree", Window, false, []),
    (Parameters, "editor.parameters", Window, false, []),
    (Properties, "editor.properties", Window, false, []),
    (DrawOrder, "editor.draw_order", Window, false, []),
    (Relations, "editor.relations", Window, false, []),
    (Keyforms, "editor.keyforms", Window, false, []),
    (BlendShapes, "editor.blend_shapes", Window, false, []),
    (Glue, "editor.glue", Window, false, []),
    (Motions, "editor.motions", Window, false, []),
    (Expressions, "editor.expressions", Window, false, []),
    (Physics, "editor.physics", Window, false, []),
    (Pose, "editor.pose", Window, false, []),
    (Resources, "editor.resources", Window, false, []),
    (ResetLayout, "menu.reset_layout", Window, false, []),
    (ResetShortcuts, "menu.reset_shortcuts", Window, false, []),
    (ShowShortcuts, "menu.shortcuts", Window, false, []),
];
impl CommandId {
    pub fn scene_creation_type(self) -> Option<crate::SceneObjectType> {
        use crate::SceneObjectType as T;
        Some(match self {
            Self::CreatePart => T::Part,
            Self::CreateMesh | Self::CreateDeformationMesh => T::Mesh,
            Self::CreateWarp | Self::CreateDeformationWarp => T::Warp,
            Self::CreateRotation | Self::CreateDeformationRotation => T::Rotation,
            Self::CreateOffscreen => T::Offscreen,
            _ => return None,
        })
    }
    pub fn scene_creation_projection(self) -> Option<crate::PaletteProjection> {
        self.scene_creation_type()?;
        Some(
            if matches!(
                self,
                Self::CreateDeformationMesh
                    | Self::CreateDeformationWarp
                    | Self::CreateDeformationRotation
            ) {
                crate::PaletteProjection::Deformation
            } else {
                crate::PaletteProjection::Organization
            },
        )
    }
    pub fn spec(self) -> &'static CommandSpec {
        COMMANDS.iter().find(|s| s.id == self).unwrap()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandSource {
    #[default]
    Direct,
    Shortcut,
    Toolbar,
    ContextMenu,
    NativeMenu,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineCommandTarget {
    pub panel: Option<crate::TimelineBinding>,
    pub document: DocumentId,
    pub context_generation: u64,
    pub timeline_generation: u64,
    pub model_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTarget {
    Unavailable,
    Window,
    Document(DocumentId),
    Viewport(crate::ViewportBinding),
    Timeline(TimelineCommandTarget),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    pub target: CommandTarget,
    pub source: CommandSource,
}
impl Command {
    pub fn document(self) -> Option<DocumentId> {
        match self.target {
            CommandTarget::Document(document) => Some(document),
            CommandTarget::Viewport(binding) => Some(binding.document),
            CommandTarget::Timeline(target) => Some(target.document),
            _ => None,
        }
    }
    pub fn with_source(mut self, source: CommandSource) -> Self {
        self.source = source;
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandDisabledReason {
    InvalidDocument,
    InvalidTarget,
    Closing,
    Busy,
    UnavailableInContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub checked: bool,
    pub disabled_reason: Option<CommandDisabledReason>,
}

#[derive(Clone, Copy, Debug)]
pub enum CommandEffect {
    Done,
    Host(Command),
}
impl Editor {
    pub fn command(&self, id: CommandId) -> Command {
        let target = match id.spec().scope {
            CommandScope::Window
                if !matches!(id, CommandId::Save | CommandId::SaveAs | CommandId::Close) =>
            {
                CommandTarget::Window
            }
            CommandScope::Window
            | CommandScope::Document
            | CommandScope::Property
            | CommandScope::Palette => self
                .active_id()
                .map(CommandTarget::Document)
                .unwrap_or(CommandTarget::Unavailable),
            CommandScope::Viewport => self
                .primary_viewport_binding()
                .map(CommandTarget::Viewport)
                .unwrap_or(CommandTarget::Unavailable),
            CommandScope::Timeline => self
                .timeline_command_target()
                .map(CommandTarget::Timeline)
                .unwrap_or(CommandTarget::Unavailable),
        };
        Command {
            id,
            target,
            source: CommandSource::Direct,
        }
    }
    pub fn command_for_viewport(
        &self,
        id: CommandId,
        binding: crate::ViewportBinding,
        source: CommandSource,
    ) -> Command {
        Command {
            id,
            target: CommandTarget::Viewport(binding),
            source,
        }
    }
    pub fn timeline_command_target(&self) -> Option<TimelineCommandTarget> {
        let session = self.active_session()?;
        (self.editing_context() == Some(crate::EditingContext::Animation)
            && session.animation.timeline.current().is_some())
        .then(|| TimelineCommandTarget {
            panel: None,
            document: self.active_id().unwrap(),
            context_generation: self.context_generation,
            timeline_generation: session.animation.timeline.binding_generation,
            model_generation: session.document().model_generation(),
        })
    }
    pub fn timeline_command_target_valid(&self, target: TimelineCommandTarget) -> bool {
        target
            .panel
            .is_none_or(|binding| self.timeline_binding_valid(binding))
            && self.timeline_command_target()
                == Some(TimelineCommandTarget {
                    panel: None,
                    ..target
                })
            && !self.is_busy()
            && !self.exiting()
    }
    pub fn command_for_timeline(
        &self,
        id: CommandId,
        binding: crate::TimelineBinding,
        source: CommandSource,
    ) -> Command {
        Command {
            id,
            source,
            target: self
                .timeline_command_target()
                .filter(|_| self.timeline_binding_valid(binding))
                .map(|target| {
                    CommandTarget::Timeline(TimelineCommandTarget {
                        panel: Some(binding),
                        ..target
                    })
                })
                .unwrap_or(CommandTarget::Unavailable),
        }
    }
    fn command_target_valid(&self, command: Command) -> bool {
        match (command.id.spec().scope, command.target) {
            (CommandScope::Viewport, CommandTarget::Viewport(binding)) => {
                self.viewport_binding_valid(binding)
            }
            (CommandScope::Timeline, CommandTarget::Timeline(target)) => {
                self.timeline_command_target_valid(target)
            }
            (CommandScope::Viewport | CommandScope::Timeline, _) => false,
            (_, CommandTarget::Window) => {
                command.id.spec().scope == CommandScope::Window
                    && !matches!(
                        command.id,
                        CommandId::Save | CommandId::SaveAs | CommandId::Close
                    )
            }
            (scope, CommandTarget::Document(document)) => {
                self.active_id() == Some(document)
                    && (matches!(
                        scope,
                        CommandScope::Document | CommandScope::Property | CommandScope::Palette
                    ) || matches!(
                        command.id,
                        CommandId::Save | CommandId::SaveAs | CommandId::Close
                    ))
            }
            _ => false,
        }
    }
    /// Shared UI state. Dispatch always queries it again before executing.
    pub fn command_state(&self, command: Command) -> CommandState {
        self.command_state_with_availability(command, || {
            self.authoring_command_capability(command.id)
                .is_ok_and(|state| state == zipzi_sdk::CapabilityState::Available)
        })
    }
    /// Reuse a capability queried during this UI refresh. Target and lifecycle
    /// gates still run; dispatch always queries fresh state through command_state.
    pub fn command_state_with_authoring_capability(
        &self,
        command: Command,
        capability: &Result<zipzi_sdk::CapabilityState>,
    ) -> CommandState {
        self.command_state_with_availability(command, || {
            capability
                .as_ref()
                .is_ok_and(|state| *state == zipzi_sdk::CapabilityState::Available)
        })
    }
    fn command_state_with_availability(
        &self,
        command: Command,
        authoring_available: impl FnOnce() -> bool,
    ) -> CommandState {
        use CommandId::*;
        let animation_navigation = matches!(command.target,
            CommandTarget::Viewport(binding) if binding.kind == crate::ViewportKind::Animation);
        let checked = match command.id {
            ModelingWorkspace => self.workspace() == crate::WorkspaceKind::Modeling,
            AnimationWorkspace => self.workspace() == crate::WorkspaceKind::Animation,
            PlayPause => self
                .animation()
                .is_some_and(|animation| animation.playing && !animation.physics_preview.manual),
            SelectTool => {
                !animation_navigation
                    && !self.manual_active()
                    && self.active_tool() == crate::ToolId::Select
            }
            LassoTool => {
                !animation_navigation
                    && !self.manual_active()
                    && self.active_tool() == crate::ToolId::Lasso
            }
            GlueTool => {
                !animation_navigation
                    && !self.manual_active()
                    && self.active_tool() == crate::ToolId::Glue
            }
            HandTool => animation_navigation || self.active_tool() == crate::ToolId::Hand,
            ManualMesh => self.manual_active(),
            ManualSelect => self.manual_tool() == Some(crate::ManualTool::Select),
            ManualCreate => self.manual_tool() == Some(crate::ManualTool::Create),
            ManualDelete => self.manual_tool() == Some(crate::ManualTool::Delete),
            ManualPreviewResult => self
                .active_session()
                .is_some_and(|s| s.modeling.tools.manual_preview_result()),
            ToggleViewportIsolation => matches!(command.target,
                CommandTarget::Viewport(binding) if self.viewport_isolated(binding)),
            ToggleDeformers => self.deformer_visibility().all,
            ToggleAncestorDeformers => self.deformer_visibility().ancestors,
            _ => false,
        };
        let disabled_reason = if self.command_enabled(command, authoring_available) {
            None
        } else if command.id.spec().scope != CommandScope::Window
            && (command.document().is_none() || command.document() != self.active_id())
        {
            Some(CommandDisabledReason::InvalidDocument)
        } else if self.exiting() {
            Some(CommandDisabledReason::Closing)
        } else if self.is_busy() {
            Some(CommandDisabledReason::Busy)
        } else if !self.command_target_valid(command) {
            Some(CommandDisabledReason::InvalidTarget)
        } else {
            Some(CommandDisabledReason::UnavailableInContext)
        };
        CommandState {
            enabled: disabled_reason.is_none(),
            checked,
            disabled_reason,
        }
    }
    pub fn command_available(&self, command: Command) -> bool {
        self.command_state(command).enabled
    }
    fn command_enabled(
        &self,
        command: Command,
        authoring_available: impl FnOnce() -> bool,
    ) -> bool {
        use CommandId::*;
        if !self.command_target_valid(command) {
            return false;
        }
        if crate::PANELS
            .iter()
            .any(|spec| spec.command == Some(command.id))
        {
            return !self.exiting()
                && crate::PanelKind::for_command(command.id, self.workspace()).is_some();
        }
        let target = command.document().is_some() && self.command_target_valid(command);
        if matches!(
            command.id.spec().scope,
            CommandScope::Viewport | CommandScope::Timeline
        ) && !target
        {
            return false;
        }
        let modeling = match command.target {
            CommandTarget::Viewport(binding) => {
                binding.kind == crate::ViewportKind::Modeling
                    && self.editing_context() == Some(crate::EditingContext::Modeling)
            }
            _ => self.editing_context() == Some(crate::EditingContext::Modeling),
        };
        let animation_navigation = matches!(command.target,
            CommandTarget::Viewport(binding) if binding.kind == crate::ViewportKind::Animation);
        let view = match command.target {
            CommandTarget::Viewport(binding) => self.viewport_view(binding).unwrap_or_default(),
            _ => self.view(),
        };
        let metrics = match command.target {
            CommandTarget::Viewport(binding) => {
                self.viewport_metrics(binding.viewport).unwrap_or_default()
            }
            _ => self.canvas_metrics(),
        };
        if command.id.spec().scope == CommandScope::Timeline {
            return target && self.timeline_command_enabled(command.id);
        }
        match command.id {
            ModelingWorkspace | AnimationWorkspace => !self.exiting(),
            PlayPause => {
                !modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && self.animation_playable()
            }
            RewindAnimation | ResetAnimation => {
                !modeling && target && !self.is_busy() && !self.exiting()
            }
            ClearExpression => {
                !modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && self
                        .animation()
                        .is_some_and(|a| a.expression.is_some() || !a.expressions.is_empty())
            }

            New | Open | ImportModel3 | ImportPsd | ImportCubism => !self.exiting(),
            Quit => true,
            Previous | Next => self.document_count() > 1,
            ResetLayout | ResetShortcuts | ShowShortcuts => true,
            Close => target,
            Save | SaveAs => target && !self.is_busy(),
            ExportRuntime | ExportCubism => target && !self.is_busy() && !self.exiting(),
            GenerateMesh => {
                modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && !self.gesture_active()
                    && (self.manual_active()
                        || self
                            .document()
                            .is_some_and(|d| d.mesh_generation_target().is_ok()))
            }
            ManualMesh => {
                modeling
                    && target
                    && !self.is_busy()
                    && !self.exiting()
                    && (self.manual_active()
                        || self
                            .document()
                            .is_some_and(|d| matches!(d.context().selections.as_slice(), [object] if object.kind == crate::ObjectKind::Mesh)))
            }
            ManualSelect
            | ManualCreate
            | ManualDelete
            | ManualReconnect
            | ManualRemoveSelection
            | ManualPreviewResult => modeling && target && !self.is_busy() && self.manual_active(),
            PaletteMoveUp | PaletteMoveDown | PaletteMoveTo => {
                target && !self.is_busy() && !self.exiting()
            }
            CreatePhysicsGroup
            | CreatePoseGroup
            | CreatePoseEntry
            | CreatePoseLink
            | CreateMotion
            | CreateMotionGroup
            | CreateExpression
            | CreateExpressionEntry => {
                !modeling && target && !self.is_busy() && !self.exiting() && authoring_available()
            }
            CreateParameter | CreateParameterGroup => {
                modeling && target && !self.is_busy() && !self.exiting() && authoring_available()
            }
            CreatePart
            | CreateMesh
            | CreateWarp
            | CreateRotation
            | CreateOffscreen
            | CreateDeformationMesh
            | CreateDeformationWarp
            | CreateDeformationRotation => {
                modeling && target && !self.is_busy() && !self.exiting() && authoring_available()
            }
            EditParameter | DeleteParameter | EditParameterGroup | DeleteParameterGroup => {
                modeling && target && !self.is_busy() && !self.exiting() && authoring_available()
            }
            Undo => target && (self.can_undo() || (self.manual_active() && self.gesture_active())),
            Redo => target && (self.can_redo() || (self.manual_active() && self.gesture_active())),
            Deselect => {
                modeling
                    && target
                    && !self.is_busy()
                    && !self.gesture_active()
                    && self
                        .document()
                        .is_some_and(|d| !d.context().selections.is_empty())
            }
            SelectAll => {
                modeling
                    && !self.is_busy()
                    && !self.exiting()
                    && !self.gesture_active()
                    && matches!(command.target, CommandTarget::Viewport(binding)
                        if self.validate_modeling_press(binding).is_ok())
            }
            CommitProperty | CancelProperty => {
                target
                    && !self.is_busy()
                    && !self.gesture_active()
                    && self.document().is_some_and(|d| d.has_pending_edit())
            }
            ZoomIn => target && !self.is_busy() && !self.gesture_active() && view.zoom < MAX_ZOOM,
            ZoomOut => target && !self.is_busy() && !self.gesture_active() && view.zoom > MIN_ZOOM,
            ResetView => target && !self.is_busy() && !self.gesture_active(),
            FitView => target && !self.is_busy() && !self.gesture_active() && metrics.visible(),
            ToggleViewportIsolation => {
                modeling
                    && !self.is_busy()
                    && matches!(command.target,
                CommandTarget::Viewport(binding) if self.can_toggle_viewport_isolation(binding))
            }
            CancelGesture => target && !self.is_busy(),
            LassoTool | GlueTool => modeling && target && !self.is_busy() && !self.manual_active(),
            HandTool => target && !self.is_busy() && (modeling || animation_navigation),
            _ => modeling && target && !self.is_busy(),
        }
    }
    pub fn dispatch(&mut self, command: Command) -> Result<CommandEffect> {
        ensure!(
            self.command_available(command),
            "Command is unavailable: {:?}",
            self.command_state(command).disabled_reason
        );
        use CommandId::*;
        let viewport = match command.target {
            CommandTarget::Viewport(binding) => Some(binding),
            _ => None,
        };
        match command.id {
            id if id.spec().scope == CommandScope::Timeline => {
                self.dispatch_timeline_command(command.document().unwrap(), id)?;
            }
            PlayPause => self.toggle_animation()?,
            RewindAnimation => self.rewind_animation()?,
            ClearExpression => self.clear_expression()?,
            ResetAnimation => self.reset_animation()?,
            New => {
                self.new_document()?;
            }
            Previous | Next => {
                self.activate(self.adjacent(command.id == Next).unwrap())?;
            }
            Undo | Redo if self.manual_active() && self.gesture_active() => {
                self.cancel_gesture();
            }
            Undo => self.execute_for(command.document().unwrap(), EditorCommand::Undo)?,
            Redo => self.execute_for(command.document().unwrap(), EditorCommand::Redo)?,
            Deselect => {
                self.execute_for(
                    command.document().unwrap(),
                    EditorCommand::SelectObjects(Vec::new()),
                )?;
            }
            SelectAll => {
                if self.manual_active() {
                    let session = self.active_session_mut().unwrap();
                    session.modeling.tools.manual_select_all(&session.document);
                    return Ok(CommandEffect::Done);
                }
                let frame = self
                    .displayed_modeling_frame(viewport.unwrap())
                    .unwrap()
                    .clone();
                let session = self.active_session_mut().unwrap();
                session
                    .modeling
                    .tools
                    .select_all(&mut session.document, &frame)?;
            }
            CommitProperty => {
                self.execute_for(command.document().unwrap(), EditorCommand::FinishEdit)?
            }
            CancelProperty => {
                self.execute_for(command.document().unwrap(), EditorCommand::CancelEdit)?
            }
            ZoomIn => self.zoom_viewport(viewport.unwrap(), 0.25, None)?,
            ZoomOut => self.zoom_viewport(viewport.unwrap(), -0.25, None)?,
            ResetView => self.reset_viewport(viewport.unwrap())?,
            FitView => self.fit_viewport(viewport.unwrap())?,
            ToggleViewportIsolation => self.toggle_viewport_isolation(viewport.unwrap())?,
            HandTool
                if viewport
                    .is_some_and(|binding| binding.kind == crate::ViewportKind::Animation) =>
            {
                // Animation exposes navigation without changing the document's Modeling tool.
            }
            SelectTool if self.manual_active() => {
                let session = self.active_session_mut().unwrap();
                session
                    .modeling
                    .tools
                    .set_manual_tool(crate::ManualTool::Select, &mut session.document);
                session.invalidate_scene();
            }
            SelectTool | LassoTool | GlueTool | HandTool => {
                let tool = match command.id {
                    SelectTool => crate::ToolId::Select,
                    LassoTool => crate::ToolId::Lasso,
                    GlueTool => crate::ToolId::Glue,
                    _ => crate::ToolId::Hand,
                };
                self.execute_for(command.document().unwrap(), EditorCommand::SetTool(tool))?;
            }
            ManualMesh if self.manual_active() => self.exit_manual_mesh(),
            ManualMesh => return Ok(CommandEffect::Host(command)),
            ManualSelect | ManualCreate | ManualDelete => {
                let tool = match command.id {
                    ManualCreate => crate::ManualTool::Create,
                    ManualDelete => crate::ManualTool::Delete,
                    _ => crate::ManualTool::Select,
                };
                let session = self.active_session_mut().unwrap();
                session
                    .modeling
                    .tools
                    .set_manual_tool(tool, &mut session.document);
                session.invalidate_scene();
            }
            ManualRemoveSelection => {
                let session = self.active_session_mut().unwrap();
                if session
                    .modeling
                    .tools
                    .manual_delete_selection(&mut session.document)?
                {
                    session.invalidate_scene();
                }
            }
            ManualReconnect => {
                let session = self.active_session_mut().unwrap();
                let target = session.modeling.tools.manual_target().unwrap().clone();
                let changed = session
                    .document
                    .apply_manual_mesh_once(&target, zipzi_sdk::ManualMeshOperation::Reconnect)?;
                session
                    .modeling
                    .tools
                    .refresh_manual_source(&session.document)?;
                if changed {
                    session.invalidate_scene();
                }
            }
            ManualPreviewResult => {
                let session = self.active_session_mut().unwrap();
                session.modeling.tools.toggle_manual_preview_result();
                session.invalidate_scene();
            }
            ToggleDeformers => {
                self.execute_for(command.document().unwrap(), EditorCommand::ToggleDeformers)?
            }
            ToggleAncestorDeformers => self.execute_for(
                command.document().unwrap(),
                EditorCommand::ToggleAncestorDeformers,
            )?,
            CancelGesture => {
                let exit_manual = self.manual_active() && !self.gesture_active();
                let binding = viewport.unwrap();
                if let Some(token) = self
                    .viewport_interaction
                    .filter(|token| token.binding == binding)
                {
                    self.cancel_viewport_interaction(token);
                }
                self.cancel_gesture_for(binding);
                if exit_manual {
                    self.exit_manual_mesh();
                }
            }
            _ => return Ok(CommandEffect::Host(command)),
        }
        Ok(CommandEffect::Done)
    }
}
