//! Default layouts, restoration, switching and structural validation.
use super::*;

#[derive(Serialize, Deserialize)]
pub struct LayoutConfig {
    pub workspace: WorkspaceKind,
    pub layout: DockAreaState,
}

pub(super) fn layout_file(workspace: WorkspaceKind) -> &'static str {
    match workspace {
        WorkspaceKind::Modeling => "layout-modeling.json",
        WorkspaceKind::Animation => "layout-animation.json",
    }
}

pub(super) fn layout_version(workspace: WorkspaceKind) -> usize {
    match workspace {
        WorkspaceKind::Modeling => 5,
        // Three columns above a full-width timeline.
        WorkspaceKind::Animation => 10,
    }
}

pub(super) fn timeline_height(window: &Window) -> Pixels {
    ((window.viewport_size().height - px(98.)).max(px(0.)) * 0.44).clamp(px(180.), px(320.))
}

impl DockWorkspace {
    pub fn reset(&mut self, window: &mut Window, cx: &mut App) {
        let animation = self.workspace == WorkspaceKind::Animation;
        let viewport = window.viewport_size();
        // Window chrome: document tabs (34), workspace toolbar (40), status (24).
        let content_height = (viewport.height - px(98.)).max(px(0.));
        let content_height = if animation {
            (content_height - timeline_height(window)).max(px(0.))
        } else {
            content_height
        };
        let column = |region, cx: &App| {
            let mut groups = std::collections::BTreeMap::<usize, DockLayout>::new();
            let mut specs: Vec<_> = PANELS
                .iter()
                .filter(|spec| {
                    spec.available(self.workspace) && spec.default_visible && spec.region == region
                })
                .collect();
            specs.sort_by_key(|spec| (spec.group, animation && spec.kind != PanelKind::Properties));
            for spec in specs {
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
                    .into_iter()
                    .fold(DockLayout::v_split(), |column, (index, group)| {
                        let size = if animation && index == 0 {
                            match region {
                                PanelRegion::Left => Some(content_height * 0.56),
                                PanelRegion::Right => Some(content_height * 0.48),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        column.child(group, size)
                    })
            }
        };
        let (left, right) = if animation {
            (viewport.width * 0.28, viewport.width * 0.32)
        } else {
            (px(360.), px(360.))
        };
        let layout = DockLayout::h_split()
            .child(column(PanelRegion::Left, cx), Some(left))
            .child(
                column(PanelRegion::Center, cx),
                animation.then_some(viewport.width * 0.40),
            )
            .child(column(PanelRegion::Right, cx), Some(right));
        let bottom = PANELS
            .iter()
            .any(|spec| {
                spec.available(self.workspace)
                    && spec.default_visible
                    && spec.region == PanelRegion::Bottom
            })
            .then(|| column(PanelRegion::Bottom, cx));
        let version = layout_version(self.workspace);
        let bottom_height = timeline_height(window);
        self.area.update(cx, |area, cx| {
            area.set_version(Some(version), cx);
            area.set_zoomed_out(window, cx);
            for placement in REGIONS.into_iter().skip(1) {
                area.remove_dock(placement, window, cx);
            }
            area.set_center(layout, window, cx);
            if let Some(bottom) = bottom {
                area.set_dock(DockPlacement::Bottom, bottom, window, cx);
                area.set_dock_size(DockPlacement::Bottom, bottom_height, window, cx);
            }
        });
        self.last_valid = self.area.read(cx).dump(cx);
    }

    pub(super) fn restore(&mut self, window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
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

    pub(super) fn switch(
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
                Ok(usize::from(kind == PanelKind::primary_viewport(workspace)))
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
    ensure!(
        state.version == Some(layout_version(workspace)),
        "Unsupported layout version"
    );
    let mut seen = std::collections::HashSet::new();
    let mut canvases = walk(&state.center, &mut seen, 0, workspace)?;
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
        canvases += walk(dock.panel(), &mut seen, 0, workspace)?;
    }
    // Product workspaces declare one required viewport kind. Simultaneous
    // instances are exercised by the test scene without expanding product layouts.
    ensure!(canvases == 1, "Exactly one primary viewport is required");
    Ok(())
}
