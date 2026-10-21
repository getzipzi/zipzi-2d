//! Compact executable entries retain their native identities in both directions.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionDisabledReason {
    NeedsConfiguration,
    UnresolvedParameter { runtime_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledExpressionEntry {
    pub entry_id: String,
    pub reason: ExpressionDisabledReason,
}
#[derive(Debug, Clone)]
pub struct ExpressionProjection {
    pub revision: u64,
    pub entries: Vec<ExpressionEntry>,
    pub native_to_runtime: BTreeMap<String, Option<usize>>,
    pub runtime_to_native: Vec<String>,
    pub disabled: Vec<DisabledExpressionEntry>,
}
impl Document {
    pub fn expression_projection(&self, id: &str) -> Result<ExpressionProjection, Status> {
        let asset = self
            .get_expression(id)
            .ok_or_else(|| Status::error("MISSING_EXPRESSION", id))?;
        let mut result = ExpressionProjection {
            revision: self.revision(),
            entries: vec![],
            native_to_runtime: BTreeMap::new(),
            runtime_to_native: vec![],
            disabled: vec![],
        };
        for entry in &asset.entries {
            let reason = match &entry.target {
                ExpressionTarget::Unspecified => Some(ExpressionDisabledReason::NeedsConfiguration),
                ExpressionTarget::Unresolved { runtime_id } => {
                    Some(ExpressionDisabledReason::UnresolvedParameter {
                        runtime_id: runtime_id.clone(),
                    })
                }
                ExpressionTarget::Resolved { parameter_id } => {
                    if self.get_parameter(parameter_id).is_none() {
                        return Err(Status::error("MISSING_PARAMETER", &entry.id));
                    }
                    None
                }
            };
            if let Some(reason) = reason {
                result.native_to_runtime.insert(entry.id.clone(), None);
                result.disabled.push(DisabledExpressionEntry {
                    entry_id: entry.id.clone(),
                    reason,
                });
            } else {
                result
                    .native_to_runtime
                    .insert(entry.id.clone(), Some(result.entries.len()));
                result.runtime_to_native.push(entry.id.clone());
                result.entries.push(entry.clone());
            }
        }
        Ok(result)
    }
}

impl ExpressionProjection {
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + super::super::checkpoint::expression_entries_bytes(&self.entries)
            + self
                .runtime_to_native
                .iter()
                .map(|id| std::mem::size_of::<String>() + id.capacity())
                .sum::<usize>()
            + self
                .native_to_runtime
                .keys()
                .map(|id| 64 + id.capacity())
                .sum::<usize>()
            + self
                .disabled
                .iter()
                .map(|e| {
                    std::mem::size_of::<DisabledExpressionEntry>()
                        + e.entry_id.capacity()
                        + match &e.reason {
                            ExpressionDisabledReason::UnresolvedParameter { runtime_id } => {
                                runtime_id.capacity()
                            }
                            _ => 0,
                        }
                })
                .sum::<usize>()
    }
}
