//! Normalize missing suffixes before AppKit checks replacement of the final URL.
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSOpenSavePanelDelegate;
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use std::path::Path;

define_class!(
    // SAFETY: NSObject has no additional subclassing requirements.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = String]
    pub(crate) struct ExtensionSavePanelDelegate;

    unsafe impl NSObjectProtocol for ExtensionSavePanelDelegate {}

    unsafe impl NSOpenSavePanelDelegate for ExtensionSavePanelDelegate {
        #[unsafe(method_id(panel:userEnteredFilename:confirmed:))]
        unsafe fn panel_userEnteredFilename_confirmed(
            &self,
            _sender: &AnyObject,
            filename: &NSString,
            _confirmed: bool,
        ) -> Option<Retained<NSString>> {
            let name = filename.to_string();
            let name = if Path::new(&name).extension().is_none() {
                format!("{name}.{}", self.ivars())
            } else {
                name
            };
            Some(NSString::from_str(&name))
        }
    }
);

impl ExtensionSavePanelDelegate {
    pub(crate) fn new(marker: MainThreadMarker, extension: String) -> Retained<Self> {
        let this = Self::alloc(marker).set_ivars(extension);
        // SAFETY: NSObject's designated initializer takes no arguments.
        unsafe { msg_send![super(this), init] }
    }
}
