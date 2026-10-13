use super::*;
use editor_core::{PANELS, PanelKind, PanelRegion, WorkspaceKind};
use gpui_kit::component::dock::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    rc::{Rc, Weak},
    sync::Arc,
};
use std::{sync::mpsc, thread, time::Duration};

pub(super) mod add_panel;
mod skin;

/// Serialize persistence off the UI thread and coalesce a resize/drag burst.
/// Dropping the writer flushes the last layout before joining its worker.
struct LayoutWriter {
    sender: Option<mpsc::Sender<LayoutConfig>>,
    worker: Option<thread::JoinHandle<()>>,
    #[cfg(test)]
    test_store: Option<editor_core::ConfigStore>,
}
impl LayoutWriter {
    #[cfg(test)]
    fn new_sync(store: editor_core::ConfigStore) -> (Self, async_channel::Receiver<String>) {
        let (_, errors) = async_channel::unbounded();
        (
            Self {
                sender: None,
                worker: None,
                test_store: Some(store),
            },
            errors,
        )
    }
    fn new(store: editor_core::ConfigStore) -> (Self, async_channel::Receiver<String>) {
        let (sender, receiver) = mpsc::channel::<LayoutConfig>();
        let (errors, error_receiver) = async_channel::unbounded();
        let worker = thread::spawn(move || {
            while let Ok(first) = receiver.recv() {
                let mut pending = HashMap::from([(first.workspace, first)]);
                let closed = loop {
                    match receiver.recv_timeout(Duration::from_millis(150)) {
                        Ok(next) => {
                            pending.insert(next.workspace, next);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => break false,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break true,
                    }
                };
                for (workspace, latest) in pending {
                    if let Err(error) = store.write(layout_file(workspace), &latest) {
                        let _ = errors.try_send(error.to_string());
                    }
                }
                if closed {
                    break;
                }
            }
        });
        (
            Self {
                sender: Some(sender),
                worker: Some(worker),
                #[cfg(test)]
                test_store: None,
            },
            error_receiver,
        )
    }
    fn save(&self, workspace: WorkspaceKind, layout: DockAreaState) -> anyhow::Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return store.write(layout_file(workspace), &LayoutConfig { workspace, layout });
        }
        self.sender
            .as_ref()
            .unwrap()
            .send(LayoutConfig { workspace, layout })
            .map_err(|_| anyhow::anyhow!("Layout persistence worker stopped"))
    }
}
impl Drop for LayoutWriter {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

type PanelViews = HashMap<PanelKind, Arc<dyn BasePanelView>>;

// Factories resolve against the requesting area, never the last-created window.
// Weak ownership keeps the global registry from retaining closed editors.
#[derive(Default)]
struct WorkspacePanels(HashMap<EntityId, Weak<PanelViews>>);
impl Global for WorkspacePanels {}

const REGIONS: [DockPlacement; 4] = [
    DockPlacement::Center,
    DockPlacement::Left,
    DockPlacement::Right,
    DockPlacement::Bottom,
];

pub struct DockWorkspace {
    pub area: Entity<DockArea>,
    pub canvas: Entity<crate::canvas::CanvasView>,
    pub animation_motions: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_expressions: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_info: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_parameters: Entity<crate::panels::animation_panel::AnimationPanel>,
    pub animation_inspectors:
        HashMap<PanelKind, Entity<crate::panels::animation_panel::AnimationPanel>>,
    pub properties: Entity<crate::panels::PropertiesPanel>,
    pub mesh_tree: Entity<crate::panels::MeshTreePanel>,
    pub deformer_tree: Entity<crate::panels::DeformerTreePanel>,
    pub parameters: Entity<crate::panels::ParametersPanel>,
    pub document_panels: std::collections::HashMap<
        crate::panels::DocumentPanelKind,
        Entity<crate::panels::DocumentPanel>,
    >,
    panels: Rc<PanelViews>,
    pub last_valid: DockAreaState,
    pending_layout_measurement: bool,
    workspace: WorkspaceKind,
    layouts: HashMap<WorkspaceKind, DockAreaState>,
    layout_epoch: u64,
    writer: LayoutWriter,
    errors: Option<async_channel::Receiver<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct LayoutConfig {
    pub workspace: WorkspaceKind,
    pub layout: DockAreaState,
}

fn layout_file(workspace: WorkspaceKind) -> &'static str {
    match workspace {
        WorkspaceKind::Modeling => "layout-modeling.json",
        WorkspaceKind::Animation => "layout-animation.json",
    }
}

impl DockWorkspace {
    pub fn new(
        workspace: WorkspaceKind,
        owner: WeakEntity<EditorView>,
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let canvas = crate::canvas::CanvasView::new(owner.clone(), window, cx);
        let properties = crate::panels::PropertiesPanel::new(owner.clone(), window, cx);
        let mesh_tree =
            crate::panels::MeshTreePanel::new(owner.clone(), thumbnails.clone(), window, cx);
        let deformer_tree =
            crate::panels::DeformerTreePanel::new(owner.clone(), thumbnails, window, cx);
        let parameters = crate::panels::ParametersPanel::new(owner.clone(), window, cx);
        use crate::panels::animation_panel::{AnimationPanel, AnimationPanelKind};
        let animation_motions =
            AnimationPanel::new(owner.clone(), AnimationPanelKind::Motions, window, cx);
        let animation_expressions =
            AnimationPanel::new(owner.clone(), AnimationPanelKind::Expressions, window, cx);
        let animation_info =
            AnimationPanel::new(owner.clone(), AnimationPanelKind::Info, window, cx);
        let animation_parameters =
            AnimationPanel::new(owner.clone(), AnimationPanelKind::Parameters, window, cx);

        let animation_inspectors: HashMap<_, _> = [
            (
                PanelKind::AnimationProperties,
                AnimationPanelKind::Properties,
            ),
            (PanelKind::AnimationPhysics, AnimationPanelKind::Physics),
            (PanelKind::AnimationPose, AnimationPanelKind::Pose),
        ]
        .into_iter()
        .map(|(id, kind)| (id, AnimationPanel::new(owner.clone(), kind, window, cx)))
        .collect();
        let mut panels: PanelViews = animation_inspectors
            .iter()
            .map(|(kind, panel)| (*kind, panel_handle(panel.clone())))
            .collect();
        for (name, handle) in [
            ("Canvas", panel_handle(canvas.clone())),
            ("AnimationMotions", panel_handle(animation_motions.clone())),
            (
                "AnimationExpressions",
                panel_handle(animation_expressions.clone()),
            ),
            ("AnimationInfo", panel_handle(animation_info.clone())),
            (
                "AnimationParameters",
                panel_handle(animation_parameters.clone()),
            ),
            ("Properties", panel_handle(properties.clone())),
            ("MeshTree", panel_handle(mesh_tree.clone())),
            ("Deformers", panel_handle(deformer_tree.clone())),
            ("Parameters", panel_handle(parameters.clone())),
        ] {
            panels.insert(PanelKind::from_name(name).unwrap(), handle);
        }

        let mut document_panels = std::collections::HashMap::new();
        for kind in crate::panels::DocumentPanelKind::ALL {
            let panel = crate::panels::DocumentPanel::new(owner.clone(), kind, window, cx);
            let handle = panel_handle(panel.clone());
            panels.insert(PanelKind::from_name(kind.name()).unwrap(), handle);
            document_panels.insert(kind, panel);
        }
        let area = cx.new(|cx| {
            let skin = DockSkin::new(cx);
            DockArea::new("kasane-workspace", Some(3), window, cx).with_renderer(
                skin::EditorDockSkin::new(
                    skin,
                    parameters.entity_id(),
                    cx.weak_entity(),
                    owner.clone(),
                ),
            )
        });
        let panels = Rc::new(panels);
        if cx.try_global::<WorkspacePanels>().is_none() {
            cx.set_global(WorkspacePanels::default());
        }
        let registry = &mut cx.global_mut::<WorkspacePanels>().0;
        registry.retain(|_, views| views.strong_count() > 0);
        registry.insert(area.entity_id(), Rc::downgrade(&panels));
        for spec in PANELS {
            let kind = spec.kind;
            register_panel(cx, spec.name, move |context, _, cx| {
                cx.global::<WorkspacePanels>().0[&context.dock_area().entity_id()]
                    .upgrade()
                    .expect("live editor panel catalog")[&kind]
                    .clone()
            });
        }
        let last_valid = area.read(cx).dump(cx);
        #[cfg(test)]
        let (writer, errors) =
            LayoutWriter::new_sync(cx.global::<settings::Settings>().store.clone());
        #[cfg(not(test))]
        let (writer, errors) = LayoutWriter::new(cx.global::<settings::Settings>().store.clone());

        let mut result = Self {
            area,
            canvas,
            animation_motions,
            animation_expressions,
            animation_info,
            animation_parameters,
            animation_inspectors,
            properties,
            mesh_tree,
            deformer_tree,
            parameters,
            document_panels,
            panels,
            last_valid,
            pending_layout_measurement: false,
            workspace,
            layouts: HashMap::new(),
            layout_epoch: 0,
            writer,
            errors: Some(errors),
        };
        result.reset(window, cx);
        result
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut App) {
        let column = |region, cx: &App| {
            let mut groups = std::collections::BTreeMap::<usize, DockLayout>::new();
            for spec in PANELS.iter().filter(|spec| {
                spec.available(self.workspace) && spec.default_visible && spec.region == region
            }) {
                let group = groups
                    .remove(&spec.group)
                    .unwrap_or_else(DockLayout::tabs)
                    .panel_view(self.panels[&spec.kind].clone(), cx);
                groups.insert(spec.group, group);
            }
            if groups.len() == 1 {
                groups.into_values().next().unwrap()
            } else {
                groups
                    .into_values()
                    .fold(DockLayout::v_split(), |column, group| {
                        column.child(group, None)
                    })
            }
        };
        let (left, right) = if self.workspace == WorkspaceKind::Animation {
            (220., 240.)
        } else {
            (360., 360.)
        };
        let layout = DockLayout::h_split()
            .child(column(PanelRegion::Left, cx), Some(px(left)))
            .child(column(PanelRegion::Center, cx), None)
            .child(column(PanelRegion::Right, cx), Some(px(right)));
        self.area.update(cx, |area, cx| {
            area.set_zoomed_out(window, cx);
            for placement in REGIONS.into_iter().skip(1) {
                area.remove_dock(placement, window, cx);
            }
            area.set_center(layout, window, cx);
        });
        self.last_valid = self.area.read(cx).dump(cx);
    }

