# Dock Panel UI

Dock groups use one 28 px header instead of a horizontal tab strip and a
separate panel toolbar. The first control shows the active panel's icon and
opens a menu of the group's visible panels. Entries include names, icons and
the current selection. The same trigger supports keyboard activation, dragging
the active panel, and its close context menu. The group menu retains add,
split, close and maximize actions.

The rest of the header contains the active panel's controls, followed by dock
actions. Panel names remain available in the selector's tooltip and menu.
The content region has the same padding whether it holds one or several
panels. Bottom docks retain Kit's 29 px collapsed extent; the 28 px header fits
inside that strip and selecting a panel reopens it before changing selection.

## Header ownership

`docking/header.rs` defines `PanelHeader` and an observed header view keyed by
the panel entity's identity. Each panel builds its own controls with its own
`Context`, so existing listeners, input entities, filters and popovers retain
their owners. The header observes the panel to refresh selected and disabled
states without rebuilding the whole dock.

Hierarchy trees, Parameters, animation resources, preview information,
Relations and Timeline expose their controls through this interface. Their
content renderers no longer draw a second toolbar row. `PanelToolbar` supplies
the shared compact spacing and search/popover controls.

When there is insufficient header width, an observed header replaces its
inline controls with a tools popover. The panel's input state survives this
change. Panels can specify their minimum inline width and respond to actual
header width separately from content width; Timeline uses this for its
transport and overflow tools. Its compact time label omits the duration while
the tooltip retains the complete time information.

## Canvas

Canvas uses the same header and selector and can be grouped, moved to a side
dock or maximized. Closing the only canvas remains prohibited. A canvas's
editing tools live in a vertical floating toolbar at its left edge. Deformer
controls sit beside it. Pointer events on the floating controls do not start
canvas gestures, and activating a tool focuses that canvas.

`CanvasPanels` stores view instances by entity identity and tracks the active
canvas. Commands obtain `active_canvas()`; window/document lifecycle changes
and invalidation iterate `canvas_views()`. Layout menu targets are resolved
across all dock regions rather than assuming a central canvas.

This creates the UI access points for future multiple canvases, but the current
application still creates and validates exactly one canvas. Enabling additional
instances requires:

- Persistent instance identifiers and factories that restore each instance,
  rather than returning a singleton for the Canvas panel kind.
- Viewport size, pan, zoom, presented-frame identity and gesture ownership per
  canvas, separated from shared document and editing history.
- Render requests, coalescing and presentation per canvas, with shared scene
  evaluation where appropriate.
- Explicit document binding and active-canvas routing for tool panels.

## Verification

Headless coverage exercises selector selection and closing, drag between
groups, reopening a collapsed dock, controls within the header, search state
across panel switches, narrow-header popovers, canvas grouping, layout
restoration and the floating toolbar. Existing hierarchy, parameter, timeline,
focus and cache-boundary regression tests remain applicable.

Real Metal screenshots are captured by `headless_screenshot`; use
`KASANE_SCREENSHOT_SCENE=timeline` for narrow Timeline views and
`KASANE_SCREENSHOT_SCENE=dock` for selector and compact-header views.
