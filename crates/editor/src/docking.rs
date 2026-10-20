use super::*;
use editor_core::{PANELS, PanelKind, PanelRegion, WorkspaceKind};
use gpui_kit::component::dock::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{rc::Rc, sync::Arc};

pub(super) mod add_panel;
pub(crate) mod header;
mod layout;
mod persistence;
mod registry;
mod skin;
mod viewports;
mod workspace;
pub use layout::{LayoutConfig, validate_layout};
use layout::{layout_file, layout_version, timeline_height};
use persistence::LayoutWriter;

type PanelViews = HashMap<PanelKind, Arc<dyn BasePanelView>>;

const REGIONS: [DockPlacement; 4] = [
    DockPlacement::Center,
    DockPlacement::Left,
    DockPlacement::Right,
    DockPlacement::Bottom,
];

pub struct DockWorkspace {
    pub(crate) input_regions: HashMap<PanelKind, crate::input_routing::InputRegion>,
    pub area: Entity<DockArea>,
    viewports: viewports::ViewportPanels,
    pub timeline: Entity<crate::panels::timeline::Timeline>,
    pub timeline_id: editor_core::TimelineId,
    pub animation_motions: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_expressions: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_inspectors:
        HashMap<PanelKind, Entity<crate::panels::animation_panel::AnimationPanel>>,
    pub properties: Entity<crate::panels::PropertiesPanel>,
    pub mesh_tree: Entity<crate::panels::MeshTreePanel>,
    pub deformer_tree: Entity<crate::panels::DeformerTreePanel>,
    pub parameters: Entity<crate::panels::ParametersPanel>,
    pub document_panels:
        std::collections::HashMap<editor_core::PanelKind, Entity<crate::panels::DocumentPanel>>,
    panels: Rc<PanelViews>,
    pub last_valid: DockAreaState,
    pending_layout_measurement: bool,
    workspace: WorkspaceKind,
    layouts: HashMap<WorkspaceKind, DockAreaState>,
    layout_epoch: u64,
    writer: LayoutWriter,
    errors: Option<async_channel::Receiver<String>>,
}

impl DockWorkspace {
    pub fn active_viewport(&self) -> &Entity<crate::viewports::ViewportSurface> {
        &self.viewports.active(self.workspace).surface
    }

    pub(super) fn activate_viewport(&mut self, id: EntityId) {
        self.viewports.activate(id);
    }

    pub(super) fn viewport_surfaces(
        &self,
    ) -> impl Iterator<Item = &Entity<crate::viewports::ViewportSurface>> {
        self.viewports.iter().map(|entry| &entry.surface)
    }

