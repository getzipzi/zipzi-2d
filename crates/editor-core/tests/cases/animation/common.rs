use super::super::support;
use editor_core::*;

pub(super) fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (document, assets) = support::animation_document()?;
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    Ok((editor, assets))
}

pub(super) fn select(editor: &mut Editor, index: usize) -> anyhow::Result<()> {
    let selection = editor.document().unwrap().motion_resources()[index]
        .selection
        .clone();
    editor.select_motion(editor.active_id().unwrap(), selection)
}

pub(super) fn expression(editor: &mut Editor, index: usize) -> anyhow::Result<()> {
    let id = editor.document().unwrap().expression_resources()[index]
        .id
        .clone();
    editor.select_expression(editor.active_id().unwrap(), id)
}

pub(super) fn evaluate(
    editor: &mut Editor,
    evaluator: &mut AnimationEvaluator,
) -> anyhow::Result<AnimationEvaluated> {
    evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)
}
