//! No viewport worker or GPU resources on platforms without Metal.
use crate::worker_queue::MailboxReceiver;
use gpui_kit::{IntoElement, Styled, Window, div};
use std::sync::Arc;
use zipzi_editor_core::{SceneSnapshot, ViewportBinding, ViewportId};

pub type Device = ();
pub fn available() -> bool {
    false
}
pub type Image = Arc<gpui_kit::RenderImage>;

#[derive(Clone)]
pub struct Presented {
    pub image: Image,
    pub canvas: zipzi_editor_core::CanvasFrame,
}

pub struct Request {
    pub binding: ViewportBinding,
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<zipzi_editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
    pub transform: zipzi_editor_core::ViewportTransform,
    pub scene: SceneSnapshot,
}

pub struct Frame {
    pub binding: Option<ViewportBinding>,
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<zipzi_editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
    pub stats: (u64, u64, u64),
    pub image: Result<Image, String>,
}

pub struct RequestSender;
impl RequestSender {
    pub fn cancel(&self, _: ViewportId) {}
    pub fn send(&self, _: Request) -> Result<(), ()> {
        Err(())
    }
}

type Connection = (
    RequestSender,
    MailboxReceiver<Option<ViewportId>, Frame>,
    Option<Device>,
);
pub fn start(_: &Window) -> anyhow::Result<Connection> {
    anyhow::bail!("Viewport rendering disabled: Metal requires macOS")
}
pub fn device_changed(_: &Window, _: Option<&Device>) -> bool {
    false
}
pub fn element(_: Image) -> impl IntoElement {
    div().size_full()
}
