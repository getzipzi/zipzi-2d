//! Socket IO decodes off-thread; all Runtime admission/publication runs on App.
use super::*;
use std::time::{Duration, Instant};
use zipzi_runtime::{
    Executor, Work,
    local::{Endpoint, Inbound, Owner, read_frame, write_frame},
    protocol::CONTRACT,
};

pub(super) fn reopen(endpoint: &Endpoint) -> std::io::Result<u32> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match std::os::unix::net::UnixStream::connect(&endpoint.socket) {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                ))?;
                stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                write_frame(&mut stream, &serde_json::json!({"protocol": CONTRACT}))?;
                let hello = read_frame(&mut stream)?;
                let Some(ok) = hello.get("ok") else {
                    return Err(std::io::Error::other(hello.to_string()));
                };
                write_frame(
                    &mut stream,
                    &serde_json::json!({"protocol": CONTRACT, "runtime": ok["runtime"],
                    "client": ok["client"], "request_id": 1, "operation": {"op": "show_window", "session": null}}),
                )?;
                let response = read_frame(&mut stream)?;
                return if response.get("ok").is_some() {
                    ok["host_pid"]
                        .as_u64()
                        .and_then(|pid| u32::try_from(pid).ok())
                        .ok_or_else(|| std::io::Error::other("Missing host PID"))
                } else {
                    Err(std::io::Error::other(response.to_string()))
                };
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e),
        }
    }
}
pub(super) fn start(owner: Owner, cx: &mut App) -> std::io::Result<()> {
    let budget = cx
        .global::<host::DesktopHost>()
        .runtime
        .read()
        .transport_budget();
    let (server, incoming) = owner.listen_with_budget(budget)?;
    if let Some(ready_file) = std::env::var_os("ZIPZI_DEV_READY_FILE") {
        let endpoint = Endpoint::from_env()?;
        // The PID record establishes launch ownership early. Development
        // readiness is published only after this real strict socket handshake.
        cx.background_executor()
            .spawn(async move {
                let result = (|| -> std::io::Result<()> {
                    let mut stream = std::os::unix::net::UnixStream::connect(&endpoint.socket)?;
                    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                    write_frame(&mut stream, &serde_json::json!({"protocol": CONTRACT}))?;
                    let hello = read_frame(&mut stream)?;
                    if hello.get("ok").is_none() {
                        return Err(std::io::Error::other(hello.to_string()));
                    }
                    std::fs::write(ready_file, "handshake complete")
                })();
                if let Err(error) = result {
                    eprintln!("Development Runtime handshake failed: {error}");
                }
            })
            .detach();
    }
    let protocol = cx.global::<host::DesktopHost>().protocol.clone();
    let expiry = protocol.clone();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            cx.update(|cx| {
                let host = cx.global::<host::DesktopHost>().clone();
                expiry
                    .borrow_mut()
                    .expire(&mut host.runtime.write(), Instant::now());
            });
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        let _server = server; // Holds endpoint ownership throughout host residency.
        while let Ok(message) = incoming.recv().await {
            cx.update(|cx| {
                let host = cx.global::<host::DesktopHost>().clone();
                match message {
                    Inbound::Frame {
                        auth,
                        value,
                        reply,
                        _transfer,
                    } => {
                        let result = protocol.borrow_mut().receive_encoded(
                            &mut host.runtime.write(),
                            auth,
                            value,
                            Instant::now(),
                        );
                        if let Some(result) = result {
                            let _ = reply.send(result);
                        }
                        if let Some(session) = protocol.borrow_mut().take_window_request() {
                            show_session(session, cx);
                        }
                        dispatch(host, cx);
                    }
                    Inbound::Disconnected(auth) => protocol.borrow_mut().disconnect(
                        auth.client,
                        auth.connection,
                        Instant::now(),
                    ),
                    Inbound::Binary {
                        auth,
                        value,
                        bytes,
                        reply,
                        _transfer,
                    } => {
                        let mut result = protocol.borrow_mut().receive_binary(
                            &mut host.runtime.write(),
                            auth,
                            value,
                            bytes,
                            Instant::now(),
                        );
                        result.transfer = Some(_transfer);
                        let _ = reply.send(result);
                        dispatch(host, cx);
                    }
                }
            });
            // Yield between requests so a busy socket cannot monopolize UI input.
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
        }
    })
    .detach();
    Ok(())
}
fn show_session(session: Option<zipzi_runtime::SessionId>, cx: &mut App) {
    host::open_window(cx, true);
    for window in cx.windows() {
        let _ = window.update(cx, |root, window, cx| {
            let Ok(view) = root.downcast::<EditorView>() else {
                return;
            };
            view.update(cx, |view, cx| {
                if let Err(error) = view.editor.attach_resident_sessions() {
                    view.set_edit_error(error.to_string(), cx);
                    return;
                }
                if let Some(session) = session {
                    let id = view
                        .editor
                        .documents()
                        .iter()
                        .find(|d| view.editor.runtime_session_id(d.id) == Some(session))
                        .map(|d| d.id);
                    if let Some(id) = id {
                        view.activate_document(id, window, cx);
                    }
                }
                view.redraw(cx);
            });
        });
    }
}
#[derive(Default)]
struct Batch(Vec<Work>);
impl Executor for Batch {
    fn submit(&mut self, work: Work) -> Result<(), Box<Work>> {
        self.0.push(work);
        Ok(())
    }
}
fn dispatch(host: host::DesktopHost, cx: &mut App) {
    let mut batch = Batch::default();
    host.runtime.write().dispatch(&mut batch);
    for work in batch.0 {
        let host = host.clone();
        let task = cx
            .background_executor()
            .spawn(async move { work.run_on_thread("zipzi-ipc-job", 16 << 20) });
        cx.spawn(async move |cx| {
            let completion = task.await;
            cx.update(|cx| {
                let terminal = completion.is_terminal();
                host.runtime.write().complete(completion);
                if terminal {
                    host.completed();
                }
                dispatch(host, cx);
            });
        })
        .detach();
    }
}
