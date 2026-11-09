//! Document inspector visibility.
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
