#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Coordinate space of authoring geometry positions, selected by deformation context.
pub enum GeometrySpace {
    /// Root model coordinate space.
    Model,
    /// Local space of the specified Rotation transform.
    RotationLocal(
        /// Stable rotation-transform ID defining the local position space.
        String,
    ),
    /// Parameter/lattice space of the specified Warp transform.
    WarpParameter(
        /// Stable warp-transform ID defining the parameter/lattice position space.
        String,
    ),
}
