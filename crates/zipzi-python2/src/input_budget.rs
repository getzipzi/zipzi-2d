//! Borrowed Python sizing precedes conversion into owned Rust input. Estimates
//! cover Vec capacity growth, strings, maps and conversion temporaries.
use crate::*;
pub fn reserve(
    runtime: &NativeRuntime,
    py: Python<'_>,
    bytes: usize,
    edit: Option<EditId>,
) -> std::result::Result<zipzi_runtime::TransferReservation, (HostError, Option<EditState>)> {
    let client = runtime.client;
    match py.detach(|| {
        runtime.host.call(move |rt| {
            let state = if let Some(edit) = edit {
                Some(rt.edit_state(client, edit)?)
            } else {
                None
            };
            Ok((rt.reserve_transfer(bytes), state))
        })
    }) {
        Ok((Ok(lease), _)) => Ok(lease),
        Ok((Err(error), state)) => Err((HostError::Runtime(error), state)),
        Err(error) => Err((error, None)),
    }
}
pub fn refusal(
    py: Python<'_>,
    error: HostError,
    state: Option<EditState>,
    operation: &str,
) -> PyResult<Py<PyDict>> {
    let response = reply::<()>(py, Err(error), operation, |()| Ok(py.None().into_bound(py)))?;
    if let Some(state) = state {
        if let Some(error) = response.bind(py).get_item("error")? {
            error.set_item(
                "edit_state",
                if state == EditState::Editing {
                    "editing"
                } else {
                    "aborted"
                },
            )?;
        }
    }
    Ok(response)
}
macro_rules! input_lease {
    ($runtime:expr,$py:expr,$bytes:expr,$operation:expr,$edit:expr) => {
        match crate::input_budget::reserve($runtime, $py, $bytes, $edit) {
            Ok(lease) => lease,
            Err((error, state)) => {
                return crate::input_budget::refusal($py, error, state, $operation)
            }
        }
    };
}
pub(crate) use input_lease;
pub fn text(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    Ok(if value.is_none() {
        0
    } else {
        value.len()?.saturating_mul(4).saturating_add(32)
    })
}
fn attr_text(value: &Bound<'_, PyAny>, name: &str) -> PyResult<usize> {
    text(&value.getattr(name)?)
}
fn geometry(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    let meshing = value.getattr("meshing")?;
    let meshing_bytes = if meshing.is_none() {
        0
    } else {
        256usize
            .saturating_add(meshing.getattr("boundary")?.len()?.saturating_mul(8))
            .saturating_add(meshing.getattr("constraints")?.len()?.saturating_mul(512))
    };
    Ok(256usize
        .saturating_add(value.getattr("vertex_ids")?.len()?.saturating_mul(8))
        .saturating_add(value.getattr("positions")?.len()?.saturating_mul(32))
        .saturating_add(value.getattr("uvs")?.len()?.saturating_mul(32))
        .saturating_add(value.getattr("triangles")?.len()?.saturating_mul(48))
        .saturating_add(meshing_bytes))
}
pub fn operations(values: &Bound<'_, PyAny>) -> PyResult<usize> {
    if values.len()? > 1024 {
        return Ok(usize::MAX);
    }
    let mut bytes = 4096usize;
    for op in values.try_iter()? {
        let op = op?;
        let kind = op.getattr("kind")?.extract::<String>()?;
        let size = match kind.as_str() {
            "modeling" => generated::modeling::measure_modelingcommand(&op)?,
            value if value.starts_with("scene_") => generated::scene::measure_operation(&op)?,
            "create" => {
                let spec = op.getattr("spec")?;
                geometry(&spec.getattr("geometry")?)?
                    .saturating_add(attr_text(&spec, "name")?)
                    .saturating_add(attr_text(&spec, "id")?)
                    .saturating_add(attr_text(&spec, "texture_asset_id")?)
            }
            "rename" => attr_text(&op, "id")?
                .saturating_add(attr_text(&op, "name")?)
                .saturating_add(attr_text(&op, "expected_name")?),
            "set_positions" => attr_text(&op, "id")?
                .saturating_add(op.getattr("vertex_ids")?.len()?.saturating_mul(8))
                .saturating_add(op.getattr("positions")?.len()?.saturating_mul(32)),
            "create_parameter" | "patch_parameter" => {
                generated::modeling::measure_parameteroperation(&op)?
            }
            "create_mesh_binding" | "set_mesh_keyform" => {
                generated::modeling::measure_meshbindingoperation(&op)?
            }
            "create_image" | "replace_image" | "rename_image" | "apply_plan" => {
                generated::resources::measure_resourceoperation(&op)?
            }
            _ => return Err(pyo3::exceptions::PyTypeError::new_err("Unknown operation")),
        };
        bytes = bytes.saturating_add(size).saturating_add(256);
    }
    Ok(bytes)
}
pub fn strings(value: Option<&Bound<'_, PyAny>>) -> PyResult<usize> {
    let Some(value) = value else { return Ok(0) };
    let mut bytes = 128usize;
    for item in value.try_iter()? {
        bytes = bytes.saturating_add(text(&item?)?).saturating_add(32);
    }
    Ok(bytes)
}
pub fn poses(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    let mut bytes = 128usize;
    for pose in value.try_iter()? {
        let pose = pose?;
        bytes = bytes.saturating_add(128);
        for item in pose.call_method0("items")?.try_iter()? {
            bytes = bytes
                .saturating_add(text(&item?.get_item(0)?)?)
                .saturating_add(128);
        }
    }
    Ok(bytes)
}
