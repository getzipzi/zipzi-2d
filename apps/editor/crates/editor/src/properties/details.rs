//! Generic detail descriptors and caching. Context providers supply the content.
use super::*;
use gpui_kit::ElementId;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum DetailRow {
    Hint(SharedString),
    Heading(SharedString),
    Text {
        id: Option<ElementId>,
        value: SharedString,
    },
    Value {
        id: ElementId,
        label: SharedString,
        value: SharedString,
        stacked: bool,
    },
}
impl DetailRow {
    #[allow(dead_code)]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn id(&self) -> Option<ElementId> {
        match self {
            Self::Text { id, .. } => id.clone(),
            Self::Value { id, .. } => Some(id.clone()),
            _ => None,
        }
    }

    pub(super) fn property(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self::Value {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            stacked: false,
        }
    }
    pub(super) fn field(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self::Value {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            stacked: true,
        }
    }
    pub(super) fn text(value: impl Into<SharedString>) -> Self {
        Self::Text {
            id: None,
            value: value.into(),
        }
    }
}

pub(super) struct DetailSource {
    pub dependencies: Vec<Option<String>>,
    pub build: fn(&Editor, Option<&PropertyTarget>) -> Vec<DetailRow>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DetailKey {
    document: editor_core::DocumentId,
    revision: u64,
    context: EditingContext,
    dependencies: Vec<Option<String>>,
    locale: String,
}
#[derive(Default)]
pub(crate) struct Details {
    key: Option<DetailKey>,
    rows: Arc<[DetailRow]>,
    pub builds: u64,
}
impl Details {
    pub fn key(editor: &Editor, target: Option<&PropertyTarget>) -> Option<DetailKey> {
        Some(DetailKey {
            document: editor.active_id()?,
            revision: editor.document()?.properties_revision(),
            context: editor.editing_context()?,
            dependencies: super::details_source(editor, target)?.dependencies,
            locale: rust_i18n::locale().to_string(),
        })
    }

    pub fn rows(&mut self, editor: &Editor, target: Option<&PropertyTarget>) -> Arc<[DetailRow]> {
        let Some(source) = super::details_source(editor, target) else {
            return Arc::from([]);
        };
        let (Some(document), Some(id), Some(context)) = (
            editor.document(),
            editor.active_id(),
            editor.editing_context(),
        ) else {
            return Arc::from([]);
        };
        let key = DetailKey {
            document: id,
            revision: document.properties_revision(),
            context,
            dependencies: source.dependencies,
            locale: rust_i18n::locale().to_string(),
        };
        if self.key.as_ref() != Some(&key) {
            self.rows = (source.build)(editor, target).into();
            self.key = Some(key);
            self.builds += 1;
        }
        self.rows.clone()
    }
}
