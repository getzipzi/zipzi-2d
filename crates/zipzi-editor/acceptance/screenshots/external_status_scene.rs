//! Actual socket admission and Metal pixels for the quiet external activity indicator.
use super::{
    super::{fixture, runtime_ipc},
    common::save,
};
use crate::host::external_status::ExternalStatus;
use gpui_kit::test::TestWindowExt;
use std::time::{Duration, Instant};

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let (document, assets) = fixture::sample_document()?;
    let (mut h, host, endpoint) = runtime_ipc::setup(document)?;
    h._assets = assets;
    let (session, mesh) = h.view.read_with(&h.cx, |view, _| {
        (
            view.editor.document().unwrap().runtime_session_id(),
            view.editor.layers()[0].id.clone(),
        )
    });
    // This scene checks native chrome pixels, with no model preview dependency.
    h.view.update(&mut h.cx, |view, cx| {
        view.replace_editor(
            zipzi_editor_core::Editor::with_runtime(host.runtime.clone()),
            cx,
        );
        view.redraw(cx);
    });
    // Paint the initially empty entity before the first connection. This checks
    // that socket admission alone stays quiet in an idle window.
    h.capture()?;
    let address = zipzi_runtime::local::Endpoint::new(endpoint.path(), "test")?;
    let (auth_tx, auth_rx) = std::sync::mpsc::channel();
    let (close_tx, close_rx) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || -> anyhow::Result<()> {
        let mut socket = address.connect(Duration::from_secs(5), Duration::from_secs(5))?;
        zipzi_runtime::local::write_frame(
            &mut socket,
            &serde_json::json!({
                "protocol": zipzi_runtime::protocol::CONTRACT,
            }),
        )?;
        let hello = zipzi_runtime::local::read_frame(&mut socket)?;
        auth_tx.send(hello)?;
        let _ = close_rx.recv();
        Ok(())
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let hello = loop {
        h.cx.advance_clock(Duration::from_millis(2));
        h.cx.run_until_parked();
        if let Ok(hello) = auth_rx.try_recv() {
            break hello;
        }
        anyhow::ensure!(Instant::now() < deadline, "socket handshake timed out");
        std::thread::sleep(Duration::from_millis(2));
    };
    let client = serde_json::from_value(hello["ok"]["client"].clone())?;
    let frame = h.capture()?;
    save(&frame, directory, "external-connection-quiet.png")?;
    h.cx.update_window(h.window, |_, window, _| {
        anyhow::ensure!(
            window.try_find("app-external-status").is_none(),
            "connection must not show a status"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    let job = host.runtime.write().request_save(
        client,
        session,
        &endpoint.path().join("Background-rig"),
    )?;
    let work = host.runtime.write().take_work(job)?;
    anyhow::ensure!(
        host.runtime.write().complete(work.run()),
        "save completion rejected"
    );
    host.runtime.write().release_job(client, job)?;
    let edit = host
        .runtime
        .write()
        .begin_edit(client, session, "Native external update", None)?;
    host.runtime
        .write()
        .apply_edit(client, edit, |e| e.rename_mesh(&mesh, "External update"))?;
    host.runtime.write().commit_edit(client, edit)?;
    let frame = h.capture()?;
    save(&frame, directory, "external-other-project-applied.png")?;
    anyhow::ensure!(
        host.external_status.read_with(&h.cx, |s, _| s.label())
            == "Background-rig · External changes applied"
    );
    h.cx.update_window(h.window, |_, window, _| {
        anyhow::ensure!(
            window.try_find("app-external-connected-dot").is_none(),
            "connection dot must not be rendered"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    let crop = h.element_image(&frame, "app-status-bar")?;
    save(&crop, directory, "external-statusbar-detail.png")?;
    close_tx.send(())?;
    peer.join().unwrap()?;
    h.cx.run_until_parked();
    anyhow::ensure!(
        !host
            .external_status
            .read_with(&h.cx, |s, _| s.label())
            .is_empty(),
        "disconnect must not erase a commit notice"
    );
    h.cx.advance_clock(ExternalStatus::NOTICE_DURATION);
    h.cx.run_until_parked();
    h.capture()?;
    h.cx.update_window(h.window, |_, window, _| {
        anyhow::ensure!(
            window.try_find("app-external-status").is_none(),
            "commit notice must disappear without a disconnect message"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}
