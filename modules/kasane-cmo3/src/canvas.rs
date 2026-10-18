use crate::Cmo3Error;
use kasane_core::{types::Canvas, Document, DrawableFrame};

/// Use the same evaluated default-pose bounds as MOC3, including hidden meshes
/// and the full deformer hierarchy. This never mutates the authored canvas.
pub(crate) fn for_document(doc: &Document) -> Result<Canvas, Cmo3Error> {
    let space = doc.model_space();
    let mut frame = DrawableFrame::default();

    let status = kasane_core::evaluation::evaluate_frame_including_hidden(
        doc,
        &Default::default(),
        &mut frame,
    );
    if !status.is_ok() {
        return Err(Cmo3Error::InvalidDocument(status.message));
    }

    kasane_core::geometry::finite_export_canvas(
        space,
        doc.source_canvas(),
        frame
            .drawables
            .iter()
            .flat_map(|d| d.positions.iter().copied()),
    )
    .map_err(|status| Cmo3Error::InvalidDocument(status.message))
}
