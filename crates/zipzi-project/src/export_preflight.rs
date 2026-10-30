//! Opaque export guards shared with actual exporters. No file encoding, I/O,
//! provenance rewriting or implicit extension removal occurs during preflight.
use std::collections::BTreeSet;
use zipzi_core::{document::ConversionControl, Document, Status};

pub use zipzi_contract::project::{OpaqueExportIssue, OpaqueExportProblem};

pub(crate) trait OpaqueExportProblemCode {
    fn code(self) -> &'static str;
}
impl OpaqueExportProblemCode for OpaqueExportProblem {
    fn code(self) -> &'static str {
        match self {
            Self::NamespaceChanged => "OPAQUE_NAMESPACE_CHANGED",
            Self::ContentChanged => "OPAQUE_CONTENT_CHANGED",
            Self::ProvenanceMissing => "OPAQUE_PROVENANCE_MISSING",
        }
    }
}
pub fn preflight_opaque_exports(
    document: &Document,
    control: &ConversionControl,
) -> Result<BTreeSet<OpaqueExportIssue>, Status> {
    control.check()?;
    let mut issues = BTreeSet::new();
    if crate::model3::validate_settings_namespace(
        document.model3_settings(),
        &crate::model3::runtime_namespace(document),
    )
    .is_err()
    {
        issues.insert(OpaqueExportIssue::Model3NamespaceChanged);
    }
    issues.extend(
        crate::cdi::opaque_export_problems(document)
            .into_iter()
            .map(|problem| OpaqueExportIssue::Cdi { problem }),
    );
    for id in document.motion_order() {
        control.check()?;
        issues.extend(
            crate::motion::opaque_export_problems(document, document.get_motion(id).unwrap())
                .into_iter()
                .map(|problem| OpaqueExportIssue::Motion {
                    id: id.clone(),
                    problem,
                }),
        );
    }
    for id in document.expression_order() {
        control.check()?;
        issues.extend(
            crate::expression::opaque_export_problems(
                document,
                document.get_expression(id).unwrap(),
            )
            .into_iter()
            .map(|problem| OpaqueExportIssue::Expression {
                id: id.clone(),
                problem,
            }),
        );
    }
    if let Some(pose) = document.pose() {
        control.check()?;
        issues.extend(
            crate::pose::opaque_export_problems(document, pose)
                .into_iter()
                .map(|problem| OpaqueExportIssue::Pose {
                    id: pose.id.clone(),
                    problem,
                }),
        );
    }
    if let Some(physics) = document.physics() {
        control.check()?;
        issues.extend(
            crate::physics::opaque_export_problems(document, physics)
                .into_iter()
                .map(|problem| OpaqueExportIssue::Physics {
                    id: physics.id.clone(),
                    problem,
                }),
        );
    }
    control.check()?;
    Ok(issues)
}
