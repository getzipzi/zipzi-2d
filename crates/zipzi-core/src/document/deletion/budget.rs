use super::*;

pub(super) struct DeletionBudget {
    workspace: usize,
    retained: usize,
}
impl DeletionBudget {
    pub fn new(
        document: &Document,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<Self, Status> {
        let overflow = || Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion workspace");
        // Candidate, publication and history metadata; source/result relation
        // graphs coexist during final validation. Shared geometry stays shared.
        let metadata = document
            .estimated_snapshot_bytes()
            .checked_mul(4)
            .and_then(|n| n.checked_add(document.estimated_authoring_index_bytes()))
            .ok_or_else(overflow)?;
        let retained = document.estimated_content_bytes();
        limits.check_cost(ConversionCost {
            temporary_bytes: metadata,
            history_bytes: retained.checked_mul(2).ok_or_else(overflow)?,
            ..Default::default()
        })?;
        let index = index::DeletionIndex::admission_bytes(
            document,
            control,
            (limits.max_conversion_bytes - metadata) / 2,
        )?;
        let workspace = index
            .checked_mul(2)
            .and_then(|n| metadata.checked_add(n))
            .ok_or_else(overflow)?;
        Ok(Self {
            workspace,
            retained,
        })
    }
    pub fn check(
        &self,
        document: &Document,
        report: &DeletionReport,
        conversions: ConversionCost,
        limits: AuthoringLimits,
    ) -> Result<ConversionCost, Status> {
        let overflow = || Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion plan size");
        let payload = document
            .estimated_deletion_write_bytes(report)
            .checked_add(report.estimated_bytes())
            .and_then(|n| n.checked_add(conversions.payload_bytes))
            .ok_or_else(overflow)?;
        let temporary = payload
            .checked_mul(2)
            .and_then(|n| self.workspace.checked_add(n))
            .and_then(|n| n.checked_add(conversions.temporary_bytes))
            .ok_or_else(overflow)?;
        let history = self
            .retained
            .checked_mul(2)
            .and_then(|n| n.checked_add(payload))
            .ok_or_else(overflow)?;
        limits.check_cost(ConversionCost {
            sample_combinations: conversions.sample_combinations,
            payload_bytes: payload,
            temporary_bytes: temporary,
            history_bytes: history,
        })
    }
}
impl Document {
    /// Budget a deletion report that still needs choices or is blocked. Shares
    /// the same admission model as executable plans and includes review storage.
    pub fn deletion_report_cost(
        &self,
        report: &DeletionReport,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ConversionCost, Status> {
        DeletionBudget::new(self, limits, control)?.check(
            self,
            report,
            ConversionCost::default(),
            limits,
        )
    }
}
