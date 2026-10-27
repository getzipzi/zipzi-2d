//! GPU context and autorelease handling at the native rendering boundary.
#[cfg(target_os = "macos")]
pub type GpuContext = gpui_kit::SharedMetalContext;
#[cfg(not(target_os = "macos"))]
pub type GpuContext = ();

pub fn gpu_context(window: &gpui_kit::Window) -> Option<GpuContext> {
    #[cfg(target_os = "macos")]
    {
        window.metal_context()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        None
    }
}

pub fn autorelease<R>(work: impl FnOnce() -> R) -> R {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_| work())
    }
    #[cfg(not(target_os = "macos"))]
    {
        work()
    }
}
