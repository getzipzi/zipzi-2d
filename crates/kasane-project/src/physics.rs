//! physics3 import and export with stable parameter bindings.
use std::collections::BTreeMap;

use kasane_core::document::PhysicsAsset;
use kasane_core::Document;
use kasane_live2d::physics3::{decode_physics3, encode_physics3};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

pub struct PhysicsImport {
    pub candidate: Document,
    pub diagnostics: Vec<PhysicsDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Physics {code} at {path}: {message}")]
pub struct PhysicsProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PhysicsProjectError {
    PhysicsProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn namespace(document: &Document, asset: &PhysicsAsset) -> BTreeMap<String, String> {
    asset
        .data
        .settings
        .iter()
        .flat_map(|g| {
            g.inputs
                .iter()
                .map(|v| &v.source)
                .chain(g.outputs.iter().map(|v| &v.destination))
        })
        .filter_map(|t| t.parameter_id())
        .filter_map(|id| {
            document
                .get_parameter(id)
                .map(|p| (id.to_owned(), p.runtime_id.clone()))
        })
        .collect()
}

fn content_hash(asset: &PhysicsAsset) -> String {
    let bytes = serde_json::to_vec(&asset.data).expect("physics content serializes");
    crate::content_sha256(&bytes)
}

pub fn import_physics3(
    document: &Document,
    id: &str,
    text: &str,
) -> Result<PhysicsImport, PhysicsProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let data = decode_physics3(text)
        .map_err(|failure| error(failure.code, failure.path, failure.message))?;
    let mut bindings = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (rig_index, rig) in data.settings.iter().enumerate() {
        for (index, input) in rig.inputs.iter().enumerate() {
            bind(
                document,
                &mut bindings,
                &mut diagnostics,
                &input.source.id,
                format!("$.PhysicsSettings[{rig_index}].Input[{index}].Source.Id"),
            );
        }
        for (index, output) in rig.outputs.iter().enumerate() {
            bind(
                document,
                &mut bindings,
                &mut diagnostics,
                &output.destination.id,
                format!("$.PhysicsSettings[{rig_index}].Output[{index}].Destination.Id"),
            );
        }
    }
    let mut asset =
        PhysicsAsset::from_wire(id.into(), data, |runtime| bindings.get(runtime).cloned())
            .map_err(|s| error(s.code, "$", s.message))?;
    // Unknown extensions cannot be remapped safely when model IDs change.
    if has_extensions(&asset) {
        asset.opaque_source_ids = Some(namespace(document, &asset));
        asset.opaque_source_content_hash = Some(content_hash(&asset));
    }
    let mut candidate = document.fork_candidate();
    let result = candidate.set_physics(asset);
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(PhysicsImport {
        candidate,
        diagnostics,
    })
}

fn bind(
    document: &Document,
    bindings: &mut BTreeMap<String, String>,
    diagnostics: &mut Vec<PhysicsDiagnostic>,
    runtime_id: &str,
    path: String,
) {
    if bindings.contains_key(runtime_id) {
        return;
    }
    if let Some(id) = document.parameter_order().iter().find(|id| {
        document
            .get_parameter(id)
            .is_some_and(|p| p.runtime_id == runtime_id)
    }) {
        bindings.insert(runtime_id.into(), id.clone());
    } else {
        diagnostics.push(PhysicsDiagnostic {
            code: "UNRESOLVED_PARAMETER".into(),
            path,
            message: format!("{runtime_id} is absent from the model"),
        });
    }
}

fn has_extensions(asset: &PhysicsAsset) -> bool {
    let data = &asset.data;
    !data.extensions.is_empty()
        || !data.meta.extensions.is_empty()
        || !data.meta.forces.extensions.is_empty()
        || data
            .meta
            .dictionary
            .iter()
            .any(|item| !item.extensions.is_empty())
        || data.settings.iter().any(|rig| {
            !rig.extensions.is_empty()
                || !rig.normalization.extensions.is_empty()
                || rig.inputs.iter().any(|item| {
                    !item.extensions.is_empty() || !item.reference_extensions.is_empty()
                })
                || rig.outputs.iter().any(|item| {
                    !item.extensions.is_empty() || !item.reference_extensions.is_empty()
                })
                || rig.vertices.iter().any(|item| !item.extensions.is_empty())
        })
}

pub fn export_physics3(document: &Document) -> Result<Option<String>, PhysicsProjectError> {
    let Some(asset) = document.physics() else {
        return Ok(None);
    };
    if has_extensions(asset) {
        if asset.opaque_source_ids.as_ref() != Some(&namespace(document, asset)) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "Parameter runtime IDs changed since import",
            ));
        }
        if asset.opaque_source_content_hash.as_deref() != Some(content_hash(asset).as_str()) {
            return Err(error(
                "OPAQUE_CONTENT_CHANGED",
                "$",
                "Physics content changed since import",
            ));
        }
    }
    for g in &asset.data.settings {
        for (id, target) in g
            .inputs
            .iter()
            .map(|v| (&v.id, &v.source))
            .chain(g.outputs.iter().map(|v| (&v.id, &v.destination)))
        {
            if let Some(reason) = target.disabled_reason() {
                return Err(error(
                    match reason {
                        kasane_core::document::PhysicsDisabledReason::NeedsConfiguration => {
                            "NEEDS_CONFIGURATION"
                        }
                        _ => "UNRESOLVED_PARAMETER",
                    },
                    id,
                    format!("{reason:?}"),
                ));
            }
        }
    }
    let data = asset.data.to_wire(|t| {
        document
            .get_parameter(t.parameter_id().expect("validated target"))
            .expect("valid parameter")
            .runtime_id
            .clone()
    });
    encode_physics3(&data)
        .map(Some)
        .map_err(|failure| error(failure.code, failure.path, failure.message))
}

/// Export only the channels accepted by the same projection used for preview.
/// Opaque payload protection is deliberately identical to strict export.
pub fn export_available_physics3(
    document: &Document,
) -> Result<Option<String>, PhysicsProjectError> {
    let Some(asset) = document.physics() else {
        return Ok(None);
    };
    if has_extensions(asset) {
        if asset.opaque_source_ids.as_ref() != Some(&namespace(document, asset)) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "parameter namespace changed",
            ));
        }
        if asset.opaque_source_content_hash.as_deref() != Some(content_hash(asset).as_str()) {
            return Err(error(
                "OPAQUE_CONTENT_CHANGED",
                "$",
                "physics content changed",
            ));
        }
    }
    let Some(mut projection) = document
        .physics_projection()
        .map_err(|s| error(s.code, "$", s.message))?
    else {
        return Ok(None);
    };
    if has_extensions(asset) && !projection.disabled.is_empty() {
        return Err(error(
            "OPAQUE_CONTENT_CHANGED",
            "$",
            "cannot omit opaque channels",
        ));
    }
    for g in &mut projection.executable.data.settings {
        let g = std::sync::Arc::make_mut(g);
        for v in &mut g.inputs {
            v.source.id = document
                .get_parameter(&v.source.id)
                .unwrap()
                .runtime_id
                .clone();
        }
        for v in &mut g.outputs {
            v.destination.id = document
                .get_parameter(&v.destination.id)
                .unwrap()
                .runtime_id
                .clone();
        }
    }
    encode_physics3(&projection.executable.data)
        .map(Some)
        .map_err(|e| error(e.code, e.path, e.message))
}
