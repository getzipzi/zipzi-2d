use super::*;
use std::io::{self, Write};

// Conservative admission estimate for Value nodes, map entries, strings, the
// retained response clone and temporary encoding. This is not an RSS limit.
pub(super) const JSON_MEMORY_FACTOR: usize = 128;

struct LimitedCounter(usize);
impl Write for LimitedCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("modeling query result budget exceeded"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn response(
    doc: &Document,
    q: &zipzi_sdk::ModelingQuery,
    source: SourceRef,
    session: &SessionInfo,
    available: usize,
    retention_limit: usize,
) -> Result<Value> {
    let budget_error = || RuntimeError::Budget("modeling query result");
    // Admit the borrowed record indexes themselves before constructing them.
    let selections = [
        q.assets.len(),
        q.meshes.len(),
        q.parameters.len(),
        q.parts.len(),
        q.transforms.len(),
        q.bindings.len(),
        q.scene_bindings.len(),
        q.blend_key_tables.len(),
        q.blend_constraints.len(),
        q.blend_bindings.len(),
        q.glues.len(),
        q.offscreens.len(),
        q.parameter_groups.len(),
        q.attachments.len(),
    ];
    let identities = [
        doc.asset_order().len(),
        doc.mesh_order().len(),
        doc.parameter_order().len(),
        doc.part_order().len(),
        doc.transform_order().len(),
        doc.binding_order().len(),
        doc.scene_binding_order().len(),
        doc.blend_key_table_order().len(),
        doc.blend_constraint_order().len(),
        doc.blend_binding_order().len(),
        doc.glue_order().len(),
        doc.offscreen_order().len(),
        doc.display_info()
            .parameter_groups
            .as_ref()
            .map_or(0, Vec::len),
        doc.package_attachments().len(),
    ];
    let index_bytes = selections
        .into_iter()
        .chain(identities)
        .fold(0usize, usize::saturating_add)
        .saturating_mul(2 * size_of::<&str>())
        .saturating_add(4096);
    let remaining = available
        .checked_sub(index_bytes)
        .ok_or_else(budget_error)?;
    // Reserve the success envelope and retention metadata too.
    let limit = (remaining / JSON_MEMORY_FACTOR)
        .min(MAX_FRAME)
        .min(retention_limit)
        .checked_sub(128)
        .ok_or_else(budget_error)?;
    #[derive(Serialize)]
    struct Response<'a> {
        source: SourceRef,
        version: Version,
        state: &'static str,
        records: zipzi_sdk::ModelingRecords<'a>,
    }
    let response = Response {
        source,
        version: match source {
            SourceRef::Formal { version, .. } => version,
            SourceRef::Edit { base, .. } => base,
        },
        state: match session.state {
            SessionState::Ready => "ready",
            SessionState::Busy(_) => "busy",
            SessionState::Faulted(_) => "faulted",
            SessionState::Closing => "closing",
            SessionState::Closed => "closed",
        },
        records: zipzi_sdk::modeling_records(doc, q),
    };
    // Stream borrowed records into a counter; stop at the cap without allocating
    // expanded JSON, including for repeated IDs and oversized single records.
    serde_json::to_writer(LimitedCounter(limit), &response).map_err(|_| budget_error())?;
    serde_json::to_value(response).map_err(|_| RuntimeError::InvalidReference)
}
