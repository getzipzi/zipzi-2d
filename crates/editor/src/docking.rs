use super::*;
use gpui_kit::component::dock::*;
use serde::{Deserialize, Serialize};
use std::{sync::mpsc, thread, time::Duration};

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
            while let Ok(mut latest) = receiver.recv() {
                let closed = loop {
                    match receiver.recv_timeout(Duration::from_millis(150)) {
                        Ok(next) => latest = next,
                        Err(mpsc::RecvTimeoutError::Timeout) => break false,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break true,
                    }
                };
                if let Err(error) = store.write("layout.json", &latest) {
                    let _ = errors.try_send(error.to_string());
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
    fn save(&self, layout: DockAreaState) -> anyhow::Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return store.write("layout.json", &LayoutConfig { layout });
        }
        self.sender
            .as_ref()
            .unwrap()
            .send(LayoutConfig { layout })
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

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Properties,
    MeshTree,
    Deformers,
    Parameters,
    Document(crate::panels::DocumentPanelKind),
}

pub struct DockWorkspace {
    pub area: Entity<DockArea>,
    pub canvas: Entity<crate::canvas::CanvasView>,
    pub properties: Entity<crate::panels::PropertiesPanel>,
    pub mesh_tree: Entity<crate::panels::MeshTreePanel>,
    pub deformer_tree: Entity<crate::panels::DeformerTreePanel>,
    pub parameters: Entity<crate::panels::ParametersPanel>,
    pub document_panels: std::collections::HashMap<
        crate::panels::DocumentPanelKind,
        Entity<crate::panels::DocumentPanel>,
    >,
    pub last_valid: DockAreaState,
    pending_layout_measurement: bool,
    writer: LayoutWriter,
    errors: Option<async_channel::Receiver<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct LayoutConfig {
    pub layout: DockAreaState,
}

impl DockWorkspace {
    pub fn new(
        owner: WeakEntity<EditorView>,
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let canvas = crate::canvas::CanvasView::new(owner.clone(), window, cx);
        let properties = crate::panels::PropertiesPanel::new(owner.clone(), window, cx);
        let mesh_tree = crate::panels::MeshTreePanel::new(owner.clone(), thumbnails.clone(), cx);
        let deformer_tree = crate::panels::DeformerTreePanel::new(owner.clone(), thumbnails, cx);
        let parameters = crate::panels::ParametersPanel::new(owner.clone(), window, cx);

        for (name, handle) in [
            ("Canvas", panel_handle(canvas.clone())),
            ("Properties", panel_handle(properties.clone())),
            ("MeshTree", panel_handle(mesh_tree.clone())),
            ("Deformers", panel_handle(deformer_tree.clone())),
            ("Parameters", panel_handle(parameters.clone())),
        ] {
            register_panel(cx, name, move |_, _, _| handle.clone());
        }

        let mut document_panels = std::collections::HashMap::new();
        for kind in crate::panels::DocumentPanelKind::ALL {
            let panel = crate::panels::DocumentPanel::new(owner.clone(), kind, window, cx);
            let handle = panel_handle(panel.clone());
            register_panel(cx, kind.name(), move |_, _, _| handle.clone());
            document_panels.insert(kind, panel);
        }
        let area = cx.new(|cx| {
            let skin = DockSkin::new(cx);
            DockArea::new("kasane-workspace", Some(3), window, cx).with_renderer(
                skin::EditorDockSkin::new(skin, parameters.entity_id(), cx.weak_entity()),
            )
        });
        let last_valid = area.read(cx).dump(cx);
        #[cfg(test)]
        let (writer, errors) =
            LayoutWriter::new_sync(cx.global::<settings::Settings>().store.clone());
        #[cfg(not(test))]
        let (writer, errors) = LayoutWriter::new(cx.global::<settings::Settings>().store.clone());

        let mut result = Self {
            area,
            canvas,
            properties,
            mesh_tree,
            deformer_tree,
            parameters,
            document_panels,
            last_valid,
            pending_layout_measurement: false,
            writer,
            errors: Some(errors),
        };
        result.reset(window, cx);
        result
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut App) {
        self.area.update(cx, |area, cx| {
            for placement in [
                DockPlacement::Left,
                DockPlacement::Right,
                DockPlacement::Bottom,
            ] {
                area.remove_dock(placement, window, cx);
            }
            area.set_center(
                DockLayout::h_split()
                    .child(
                        DockLayout::v_split()
                            .child(
                                DockLayout::tabs()
                                    .panel_view(panel_handle(self.mesh_tree.clone()), cx)
                                    .panel_view(
                                        panel_handle(
                                            self.document_panels
                                                [&crate::panels::DocumentPanelKind::DrawOrder]
                                                .clone(),
                                        ),
                                        cx,
                                    ),
                                None,
                            )
                            .child(
                                DockLayout::tabs()
                                    .panel_view(panel_handle(self.deformer_tree.clone()), cx),
                                None,
                            ),
                        Some(px(360.)),
                    )
                    .child(
                        DockLayout::tabs().panel_view(panel_handle(self.canvas.clone()), cx),
                        None,
                    )
                    .child(
                        DockLayout::v_split()
                            .child(
                                DockLayout::tabs()
                                    .panel_view(panel_handle(self.properties.clone()), cx)
                                    .panel_view(
                                        panel_handle(
                                            self.document_panels
                                                [&crate::panels::DocumentPanelKind::Relations]
                                                .clone(),
                                        ),
                                        cx,
                                    ),
                                None,
                            )
                            .child(
                                DockLayout::tabs()
                                    .panel_view(panel_handle(self.parameters.clone()), cx),
                                None,
                            ),
                        Some(px(360.)),
                    ),
                window,
                cx,
            );
        });
        self.last_valid = self.area.read(cx).dump(cx);
    }