    fn restore(&mut self, window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
        let saved = if let Some(layout) = self.layouts.get(&self.workspace) {
            Some(LayoutConfig {
                workspace: self.workspace,
                layout: layout.clone(),
            })
        } else {
            cx.global::<settings::Settings>()
                .store
                .read::<LayoutConfig>(layout_file(self.workspace))?
        };
        if let Some(saved) = saved {
            anyhow::ensure!(
                saved.workspace == self.workspace,
                "Layout workspace does not match"
            );
            validate_layout(&saved.layout, self.workspace)?;
            self.area
                .update(cx, |area, cx| area.load(saved.layout, window, cx))?;
            self.last_valid = self.area.read(cx).dump(cx);
        }
        Ok(())
    }

    fn switch(
        &mut self,
        workspace: WorkspaceKind,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        if self.workspace == workspace {
            return Ok(());
        }
        let previous = self.area.read(cx).dump(cx);
        let previous = if validate_layout(&previous, self.workspace).is_ok() {
            previous
        } else {
            self.last_valid.clone()
        };
        self.layouts.insert(self.workspace, previous.clone());
        // Preserve accepted structure even if switching again before the
        // measurement callback. DockArea::dump supplies valid fallback sizes.
        let saved = self.writer.save(self.workspace, previous);
        self.workspace = workspace;
        self.layout_epoch += 1;
        self.pending_layout_measurement = true;
        self.reset(window, cx);
        let result = self.restore(window, cx);
        if result.is_err() {
            self.reset(window, cx);
        }
        result.and(saved)
    }

