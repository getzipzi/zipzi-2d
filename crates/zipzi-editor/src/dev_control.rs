//! Checkout-local development restart handshake. Never discard unsaved work.
use crate::*;
use std::{path::PathBuf, time::Duration};

pub(super) fn start(cx: &mut App) {
    let Some(directory) = std::env::var_os("ZIPZI_DEV_CONTROL_DIR").map(PathBuf::from) else {
        return;
    };
    let request = directory.join(format!("quit-{}.json", std::process::id()));
    let reply = directory.join(format!("quit-{}.reply.json", std::process::id()));
    cx.spawn(async move |cx| {
        let mut last = String::new();
        loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let Ok(bytes) = std::fs::read(&request) else { continue; };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue; };
            let Some(nonce) = value["nonce"].as_str() else { continue; };
            if nonce.len() != 32 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) || nonce == last { continue; }
            last = nonce.to_owned();
            cx.update(|cx| {
                let result = prepare_exit(cx);
                let response = serde_json::json!({"nonce": nonce, "error": result.err().map(|e| e.to_string())});
                if std::fs::write(&reply, response.to_string()).is_err() { return; }
                if response["error"].is_null() { cx.quit(); }
            });
        }
    }).detach();
}

fn prepare_exit(cx: &mut App) -> anyhow::Result<()> {
    for handle in cx.windows() {
        handle.update(cx, |root, window, cx| -> anyhow::Result<()> {
            let root = root
                .downcast::<gpui_kit::base::Root>()
                .map_err(|_| anyhow::anyhow!("Unknown development window"))?;
            let view = root
                .read(cx)
                .view()
                .clone()
                .downcast::<EditorView>()
                .map_err(|_| anyhow::anyhow!("Unknown Editor view"))?;
            view.update(cx, |view, cx| {
                anyhow::ensure!(
                    !view.modal && !view.editor.gesture_active() && !view.editor.is_busy(),
                    "Finish the active dialog, gesture or operation before restarting Editor"
                );
                if let Some(id) = view.editor.active_id() {
                    anyhow::ensure!(
                        view.prepare_property_inputs(id, window, cx),
                        "Finish editing the active field before restarting Editor"
                    );
                }
                if let Some(dev) = &mut view.dev_session {
                    dev.sync(&view.editor)?;
                }
                Ok(())
            })
        })??;
    }
    let host = cx.global::<host::DesktopHost>();
    anyhow::ensure!(
        !host.protocol.borrow().has_connected_clients(),
        "Disconnect Python clients before restarting Editor"
    );
    let runtime = host.runtime.read();
    anyhow::ensure!(
        runtime.outstanding_jobs() == 0,
        "Wait for Runtime jobs before restarting Editor"
    );
    anyhow::ensure!(
        runtime
            .session_catalog(host.runtime.client_id())?
            .iter()
            .all(|s| !s.dirty && !s.saving),
        "Save or close unsaved documents before restarting Editor"
    );
    Ok(())
}