    pub fn toggle(&self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        match kind {
            PanelKind::Document(kind) => toggle_panel(
                &self.area,
                self.document_panels[&kind].clone(),
                DockPlacement::Right,
                window,
                cx,
            ),
            PanelKind::Properties => toggle_panel(
                &self.area,
                self.properties.clone(),
                DockPlacement::Right,
                window,
                cx,
            ),
            PanelKind::MeshTree => toggle_panel(
                &self.area,
                self.mesh_tree.clone(),
                DockPlacement::Left,
                window,
                cx,
            ),
            PanelKind::Deformers => toggle_panel(
                &self.area,
                self.deformer_tree.clone(),
                DockPlacement::Left,
                window,
                cx,
            ),
            PanelKind::Parameters => toggle_panel(
                &self.area,
                self.parameters.clone(),
                DockPlacement::Right,
                window,
                cx,
            ),
        }
    }
}

fn toggle_panel<P: Panel>(
    area: &Entity<DockArea>,
    panel: Entity<P>,
    placement: DockPlacement,
    window: &mut Window,
    cx: &mut App,
) {
    let exists = area
        .read(cx)
        .panel(PanelId::from(panel.entity_id()))
        .is_some();
    area.update(cx, |area, cx| {
        if exists {
            area.remove_panel(panel, window, cx);
        } else {
            area.add_panel_view(panel_handle(panel), placement, None, window, cx);
        }
    });
}

pub fn validate_layout(state: &DockAreaState) -> anyhow::Result<()> {
    use anyhow::ensure;
    fn walk(
        node: &PanelState,
        seen: &mut std::collections::HashSet<String>,
        depth: usize,
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
                ensure!(
                    [
                        "Canvas",
                        "Properties",
                        "MeshTree",
                        "Deformers",
                        "Parameters",
                        "DrawOrder",
                        "Relations",
                        "Keyforms",
                        "BlendShapes",
                        "Glue",
                        "Motions",
                        "Expressions",
                        "Physics",
                        "Pose",
                        "Resources"
                    ]
                    .contains(&node.panel_name.as_str()),
                    "Unknown panel"
                );
                ensure!(seen.insert(node.panel_name.clone()), "Duplicate panel");
                Ok(usize::from(node.panel_name == "Canvas"))
            }
            PanelInfo::Tabs { .. } => {
                let mut canvas = 0;
                for child in &node.children {
                    canvas += walk(child, seen, depth + 1)?;
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
                    canvas += walk(child, seen, depth + 1)?;
                }
                Ok(canvas)
            }
        }
    }
    ensure!(state.version == Some(3), "Unsupported layout version");
    let mut seen = std::collections::HashSet::new();
    ensure!(
        walk(&state.center, &mut seen, 0)? == 1,
        "Exactly one central canvas is required"
    );
    for dock in [&state.left_dock, &state.right_dock, &state.bottom_dock]
        .into_iter()
        .flatten()
    {
        ensure!(
            dock.size().as_f32().is_finite() && dock.size().as_f32() >= 0.,
            "Invalid dock size"
        );
        ensure!(
            walk(dock.panel(), &mut seen, 0)? == 0,
            "Canvas cannot move into a side dock"
        );
    }
    Ok(())
}
impl EditorView {
    pub(super) fn reveal_keyforms(&mut self, window: &mut Window, cx: &mut App) {
        if let Some(dock) = &self.dock {
            let panel = dock.document_panels[&crate::panels::DocumentPanelKind::Keyforms].clone();
            let id = PanelId::from(panel.entity_id());
            let exists = dock.area.read(cx).panel(id).is_some();
            dock.area.update(cx, |area, cx| {
                if !exists {
                    area.add_panel_view(
                        panel_handle(panel),
                        DockPlacement::Bottom,
                        None,
                        window,
                        cx,
                    );
                }
                area.select_panel(id, window, cx);
            });
        }
    }
    pub(super) fn init_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dock =
            DockWorkspace::new(cx.weak_entity(), self.mesh_thumbnails.clone(), window, cx);
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
        let store = cx.global::<settings::Settings>().store.clone();
        let mut save_measured_default = false;
        match store.read::<LayoutConfig>("layout.json") {
            // Version 1 includes the retired Layers panel and tabbed sidebars.
            // Replace it once with the new split layout after its first frame
            // has measured the flexible center pane.
            Ok(Some(saved)) if matches!(saved.layout.version, Some(1 | 2)) => {
                save_measured_default = true;
            }
            // An older build saved the default before layout, when every
            // unconstrained pane still had the 100px placeholder size.
            Ok(Some(saved)) if saved.layout == dock.last_valid => {
                save_measured_default = true;
            }
            Ok(Some(saved)) => {
                match validate_layout(&saved.layout).and_then(|_| {
                    dock.area
                        .update(cx, |area, cx| area.load(saved.layout, window, cx))
                }) {
                    Ok(()) => dock.last_valid = dock.area.read(cx).dump(cx),
                    Err(e) => {
                        self.message = e.to_string();
                        dock.reset(window, cx);
                        save_measured_default = true;
                    }
                }
            }
            Err(e) => self.message = e.to_string(),
            _ => {}
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
                if validate_layout(&state).is_err() {
                    let previous = dock.last_valid.clone();
                    let _ = area.update(cx, |area, cx| area.load(previous, window, cx));
                    return;
                }
                dock.last_valid = state.clone();
                if let Err(error) = dock.writer.save(state) {
                    this.edit_error = Some(error.to_string());
                }
                cx.notify();
            },
        ));
        dock.pending_layout_measurement = save_measured_default;
        self.dock = Some(dock);
        if save_measured_default {
            self.save_layout_after_frame(window, cx);
        }
    }
    fn save_layout_after_frame(&self, window: &Window, cx: &Context<Self>) {
        let owner = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = owner.update(cx, |view, cx| {
                let Some(dock) = &mut view.dock else { return };
                let state = dock.area.read(cx).dump(cx);
                dock.last_valid = state.clone();
                dock.pending_layout_measurement = false;
                if let Err(error) = dock.writer.save(state) {
                    view.edit_error = Some(error.to_string());
                    cx.notify();
                }
            });
        });
    }
    pub(super) fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dock) = &mut self.dock {
            dock.pending_layout_measurement = true;
            dock.reset(window, cx);
            self.save_layout_after_frame(window, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{LayoutConfig, validate_layout};
    use editor_core::ConfigStore;
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
        validate_layout(&default)?;
        let mut edited = default.clone();
        edited.center.children.remove(0);
        validate_layout(&edited)?;
        store.write(
            "layout.json",
            &LayoutConfig {
                layout: edited.clone(),
            },
        )?;
        assert_eq!(
            store.read::<LayoutConfig>("layout.json")?.unwrap().layout,
            edited
        );
        store.write(
            "layout.json",
            &LayoutConfig {
                layout: default.clone(),
            },
        )?;
        assert_eq!(
            store.read::<LayoutConfig>("layout.json")?.unwrap().layout,
            default
        );
        let mut invalid = default.clone();
        invalid.center.info = PanelInfo::tabs(0);
        assert!(validate_layout(&invalid).is_err());
        invalid = default.clone();
        invalid.version = Some(1);
        assert!(validate_layout(&invalid).is_err());
        invalid = default.clone();
        invalid.center.children.push(PanelState::new("Layers"));
        assert!(validate_layout(&invalid).is_err());
        invalid = default.clone();
        invalid.center.children.push(PanelState::new("unknown"));
        assert!(validate_layout(&invalid).is_err());
        invalid = default.clone();
        invalid.center.children.push(PanelState::new("Canvas"));
        assert!(validate_layout(&invalid).is_err());
        std::fs::write(dir.path().join("layout.json"), "{")?;
        assert!(store.read::<LayoutConfig>("layout.json").is_err());
        Ok(())
    }

    #[test]
    fn layout_writer_flushes_latest_state_on_shutdown_and_reports_io_errors() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let (writer, errors) = super::LayoutWriter::new(store.clone());
        writer.save(layout())?;
        let mut latest = layout();
        latest.center.children.remove(0);
        writer.save(latest.clone())?;
        drop(writer);
        assert_eq!(
            store.read::<LayoutConfig>("layout.json")?.unwrap().layout,
            latest
        );
        assert!(errors.try_recv().is_err());

        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "blocked")?;
        let (writer, errors) = super::LayoutWriter::new(ConfigStore::new(blocked));
        writer.save(layout())?;
        drop(writer);
        assert!(errors.try_recv().is_ok());
        Ok(())
    }
}
