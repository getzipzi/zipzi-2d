//! Workspace panel catalog shared by commands, menus, layout defaults and validation.
use crate::{CommandId, WorkspaceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelRegion {
    Center,
    Left,
    Right,
    Bottom,
}

pub struct PanelSpec {
    pub kind: PanelKind,
    pub name: &'static str,
    pub command: Option<CommandId>,
    pub workspace: Option<WorkspaceKind>,
    pub region: PanelRegion,
    pub group: usize,
    pub default_visible: bool,
}

macro_rules! panels {
    ($(($kind:ident, $command:expr, $workspace:expr, $region:ident, $group:expr, $visible:expr)),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum PanelKind { $($kind),* }
        pub const PANELS: &[PanelSpec] = &[$(PanelSpec {
            kind: PanelKind::$kind, name: stringify!($kind), command: $command,
            workspace: $workspace, region: PanelRegion::$region, group: $group,
            default_visible: $visible,
        }),*];
    };
}
use CommandId as C;
use WorkspaceKind::{Animation, Modeling};
panels! {
    (ModelingViewport, None, Some(Modeling), Center, 0, true),
    (AnimationViewport, None, Some(Animation), Center, 0, true),
    (PartsTree, Some(C::PartsTree), Some(Modeling), Left, 0, true),
    (DrawOrder, Some(C::DrawOrder), Some(Modeling), Left, 0, true),
    (Deformers, Some(C::Deformers), Some(Modeling), Left, 1, true),
    (Properties, Some(C::Properties), None, Right, 0, true),
    (Relations, Some(C::Relations), Some(Modeling), Right, 0, true),
    (Parameters, Some(C::Parameters), Some(Modeling), Right, 1, true),
    (Keyforms, Some(C::Keyforms), Some(Modeling), Bottom, 0, false),
    (BlendShapes, Some(C::BlendShapes), Some(Modeling), Right, 0, false),
    (Glue, Some(C::Glue), Some(Modeling), Right, 0, false),
    (Pose, Some(C::Pose), Some(Modeling), Right, 0, false),
    (Resources, Some(C::Resources), Some(Modeling), Right, 0, false),
    (AnimationMotions, Some(C::Motions), Some(Animation), Left, 0, true),
    (AnimationTimeline, Some(C::AnimationTimeline), Some(Animation), Bottom, 0, true),
    (AnimationExpressions, Some(C::Expressions), Some(Animation), Left, 1, true),
    (AnimationPhysics, Some(C::Physics), Some(Animation), Left, 1, true),
    (AnimationPose, Some(C::Pose), Some(Animation), Right, 1, false),
}
impl PanelSpec {
    pub fn available(&self, workspace: WorkspaceKind) -> bool {
        self.workspace.is_none_or(|kind| kind == workspace)
    }
}
impl PanelKind {
    pub fn primary_viewport(workspace: WorkspaceKind) -> Self {
        match workspace {
            WorkspaceKind::Modeling => Self::ModelingViewport,
            WorkspaceKind::Animation => Self::AnimationViewport,
        }
    }
    pub fn is_viewport(self) -> bool {
        matches!(self, Self::ModelingViewport | Self::AnimationViewport)
    }

    pub const DOCUMENT_PANELS: [Self; 7] = [
        Self::DrawOrder,
        Self::Relations,
        Self::Keyforms,
        Self::BlendShapes,
        Self::Glue,
        Self::Pose,
        Self::Resources,
    ];

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub fn label(self) -> &'static str {
        self.spec().command.map_or(
            match self {
                Self::ModelingViewport => "dock.modeling_viewport",
                Self::AnimationViewport => "dock.animation_viewport",
                _ => "dock.canvas",
            },
            |command| command.spec().label,
        )
    }

    pub fn spec(self) -> &'static PanelSpec {
        PANELS.iter().find(|spec| spec.kind == self).unwrap()
    }
    pub fn from_name(name: &str) -> Option<Self> {
        PANELS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| spec.kind)
    }
    pub fn for_command(command: CommandId, workspace: WorkspaceKind) -> Option<Self> {
        PANELS
            .iter()
            .find(|spec| spec.command == Some(command) && spec.available(workspace))
            .map(|spec| spec.kind)
    }
}