    pub(super) fn canvas_node(&self, cx: &App) -> Option<NodeId> {
        let id = self.viewport_panel_id();
        let area = self.area.read(cx);
        REGIONS
            .into_iter()
            .find_map(|placement| area.layout(placement)?.find_panel_node(id))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn viewport_panel(&self) -> Arc<dyn BasePanelView> {
        self.viewports.active(self.workspace).panel.clone()
    }
    pub(crate) fn viewport_panel_id(&self) -> PanelId {
        self.viewports.panel_id(self.workspace)
    }
    pub(crate) fn visible_viewports(&self, cx: &App) -> Vec<editor_core::ViewportId> {
        self.viewports
            .iter()
            .filter(|entry| self.panel_displayed(entry.panel.panel_id(cx), cx))
            .map(|entry| entry.id)
            .collect()
    }

    pub fn contains(&self, kind: PanelKind, cx: &App) -> bool {
        self.area
            .read(cx)
            .panel(self.panels[&kind].panel_id(cx))
            .is_some()
    }

    pub(crate) fn displayed(&self, kind: PanelKind, cx: &App) -> bool {
        let id = self.panels[&kind].panel_id(cx);
        self.panel_displayed(id, cx)
    }
    fn panel_displayed(&self, id: PanelId, cx: &App) -> bool {
        let area = self.area.read(cx);
        let Some(region) = panel_region(area, id) else {
            return false;
        };
        if region != DockPlacement::Center && !area.is_dock_open(region) {
            return false;
        }
        let tree = area.layout(region).unwrap();
        let node = tree.find_panel_node(id).unwrap();
        if area.is_zoomed() && area.zoomed_group() != Some(node) {
            return false;
        }
        matches!(tree.find_node(node).unwrap().kind(), PaneRef::Tabs { panels, active_ix }
            if panels.get(active_ix) == Some(&id))
    }

    pub fn toggle(&self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        if !kind.spec().available(self.workspace) || kind.is_viewport() {
            return;
        }
        let panel = self.panels[&kind].clone();
        let id = panel.panel_id(cx);
        if self.contains(kind, cx) {
            self.area.update(cx, |area, cx| {
                let region = panel_region(area, id).unwrap();
                if region != DockPlacement::Center && !area.is_dock_open(region) {
                    area.toggle_dock(region, window, cx);
                    area.select_panel(id, window, cx);
                } else {
                    match kind {
                        PanelKind::MeshTree => {
                            area.remove_panel(self.mesh_tree.clone(), window, cx)
                        }
                        PanelKind::Deformers => {
                            area.remove_panel(self.deformer_tree.clone(), window, cx)
                        }
                        PanelKind::Properties => {
                            area.remove_panel(self.properties.clone(), window, cx)
                        }
                        PanelKind::Parameters => {
                            area.remove_panel(self.parameters.clone(), window, cx)
                        }
                        PanelKind::AnimationMotions => {
                            area.remove_panel(self.animation_motions.clone(), window, cx)
                        }
                        PanelKind::AnimationExpressions => {
                            area.remove_panel(self.animation_expressions.clone(), window, cx)
                        }
                        PanelKind::AnimationTimeline => {
                            area.remove_panel(self.timeline.clone(), window, cx)
                        }
                        kind if self.animation_inspectors.contains_key(&kind) => {
                            area.remove_panel(self.animation_inspectors[&kind].clone(), window, cx);
                        }
                        _ => {
                            if let Some((_, panel)) = self
                                .document_panels
                                .iter()
                                .find(|(document, _)| document.name() == kind.spec().name)
                            {
                                area.remove_panel(panel.clone(), window, cx);
                            }
                        }
                    }
                    prune_empty_docks(area, window, cx);
                }
            });
        } else {
            self.show(kind, None, window, cx);
        }
    }

    fn default_target(&self, kind: PanelKind, cx: &App) -> InsertTarget {
        let area = self.area.read(cx);
        let spec = kind.spec();
        for peer in PANELS.iter().filter(|peer| {
            peer.kind != kind
                && peer.available(self.workspace)
                && peer.region == spec.region
                && peer.group == spec.group
        }) {
            let id = self.panels[&peer.kind].panel_id(cx);
            if let Some(node) = REGIONS
                .iter()
                .find_map(|region| area.layout(*region)?.find_panel_node(id))
            {
                return InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                };
            }
        }
        let canvas = self.viewport_panel_id();
        let node = REGIONS
            .into_iter()
            .find_map(|region| area.layout(region)?.find_panel_node(canvas))
            .expect("workspace contains a canvas");
        InsertTarget::Split {
            node,
            placement: match spec.region {
                PanelRegion::Left => gpui_kit::base::Placement::Left,
                PanelRegion::Bottom => gpui_kit::base::Placement::Bottom,
                _ => gpui_kit::base::Placement::Right,
            },
            size: Some(px(240.)),
        }
    }

