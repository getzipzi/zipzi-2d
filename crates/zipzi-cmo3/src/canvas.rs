use crate::Cmo3Error;
use zipzi_core::{types::Canvas, Document, DrawableFrame};

/// Use the same evaluated default-pose bounds as MOC3, including hidden meshes
/// and the full deformer hierarchy. This never mutates the authored canvas.
pub(crate) fn for_document(doc: &Document) -> Result<Canvas, Cmo3Error> {
    let mut frame = DrawableFrame::default();

    let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
        doc,
        &Default::default(),
        &mut frame,
    );
    if !status.is_ok() {
        return Err(Cmo3Error::InvalidDocument(status.message));
    }

    zipzi_core::geometry::finite_export_canvas(
        doc.export_settings(),
        frame
            .drawables
            .iter()
            .flat_map(|d| d.positions.iter().copied()),
    )
    .map_err(|status| Cmo3Error::InvalidDocument(status.message))
}
