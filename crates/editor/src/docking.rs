use super::*;
use gpui_kit::component::dock::*;
use serde::{Deserialize, Serialize};
use std::{sync::mpsc, thread, time::Duration};

/// Serialize persistence off the UI thread and coalesce a resize/drag burst.
/// Dropping the writer flushes the last layout before joining its worker.
struct LayoutWriter {
    sender: Option<mpsc::Sender<LayoutConfig>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl LayoutWriter {
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
            },
            error_receiver,
        )
    }
    fn save(&self, layout: DockAreaState) -> anyhow::Result<()> {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Layers,
    Properties,
}
pub struct ToolPanel {
    owner: WeakEntity<EditorView>,
    kind: PanelKind,
    focus: FocusHandle,
    _observe: Subscription,
}
impl ToolPanel {
    fn new(owner: WeakEntity<EditorView>, kind: PanelKind, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
            kind,
            focus: cx.focus_handle(),
        })
    }
}
impl EventEmitter<PanelEvent> for ToolPanel {}
impl Focusable for ToolPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for ToolPanel {
    fn panel_name(&self) -> &'static str {
        match self.kind {
            PanelKind::Layers => "Layers",
            PanelKind::Properties => "Properties",
        }
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for ToolPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self.kind {
            PanelKind::Layers => t!("editor.layers"),
            PanelKind::Properties => t!("editor.properties"),
        }
    }
}
impl Render for ToolPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner
            .update(cx, |owner, cx| {
                owner.render_tool_panel(self.kind, window, cx)
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}

pub struct DockWorkspace {
    pub area: Entity<DockArea>,
    pub canvas: Entity<crate::canvas::CanvasView>,
    layers: Entity<ToolPanel>,
    properties: Entity<ToolPanel>,
    pub last_valid: DockAreaState,
    writer: LayoutWriter,
    errors: Option<async_channel::Receiver<String>>,
}
#[derive(Serialize, Deserialize)]
pub struct LayoutConfig {
    pub layout: DockAreaState,
}
impl DockWorkspace {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Self {
        let canvas = crate::canvas::CanvasView::new(owner.clone(), window, cx);
        let layers = ToolPanel::new(owner.clone(), PanelKind::Layers, cx);
        let properties = ToolPanel::new(owner, PanelKind::Properties, cx);
        for (name, handle) in [
            ("Canvas", panel_handle(canvas.clone())),
            ("Layers", panel_handle(layers.clone())),
            ("Properties", panel_handle(properties.clone())),
        ] {
            register_panel(cx, name, move |_, _, _| handle.clone());
        }
        let (area, skin) = DockSkin::dock_area("kasane-workspace", Some(1), window, cx);
        skin.set_close_button_visible(true, cx);
        let last_valid = area.read(cx).dump(cx);
        let (writer, errors) = LayoutWriter::new(cx.global::<settings::Settings>().store.clone());
        let mut result = Self {
            area,
            canvas,
            layers,
            properties,
            last_valid,
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
                        DockLayout::tabs().panel_view(panel_handle(self.layers.clone()), cx),
                        Some(px(192.)),
                    )
                    .child(
                        DockLayout::tabs().panel_view(panel_handle(self.canvas.clone()), cx),
                        None,
                    )
                    .child(
                        DockLayout::tabs().panel_view(panel_handle(self.properties.clone()), cx),
                        Some(px(256.)),
                    ),
                window,
                cx,
            );
        });
        self.last_valid = self.area.read(cx).dump(cx);
    }
    pub fn toggle(&mut self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        let panel = match kind {
            PanelKind::Layers => self.layers.clone(),
            PanelKind::Properties => self.properties.clone(),
        };
        let exists = self
            .area
            .read(cx)
            .panel(PanelId::from(panel.entity_id()))
            .is_some();
        self.area.update(cx, |area, cx| {
            if exists {
                area.remove_panel(panel, window, cx);
            } else {
                area.add_panel_view(panel_handle(panel), DockPlacement::Right, None, window, cx);
            }
        });
    }
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
                    ["Canvas", "Layers", "Properties"].contains(&node.panel_name.as_str()),
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
    ensure!(state.version == Some(1), "Unsupported layout version");
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
    pub(super) fn init_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dock = DockWorkspace::new(cx.weak_entity(), window, cx);
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
        match store.read::<LayoutConfig>("layout.json") {
            Ok(Some(saved)) => {
                match validate_layout(&saved.layout).and_then(|_| {
                    dock.area
                        .update(cx, |area, cx| area.load(saved.layout, window, cx))
                }) {
                    Ok(()) => dock.last_valid = dock.area.read(cx).dump(cx),
                    Err(e) => {
                        self.message = e.to_string();
                        dock.reset(window, cx);
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
        self.dock = Some(dock);
    }
    pub(super) fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dock) = &mut self.dock {
            dock.reset(window, cx);
            if let Err(e) = dock.writer.save(dock.last_valid.clone()) {
                self.edit_error = Some(e.to_string());
            }
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
            version: Some(1),
            center: PanelState {
                panel_name: "StackPanel".into(),
                children: vec![
                    PanelState::new("Layers"),
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