    /// Add once at an explicit live target, or reveal an existing singleton.
    pub(crate) fn show(
        &self,
        kind: PanelKind,
        target: Option<InsertTarget>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if kind.is_viewport() || !kind.spec().available(self.workspace) {
            return;
        }
        let panel = self.panels[&kind].clone();
        let id = panel.panel_id(cx);
        if kind == PanelKind::AnimationTimeline && target.is_none() && !self.contains(kind, cx) {
            self.area.update(cx, |area, cx| {
                let new_dock = !area.has_dock(DockPlacement::Bottom);
                area.add_panel_view(panel, DockPlacement::Bottom, None, window, cx);
                if new_dock {
                    area.set_dock_size(DockPlacement::Bottom, timeline_height(window), window, cx);
                }
                if !area.is_dock_open(DockPlacement::Bottom) {
                    area.toggle_dock(DockPlacement::Bottom, window, cx);
                }
                area.select_panel(id, window, cx);
            });
            return;
        }
        let target = target.unwrap_or_else(|| self.default_target(kind, cx));
        let node = match target {
            InsertTarget::Tabs { node, .. } | InsertTarget::Split { node, .. } => node,
        };
        let area = self.area.read(cx);
        let Some(destination) = REGIONS.into_iter().find(|region| {
            area.layout(*region)
                .is_some_and(|tree| tree.find_node(node).is_some())
        }) else {
            return;
        };
        if matches!(target, InsertTarget::Tabs { .. }) {
            let tree = area.layout(destination).unwrap();
            let Some(PaneRef::Tabs { .. }) = tree.find_node(node).map(|node| node.kind()) else {
                return;
            };
        }
        self.area.update(cx, |area, cx| {
            area.set_zoomed_out(window, cx);
            if area.panel(id).is_none() {
                // Register the view in a side region before moving it to the
                // explicit target. The final tree is observed as one UI update.
                let staging = REGIONS
                    .into_iter()
                    .skip(1)
                    .find(|region| !area.has_dock(*region))
                    .unwrap_or(DockPlacement::Bottom);
                fn first_active(node: &PaneNode) -> Option<PanelId> {
                    match node.kind() {
                        PaneRef::Tabs { panels, active_ix } => panels.get(active_ix).copied(),
                        PaneRef::Split { children, .. } => children.iter().find_map(first_active),
                    }
                }
                let previous_active = area
                    .layout(staging)
                    .and_then(|tree| first_active(tree.root()));
                area.add_panel_view(panel, staging, None, window, cx);
                area.move_panel(id, target, window, cx);
                if let Some(previous) = previous_active {
                    area.select_panel(previous, window, cx);
                }
                prune_empty_docks(area, window, cx);
            }
            if let Some(region) = panel_region(area, id) {
                if region != DockPlacement::Center && !area.is_dock_open(region) {
                    area.toggle_dock(region, window, cx);
                }
            }
            area.select_panel(id, window, cx);
        });
    }
}

fn panel_region(area: &DockArea, id: PanelId) -> Option<DockPlacement> {
    REGIONS.into_iter().find(|region| {
        area.layout(*region)
            .is_some_and(|tree| tree.find_panel_node(id).is_some())
    })
}