    pub fn contains(&self, kind: PanelKind, cx: &App) -> bool {
        self.area
            .read(cx)
            .panel(self.panels[&kind].panel_id(cx))
            .is_some()
    }

    pub fn toggle(&self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        if !kind.spec().available(self.workspace) || kind == PanelKind::Canvas {
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
                        PanelKind::AnimationInfo => {
                            area.remove_panel(self.animation_info.clone(), window, cx)
                        }
                        PanelKind::AnimationParameters => {
                            area.remove_panel(self.animation_parameters.clone(), window, cx)
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
        let canvas = self.panels[&PanelKind::Canvas].panel_id(cx);
        let node = area
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(canvas)
            .unwrap();
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
    pub(super) fn show(
        &self,
        kind: PanelKind,
        target: Option<InsertTarget>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if kind == PanelKind::Canvas || !kind.spec().available(self.workspace) {
            return;
        }
        let panel = self.panels[&kind].clone();
        let id = panel.panel_id(cx);
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
            let Some(PaneRef::Tabs { panels, .. }) = tree.find_node(node).map(|node| node.kind())
            else {
                return;
            };
            if panels.contains(&self.panels[&PanelKind::Canvas].panel_id(cx)) {
                return;
            }
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

pub fn validate_layout(state: &DockAreaState, workspace: WorkspaceKind) -> anyhow::Result<()> {
    use anyhow::ensure;
    fn walk(
        node: &PanelState,
        seen: &mut std::collections::HashSet<String>,
        depth: usize,
        workspace: WorkspaceKind,
    ) -> anyhow::Result<usize> {
        ensure!(
            depth < 16 && node.children.len() < 32,
            "Layout nesting is invalid"
        );
        match &node.info {
            PanelInfo::Panel(_) => {
                ensure!(
                    node.children.is_empty(),
                    "Leaf panel cannot contain children"
                );
                let kind = PanelKind::from_name(&node.panel_name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown panel"))?;
                ensure!(
                    kind.spec().available(workspace),
                    "Panel is unavailable in this workspace"
                );
                ensure!(seen.insert(node.panel_name.clone()), "Duplicate panel");
                Ok(usize::from(node.panel_name == "Canvas"))
            }
            PanelInfo::Tabs { active_index } => {
                ensure!(
                    (*active_index < node.children.len()
                        || (node.children.is_empty() && *active_index == 0))
                        && node
                            .children
                            .iter()
                            .all(|child| matches!(child.info, PanelInfo::Panel(_))),
                    "Invalid tab group"
                );
                let mut canvas = 0;
                for child in &node.children {
                    canvas += walk(child, seen, depth + 1, workspace)?;
                }
                ensure!(
                    canvas == 0 || node.children.len() == 1,
                    "Canvas cannot share a dock tab group"
                );
                Ok(canvas)
            }
            PanelInfo::Stack { sizes, axis } => {
                ensure!(
                    *axis <= 1
                        && sizes
                            .iter()
                            .all(|s| s.as_f32().is_finite() && s.as_f32() >= 0.),
                    "Invalid split size"
                );
                let mut canvas = 0;
                for child in &node.children {
                    canvas += walk(child, seen, depth + 1, workspace)?;
                }
                Ok(canvas)
            }
        }
    }
    ensure!(state.version == Some(3), "Unsupported layout version");
    let mut seen = std::collections::HashSet::new();
    ensure!(
        walk(&state.center, &mut seen, 0, workspace)? == 1,
        "Exactly one central canvas is required"
    );
    for (placement, dock) in [
        (DockPlacement::Left, &state.left_dock),
        (DockPlacement::Right, &state.right_dock),
        (DockPlacement::Bottom, &state.bottom_dock),
    ] {
        let Some(dock) = dock else {
            continue;
        };
        ensure!(
            dock.placement() == placement,
            "Dock placement does not match its region"
        );
        ensure!(
            dock.size().as_f32().is_finite() && dock.size().as_f32() >= 0.,
            "Invalid dock size"
        );
        ensure!(
            walk(dock.panel(), &mut seen, 0, workspace)? == 0,
            "Canvas cannot move into a side dock"
        );
    }
    Ok(())
}
impl EditorView {
    pub(super) fn switch_workspace(
        &mut self,
        workspace: WorkspaceKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.workspace() == workspace {
            return;
        }
        // Let the existing input blur rules finish before changing context.
        window.focus(&self.focus, cx);
        if let Some(dock) = &self.dock {
            dock.canvas.update(cx, |canvas, _| canvas.cancel_pointer());
            for panel in dock.document_panels.values() {
                panel.update(cx, |panel, _| panel.cancel_drag());
            }
        }
        if let Err(error) = self.editor.set_workspace(workspace) {
            self.edit_error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.frame = None;
        self.animation_frame = None;
        self.animation_anchor = None;
        self.frozen_frames.clear();
        self.render_error = None;
        if let Some(dock) = &mut self.dock {
            if let Err(error) = dock.switch(workspace, window, cx) {
                self.edit_error = Some(error.to_string());
            }
        }
        self.save_layout_after_frame(window, cx);
        if let Err(error) = cx
            .global_mut::<settings::Settings>()
            .set_workspace(workspace)
        {
            self.edit_error = Some(error.to_string());
        }
        self.redraw(cx);
    }

    pub(super) fn reveal_keyforms(&mut self, window: &mut Window, cx: &mut App) {
        if self.editor.workspace() != WorkspaceKind::Modeling {
            return;
        }
        if let Some(dock) = &self.dock {
            dock.show(PanelKind::Keyforms, None, window, cx);
        }
    }
    pub(super) fn init_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dock = DockWorkspace::new(
            self.editor.workspace(),
            cx.weak_entity(),
            self.mesh_thumbnails.clone(),
            window,
            cx,
        );
        let errors = dock.errors.take().unwrap();
        cx.spawn(async move |this, cx| {
            while let Ok(error) = errors.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.edit_error = Some(error);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if let Err(error) = dock.restore(window, cx) {
            self.message = error.to_string();
            dock.reset(window, cx);
        }
        self.ui_subscriptions.push(cx.subscribe_in(
            &dock.area,
            window,
            |this, area, event, window, cx| {
                if !matches!(event, DockEvent::LayoutChanged) {
                    return;
                }
                let state = area.read(cx).dump(cx);
                let Some(dock) = &mut this.dock else {
                    return;
                };
                for panel in dock.document_panels.values() {
                    panel.update(cx, |panel, _| panel.cancel_drag());
                }

                if dock.pending_layout_measurement {
                    return;
                }
                if state == dock.last_valid {
                    return;
                }
                if validate_layout(&state, dock.workspace).is_err() {
                    let previous = dock.last_valid.clone();
                    let _ = area.update(cx, |area, cx| area.load(previous, window, cx));
                    return;
                }
                dock.last_valid = state.clone();
                if let Err(error) = dock.writer.save(dock.workspace, state) {
                    this.edit_error = Some(error.to_string());
                }
                cx.notify();
            },
        ));
        dock.pending_layout_measurement = true;
        self.dock = Some(dock);
        self.save_layout_after_frame(window, cx);
    }
    fn save_layout_after_frame(&self, window: &Window, cx: &Context<Self>) {
        let Some(dock) = &self.dock else { return };
        let workspace = dock.workspace;
        let epoch = dock.layout_epoch;
        let owner = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = owner.update(cx, |view, cx| {
                let Some(dock) = &mut view.dock else { return };
                if dock.workspace != workspace || dock.layout_epoch != epoch {
                    return;
                }
                let state = dock.area.read(cx).dump(cx);
                if validate_layout(&state, workspace).is_err() {
                    let previous = dock.last_valid.clone();
                    dock.area
                        .update(cx, |area, cx| area.load(previous, window, cx))
                        .ok();
                    dock.pending_layout_measurement = false;
                    return;
                }
                dock.last_valid = state.clone();
                dock.pending_layout_measurement = false;
                if let Err(error) = dock.writer.save(dock.workspace, state) {
                    view.edit_error = Some(error.to_string());
                    cx.notify();
                }
            });
        });
    }
    pub(super) fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dock) = &mut self.dock {
            dock.pending_layout_measurement = true;
            dock.layout_epoch += 1;
            dock.reset(window, cx);
            self.save_layout_after_frame(window, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{LayoutConfig, validate_layout};
    use editor_core::{ConfigStore, WorkspaceKind};
    use gpui_kit::component::dock::{DockAreaState, PanelInfo, PanelState};
    fn layout() -> DockAreaState {
        DockAreaState {
            version: Some(3),
            center: PanelState {
                panel_name: "StackPanel".into(),
                children: vec![
                    PanelState::new("MeshTree"),
                    PanelState::new("Canvas"),
                    PanelState::new("Properties"),
                ],
                info: PanelInfo::stack(vec![], gpui_kit::Axis::Horizontal),
            },
            ..Default::default()
        }
    }
    #[test]
    fn layouts_roundtrip_reset_and_reject_canvas_groups_or_unknown_panels() -> anyhow::Result<()> {
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
        assert!(validate_layout(&invalid, WorkspaceKind::Modeling).is_err());
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
        invalid.center.children.push(PanelState::new("Canvas"));
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
            version: Some(3),
            center: PanelState::new("Canvas"),
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
