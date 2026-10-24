//! CPU-only subprocess acceptance fixture; not a production Editor substitute.
#[cfg(unix)]
use std::time::{Duration, Instant};
#[cfg(unix)]
use zipzi_runtime::{local::*, protocol::*, *};
#[cfg(unix)]
fn milliseconds(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
#[cfg(unix)]
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint::from_env()?;
    let Some(owner) = endpoint.inherit_or_claim()? else {
        return Err("Host already owned".into());
    };
    if let Some(path) = std::env::var_os("ZIPZI_TEST_PID_LOG") {
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
            "{}",
            std::process::id()
        )?;
    }
    std::thread::sleep(Duration::from_millis(milliseconds(
        "ZIPZI_TEST_START_DELAY_MS",
        0,
    )));
    if std::env::var_os("ZIPZI_TEST_FAIL_START").is_some() {
        return Err("Injected startup failure".into());
    }
    let (_server, messages) = owner.listen()?;
    let mut limits = RuntimeLimits::default();
    limits.jobs_total = milliseconds("ZIPZI_TEST_JOB_COUNT", 32) as usize;
    limits.session_events = milliseconds("ZIPZI_TEST_EVENT_COUNT", 512) as usize;
    let mut runtime = Runtime::new(limits);
    let ui = runtime.register_client()?;
    let mut protocol = Protocol::new(ProtocolLimits {
        reconnect_grace: Duration::from_millis(milliseconds("ZIPZI_TEST_GRACE_MS", 30000)),
        results_per_client: milliseconds("ZIPZI_TEST_RESULT_COUNT", 1024) as usize,
        ..Default::default()
    });
    let mut drop_response = std::env::var("ZIPZI_TEST_DROP_RESPONSE").unwrap_or_default();
    let mut drop_after = milliseconds("ZIPZI_TEST_DROP_RESPONSE_AFTER", 0);
    let mut busy_once = std::env::var_os("ZIPZI_TEST_BUSY_COMMIT_ONCE").is_some();
    loop {
        match messages.try_recv() {
            Ok(Inbound::Frame { auth, value, reply }) => {
                let op = value["operation"]["op"].as_str().unwrap_or("").to_owned();
                let interactive = if op == "commit" && busy_once {
                    busy_once = false;
                    let session = runtime.list_sessions()[0].id;
                    runtime.begin_interaction(ui, session)?;
                    Some(session)
                } else {
                    None
                };
                if op == "request_status" {
                    std::thread::sleep(Duration::from_millis(milliseconds(
                        "ZIPZI_TEST_STATUS_DELAY_MS",
                        0,
                    )));
                }
                let response = protocol.receive(&mut runtime, auth, value, Instant::now());
                if let Some(session) = interactive {
                    runtime.end_interaction(ui, session)?;
                }
                let mut executor = Threaded::default();
                runtime.dispatch(&mut executor);
                for task in executor.0 {
                    runtime.complete(task.run());
                }
                if !drop_response.is_empty() && drop_response == op && drop_after == 0 {
                    drop_response.clear();
                    drop(reply);
                } else {
                    if !drop_response.is_empty() && drop_response == op {
                        drop_after -= 1;
                    }
                    let _ = reply.send(response);
                }
            }
            Ok(Inbound::Disconnected(auth)) => {
                protocol.disconnect(auth.client, auth.connection, Instant::now())
            }
            Err(async_channel::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(1)),
            Err(async_channel::TryRecvError::Closed) => break,
        }
        protocol.expire(&mut runtime, Instant::now());
    }
    Ok(())
}
#[cfg(unix)]
#[derive(Default)]
struct Threaded(Vec<Work>);
#[cfg(unix)]
impl Executor for Threaded {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push(work);
        Ok(())
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("The local host fixture requires Unix sockets");
}
