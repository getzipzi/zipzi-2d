//! Explicit document extension values. Ordinary modeling fields use generated
//! concrete Rust constructors rather than a JSON intermediary.
use crate::*;
pub fn read_extension(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    use serde_json::Value;
    if value.is_none() {
        return Ok(Value::Null);
    }
    if value.is_instance_of::<pyo3::types::PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }
    if let Ok(v) = value.extract::<i64>() {
        return Ok(Value::from(v));
    }
    if let Ok(v) = value.extract::<u64>() {
        return Ok(Value::from(v));
    }
    if let Ok(v) = value.extract::<f64>() {
        return serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Non-finite extension"));
    }
    if let Ok(v) = value.extract::<String>() {
        return Ok(Value::String(v));
    }
    if value.hasattr("items")? {
        let values = value
            .call_method0("items")?
            .try_iter()?
            .map(|item| {
                let item = item?;
                Ok((
                    item.get_item(0)?.extract()?,
                    read_extension(&item.get_item(1)?)?,
                ))
            })
            .collect::<PyResult<_>>()?;
        return Ok(Value::Object(values));
    }
    Ok(Value::Array(
        value
            .try_iter()?
            .map(|v| read_extension(&v?))
            .collect::<PyResult<_>>()?,
    ))
}
pub fn write_extension(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => py.None(),
        Value::Bool(v) => v.into_pyobject(py)?.to_owned().into_any().unbind(),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else if let Some(v) = v.as_u64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else {
                v.as_f64().unwrap().into_pyobject(py)?.into_any().unbind()
            }
        }
        Value::String(v) => v.into_pyobject(py)?.into_any().unbind(),
        Value::Array(values) => {
            let out = PyList::empty(py);
            for v in values {
                out.append(write_extension(py, v)?)?;
            }
            out.into_any().unbind()
        }
        Value::Object(values) => {
            let out = PyDict::new(py);
            for (k, v) in values {
                out.set_item(k, write_extension(py, v)?)?;
            }
            out.into_any().unbind()
        }
    })
}

/// Walk borrowed extension values before allocating their explicitly selected
/// JSON representation. This is never used for ordinary modeling records.
pub fn measure_extension(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    if value.is_none()
        || value.is_instance_of::<pyo3::types::PyBool>()
        || value.is_instance_of::<pyo3::types::PyInt>()
        || value.is_instance_of::<pyo3::types::PyFloat>()
    {
        return Ok(32);
    }
    if value.is_instance_of::<pyo3::types::PyString>() {
        return Ok(value.len()?.saturating_mul(4).saturating_add(32));
    }
    let mut bytes = 128usize;
    if value.hasattr("items")? {
        for item in value.call_method0("items")?.try_iter()? {
            let item = item?;
            bytes = bytes
                .saturating_add(measure_extension(&item.get_item(0)?)?)
                .saturating_add(measure_extension(&item.get_item(1)?)?)
                .saturating_add(128);
        }
    } else {
        for item in value.try_iter()? {
            bytes = bytes.saturating_add(measure_extension(&item?)?);
        }
    }
    Ok(bytes)
}