fn prune_empty_docks(area: &mut DockArea, window: &mut Window, cx: &mut Context<DockArea>) {
    for placement in REGIONS.into_iter().skip(1) {
        if area
            .layout(placement)
            .is_some_and(|tree| tree.panels().next().is_none())
        {
            area.remove_dock(placement, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LayoutConfig, validate_layout};
    use editor_core::{ConfigStore, WorkspaceKind};
    use gpui_kit::component::dock::{DockAreaState, PanelInfo, PanelState};

    #[test]
    fn workspace_layouts_require_their_concrete_viewport_and_version() {
        let mut state = DockAreaState {
            version: Some(5),
            center: PanelState::new("ModelingViewport"),
            ..Default::default()
        };
        assert!(validate_layout(&state, WorkspaceKind::Modeling).is_ok());
        assert!(validate_layout(&state, WorkspaceKind::Animation).is_err());
        state.version = Some(8);
        state.center = PanelState::new("AnimationViewport");
        // A deliberately closed Timeline is still a valid new Animation layout.
        assert!(validate_layout(&state, WorkspaceKind::Animation).is_ok());
        assert!(validate_layout(&state, WorkspaceKind::Modeling).is_err());
    }
    fn layout() -> DockAreaState {
        DockAreaState {
            version: Some(5),
            center: PanelState {
                panel_name: "StackPanel".into(),
                children: vec![
                    PanelState::new("MeshTree"),
                    PanelState::new("ModelingViewport"),
                    PanelState::new("Properties"),
                ],
                info: PanelInfo::stack(vec![], gpui_kit::Axis::Horizontal),
            },
            ..Default::default()
        }
    }
    #[test]
    fn layouts_roundtrip_and_accept_canvas_groups_but_reject_invalid_panels() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let default = layout();
        validate_layout(&default, WorkspaceKind::Modeling)?;
        let mut edited = default.clone();
        edited.center.children.remove(0);
        validate_layout(&edited, WorkspaceKind::Modeling)?;
        store.write(
            "layout-modeling.json",
            &LayoutConfig {
                workspace: WorkspaceKind::Modeling,
                layout: edited.clone(),
            },
        )?;
        assert_eq!(
            store
                .read::<LayoutConfig>("layout-modeling.json")?
                .unwrap()
                .layout,
            edited
        );
        store.write(
            "layout-modeling.json",
            &LayoutConfig {
                workspace: WorkspaceKind::Modeling,
                layout: default.clone(),
            },
        )?;
        assert_eq!(
            store
                .read::<LayoutConfig>("layout-modeling.json")?
                .unwrap()
                .layout,
            default
        );
        let mut invalid = default.clone();
        invalid.center.info = PanelInfo::tabs(0);
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_ok());
        invalid = default.clone();
        invalid.version = Some(1);
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        invalid = default.clone();
        invalid.center.children.push(PanelState::new("Layers"));
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        invalid = default.clone();
        invalid.center.children.push(PanelState::new("unknown"));
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        invalid = default.clone();
        invalid
            .center
            .children
            .push(PanelState::new("ModelingViewport"));
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        std::fs::write(dir.path().join("layout-modeling.json"), "{")?;
        assert!(store.read::<LayoutConfig>("layout-modeling.json").is_err());
        Ok(())
    }

    #[test]
    fn invalid_dock_regions_and_nested_tab_groups_are_rejected() {
        use gpui_kit::component::dock::{DockPlacement, DockState};
        let mut invalid = layout();
        invalid.right_dock = Some(DockState::new(
            PanelState::new("Resources"),
            DockPlacement::Center,
            gpui_kit::px(240.),
            true,
        ));
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        invalid = layout();
        invalid.center.children[0] = PanelState {
            panel_name: "TabPanel".into(),
            children: vec![PanelState::new("MeshTree")],
            info: PanelInfo::tabs(1),
        };
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
        invalid.center.children[0].info = PanelInfo::tabs(0);
        invalid.center.children[0].children[0].info = PanelInfo::tabs(0);
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
    }

    #[test]
    fn layout_writer_flushes_latest_state_on_shutdown_and_reports_io_errors() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let (writer, errors) = super::LayoutWriter::new(store.clone());
        writer.save(WorkspaceKind::Modeling, layout())?;
        let mut latest = layout();
        latest.center.children.remove(0);
        writer.save(WorkspaceKind::Modeling, latest.clone())?;
        let animation = DockAreaState {
            version: Some(5),
            center: PanelState::new("ModelingViewport"),
            ..Default::default()
        };
        writer.save(WorkspaceKind::Animation, animation.clone())?;
        writer.save(WorkspaceKind::Modeling, latest.clone())?;
        drop(writer);
        assert_eq!(
            store
                .read::<LayoutConfig>("layout-modeling.json")?
                .unwrap()
                .layout,
            latest
        );
        assert!(errors.try_recv().is_err());
        assert_eq!(
            store
                .read::<LayoutConfig>("layout-animation.json")?
                .unwrap()
                .layout,
            animation
        );
        assert!(validate_layout(&latest, WorkspaceKind::Animation).is_err());

        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "blocked")?;
        let (writer, errors) = super::LayoutWriter::new(ConfigStore::new(blocked));
        writer.save(WorkspaceKind::Modeling, layout())?;
        drop(writer);
        assert!(errors.try_recv().is_ok());
        Ok(())
    }
}
