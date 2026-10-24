use super::*;
use zipzi_core::document::DocumentField;
pub(super) fn summarize_document(before: &Document, after: &Document, out: &mut ChangeSet) {
    let a = before.model3_settings();
    let b = after.model3_settings();
    let fields = [
        (DocumentField::Layout, a.layout != b.layout),
        (
            DocumentField::ExportSettings,
            before.export_settings() != after.export_settings(),
        ),
        (
            DocumentField::SourceCanvas,
            before.source_canvas() != after.source_canvas(),
        ),
        (
            DocumentField::MissingAttachments,
            before.missing_attachments() != after.missing_attachments(),
        ),
        (
            DocumentField::DisplayInfo,
            before.display_info() != after.display_info(),
        ),
        (
            DocumentField::OpaqueMetadata,
            (&a.extensions, &a.source_runtime_ids, &a.source_content)
                != (&b.extensions, &b.source_runtime_ids, &b.source_content),
        ),
    ];
    let x = before.display_info();
    let y = after.display_info();
    let group_labels_only = x.origin == y.origin
        && x.parameters == y.parameters
        && x.parts == y.parts
        && x.combined_parameters == y.combined_parameters
        && x.extensions == y.extensions
        && x.opaque_source_ids == y.opaque_source_ids
        && match (&x.parameter_groups, &y.parameter_groups) {
            (Some(a), Some(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b).all(|(a, b)| {
                        a.id == b.id
                            && a.runtime_id == b.runtime_id
                            && a.parent_id == b.parent_id
                            && a.extensions == b.extensions
                    })
            }
            (None, None) => true,
            _ => false,
        };
    for (field, changed) in fields {
        if changed {
            out.document_fields.push(field);
            if field != DocumentField::DisplayInfo || !group_labels_only {
                out.categories.insert(ChangeCategory::Content);
            }
        }
    }
}
