use crate::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ObjectFit, Pixels, Style, StyleRefinement, Styled, Window,
};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_core_foundation::CFRetained;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_core_video::{CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use refineable::Refineable;
#[cfg(target_os = "windows")]
mod directx;
#[cfg(target_os = "windows")]
pub use directx::DirectXSurface;

/// A source of a surface's content.
#[derive(Clone, Debug)]
pub enum SurfaceSource {
    /// Immutable premultiplied D3D11 texture with explicit producer/consumer fences.
    #[cfg(target_os = "windows")]
    DirectX(std::sync::Arc<DirectXSurface>),
    /// A CoreVideo image buffer
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Surface(CFRetained<CVPixelBuffer>),
    /// Premultiplied texture submitted on the window shared Metal queue.
    #[cfg(target_os = "macos")]
    MetalTexture(metal::Texture),
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
impl From<CFRetained<CVPixelBuffer>> for SurfaceSource {
    fn from(value: CFRetained<CVPixelBuffer>) -> Self {
        SurfaceSource::Surface(value)
    }
}

impl PartialEq for SurfaceSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            #[cfg(target_os = "windows")]
            (Self::DirectX(a), Self::DirectX(b)) => std::sync::Arc::ptr_eq(a, b),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            (Self::Surface(a), Self::Surface(b)) => a == b,
            #[cfg(target_os = "macos")]
            (Self::MetalTexture(a), Self::MetalTexture(b)) => std::ptr::eq(a.as_ref(), b.as_ref()),
            #[allow(unreachable_patterns)]
            _ => false,
        }
    }
}
impl Eq for SurfaceSource {}

/// A surface element.
pub struct Surface {
    source: SurfaceSource,
    object_fit: ObjectFit,
    style: StyleRefinement,
}

/// Create a new surface element.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
pub fn surface(source: impl Into<SurfaceSource>) -> Surface {
    Surface {
        source: source.into(),
        object_fit: ObjectFit::Contain,
        style: Default::default(),
    }
}

impl Surface {
    /// Set the object fit for the image.
    pub fn object_fit(mut self, object_fit: ObjectFit) -> Self {
        self.object_fit = object_fit;
        self
    }
}

impl Element for Surface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        #[cfg_attr(
            not(any(target_os = "macos", target_os = "ios")),
            allow(unused_variables)
        )]
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        #[cfg_attr(
            not(any(target_os = "macos", target_os = "ios")),
            allow(unused_variables)
        )]
        window: &mut Window,
        _: &mut App,
    ) {
        match &self.source {
            #[cfg(target_os = "windows")]
            SurfaceSource::DirectX(texture) => {
                let size = crate::size(
                    crate::DevicePixels(texture.extent[0] as i32),
                    crate::DevicePixels(texture.extent[1] as i32),
                );
                window.paint_surface(
                    self.object_fit.get_bounds(bounds, size),
                    self.source.clone(),
                );
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            SurfaceSource::Surface(surface) => {
                let size = crate::size(
                    CVPixelBufferGetWidth(surface).into(),
                    CVPixelBufferGetHeight(surface).into(),
                );
                let new_bounds = self.object_fit.get_bounds(bounds, size);
                // TODO: Add support for corner_radii
                window.paint_surface(new_bounds, surface.clone());
            }
            #[cfg(target_os = "macos")]
            SurfaceSource::MetalTexture(texture) => {
                let size = crate::size(
                    crate::DevicePixels(texture.width() as i32),
                    crate::DevicePixels(texture.height() as i32),
                );
                window.paint_surface(
                    self.object_fit.get_bounds(bounds, size),
                    self.source.clone(),
                );
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
}

impl IntoElement for Surface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Styled for Surface {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
