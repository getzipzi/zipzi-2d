//! Surface declarations, rendered hitboxes and serial-checked capture cleanup.
use super::{InputTarget, LocalTarget};
use crate::*;
use std::{cell::RefCell, rc::Rc};

/// Pointer editing can reserve Escape; viewport tools use the configured command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureCancelKey {
    Escape,
    CommandBinding,
}

type TargetResolver = dyn Fn(&Editor, &App) -> Option<InputTarget>;
type CaptureResolver = dyn Fn(&Editor, &App) -> Option<CapturedInput>;
type CancelCapture = dyn FnOnce(&mut Window, &mut App) -> Option<CoreCleanup>;
pub(crate) type CoreCleanup = Box<dyn FnOnce(&mut EditorView)>;

/// A snapshot, never another stored owner. Local cleanup compares the snapshot
/// with the owner's current operation before returning domain cleanup to the root.
pub(crate) struct CapturedInput {
    pub target: LocalTarget,
    pub(super) valid: bool,
    pub(super) cancel_key: CaptureCancelKey,
    cancel: Box<CancelCapture>,
}
impl CapturedInput {
    pub fn new(
        target: LocalTarget,
        valid: bool,
        cancel_key: CaptureCancelKey,
        cancel: impl FnOnce(&mut Window, &mut App) -> Option<CoreCleanup> + 'static,
    ) -> Self {
        Self {
            target,
            valid,
            cancel_key,
            cancel: Box::new(cancel),
        }
    }
    pub(super) fn first_valid(captures: impl IntoIterator<Item = Self>) -> Option<Self> {
        captures.into_iter().find(|capture| capture.valid)
    }
    pub(super) fn cancel(self, owner: &mut EditorView, window: &mut Window, cx: &mut App) {
        if let Some(cleanup) = (self.cancel)(window, cx) {
            cleanup(owner);
        }
    }
}

#[derive(Clone)]
pub(crate) struct InputRegion {
    target: Rc<TargetResolver>,
    hitbox: Rc<RefCell<Option<Hitbox>>>,
    focus: FocusHandle,
    capture: Option<Rc<CaptureResolver>>,
}
impl InputRegion {
    pub fn new(
        focus: FocusHandle,
        target: impl Fn(&Editor, &App) -> Option<InputTarget> + 'static,
    ) -> Self {
        Self {
            target: Rc::new(target),
            focus,
            hitbox: Default::default(),
            capture: None,
        }
    }
    pub fn panel(focus: FocusHandle, kind: editor_core::PanelKind) -> Self {
        Self::new(focus, move |_, _| {
            Some(InputTarget::new(LocalTarget::Panel(kind)))
        })
    }
    pub(super) fn resolve(&self, editor: &Editor, cx: &App) -> Option<InputTarget> {
        (self.target)(editor, cx)
    }
    pub fn with_capture(
        mut self,
        capture: impl Fn(&Editor, &App) -> Option<CapturedInput> + 'static,
    ) -> Self {
        self.capture = Some(Rc::new(capture));
        self
    }
    pub fn marker(&self) -> impl IntoElement {
        let region = self.clone();
        canvas(
            move |bounds, window, _| {
                *region.hitbox.borrow_mut() =
                    Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));
            },
            |_, _, _, _| (),
        )
        .absolute()
        .inset_0()
        .size_full()
    }
    pub(crate) fn hovered(&self, window: &Window) -> bool {
        self.hitbox
            .borrow()
            .as_ref()
            .is_some_and(|hitbox| hitbox.is_hovered_at(window.mouse_position(), window))
    }
    pub(super) fn owns_keyboard(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    pub(super) fn captured(&self, editor: &Editor, cx: &App) -> Option<CapturedInput> {
        self.capture
            .as_ref()
            .and_then(|capture| capture(editor, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{CaptureCancelKey, CapturedInput, LocalTarget};
    use editor_core::PanelKind;
    use std::cell::Cell;

    #[test]
    fn snapshot_skips_stale_captures_and_keeps_only_the_first_valid_owner() {
        let resolved = Cell::new(0);
        let captures = [
            (PanelKind::MeshTree, false, CaptureCancelKey::Escape),
            (
                PanelKind::AnimationTimeline,
                true,
                CaptureCancelKey::CommandBinding,
            ),
            (PanelKind::Properties, true, CaptureCancelKey::Escape),
        ]
        .into_iter()
        .map(|(panel, valid, cancel_key)| {
            resolved.set(resolved.get() + 1);
            CapturedInput::new(LocalTarget::Panel(panel), valid, cancel_key, |_, _| {
                panic!("collecting facts must not cancel captures")
            })
        });
        let capture = CapturedInput::first_valid(captures).unwrap();
        assert_eq!(resolved.get(), 2);
        assert_eq!(
            capture.target,
            LocalTarget::Panel(PanelKind::AnimationTimeline)
        );
        assert!(capture.cancel_key == CaptureCancelKey::CommandBinding);
        assert!(
            CapturedInput::first_valid([CapturedInput::new(
                capture.target,
                false,
                CaptureCancelKey::Escape,
                |_, _| None,
            )])
            .is_none()
        );
    }
}
