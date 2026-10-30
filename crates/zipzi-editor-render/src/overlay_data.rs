//! Immutable mesh instances shared by the native Metal and D3D11 passes.
use crate::{OVERLAY_MARKER_RADIUS, OverlayPass, OverlayStyle, OverlayViewport};
use anyhow::{Result, ensure};
use zipzi_editor_core::{OverlaySnapshot, OverlayVertexKind};
#[repr(C)]
pub(super) struct Uniforms {
    pub viewport: [f32; 2],
    pub scale: f32,
    pub _padding: f32,
    pub origin: [f32; 2],
    pub texture_origin: [f32; 2],
    pub edge: [f32; 4],
    pub vertex: [f32; 4],
    pub selected: [f32; 4],
    pub restricted: [f32; 4],
}
#[derive(PartialEq)]
pub(super) struct Key {
    pub viewport: OverlayViewport,
    pub style: OverlayStyle,
    pub edges: Vec<[f32; 4]>,
    pub markers: Vec<[f32; 4]>,
}

impl Key {
    pub fn new(
        viewport: OverlayViewport,
        overlay: &OverlaySnapshot,
        style: OverlayStyle,
        pass: OverlayPass,
    ) -> Result<Self> {
        ensure!(
            viewport.extent.iter().all(|v| *v > 0 && *v <= 16384)
                && viewport.scale.is_finite()
                && viewport.scale > 0.
                && viewport.origin.iter().all(|v| v.is_finite()),
            "Invalid overlay viewport"
        );
        let lines = if pass == OverlayPass::Vertices {
            &[][..]
        } else {
            &overlay.lines
        };
        let vertices = if pass == OverlayPass::Edges {
            &[][..]
        } else {
            &overlay.vertices
        };
        Ok(Self {
            viewport,
            style,
            edges: lines
                .iter()
                .filter(|(a, b)| [a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()))
                .map(|(a, b)| [a.x, a.y, b.x, b.y])
                .collect(),
            markers: [false, true]
                .into_iter()
                .flat_map(|emphasized| {
                    vertices
                        .iter()
                        .filter(move |v| (v.selected || v.hovered) == emphasized)
                })
                .filter(|v| v.kind == OverlayVertexKind::Mesh)
                .filter(|v| v.position.x.is_finite() && v.position.y.is_finite())
                .map(|v| {
                    [
                        v.position.x,
                        v.position.y,
                        OVERLAY_MARKER_RADIUS,
                        (u8::from(v.selected) + 2 * u8::from(v.hovered) + 4 * u8::from(!v.editable))
                            as f32,
                    ]
                })
                .collect(),
        })
    }
    pub fn uniforms(&self) -> Uniforms {
        let OverlayViewport {
            extent,
            scale,
            origin,
        } = self.viewport;
        Uniforms {
            viewport: [extent[0] as f32, extent[1] as f32],
            scale,
            _padding: 0.,
            origin,
            texture_origin: origin.map(|v| (v * scale).floor()),
            edge: self.style.edge,
            vertex: self.style.vertex,
            selected: self.style.selected,
            restricted: self.style.restricted,
        }
    }
}
