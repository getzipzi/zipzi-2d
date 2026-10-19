//! Import provenance and output policy are exposed by import/export, not the inspector.
use super::*;
pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    show_readonly: |field| {
        !matches!(
            field.split('.').next(),
            Some("export_settings" | "source_canvas")
        )
    },
    ..PropertyProvider::DEFAULT
};
