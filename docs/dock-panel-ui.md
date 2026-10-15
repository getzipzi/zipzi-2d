# Dock Panel UI

Dock groups use one 26 logical px header at 100% UI scale instead of a horizontal
tab strip and a separate panel toolbar. The first control shows the active panel's
icon. Groups with multiple visible panels offer a selection-only dropdown with
names, icons and the current selection. The tab and unused header space support
dragging; right-clicking the header opens add, split, close and maximize actions.
Each group has a thin outline, rounded corners and a shared 4 logical px gap.

The rest of the header contains the active panel's controls, followed by dock
actions. Panel names remain available in the selector's tooltip and menu.
The content region has the same padding whether it holds one or several
panels. Bottom docks retain Kit's fixed 29 px collapsed extent; a 27 px header
and 2 px top inset fit inside that strip. Selecting a panel reopens it before
changing selection. The expanded header and controls scale together.

## Header ownership

`docking/header.rs` defines `PanelHeader` and an observed header view keyed by
the panel entity's identity. Each panel builds its own controls with its own
`Context`, so existing listeners, input entities, filters and popovers retain
their owners. The header observes the panel to refresh selected and disabled
states without rebuilding the whole dock.

Hierarchy trees, Parameters, animation resources, preview information,
Relations, Canvas and Timeline expose their controls through this interface. Their
content renderers no longer draw a second toolbar row. `PanelToolbar` supplies
the shared compact spacing and search/popover controls.

When there is insufficient header width, an observed header replaces its
inline controls with a tools popover. The panel's input state survives this
change. Panels can specify their minimum inline width and respond to actual
header width separately from content width; Timeline uses this for its
transport and overflow tools. Its compact time label omits the duration while
the tooltip retains the complete time information.

Header controls isolate left mouse presses after their menus and popovers handle
them. This preserves search text selection and button interaction without arming
the header's panel drag. The isolation belongs around composed controls, not on
popover triggers, because the parent popover needs the press to open.

## Canvas

Canvas uses the same header and selector and can be grouped, moved to a side
dock or maximized. Closing the only canvas remains prohibited. A canvas's
editing tools live in a vertical floating toolbar at its left edge; zoom controls
belong to the canvas header and align to its right edge. Deformer
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
