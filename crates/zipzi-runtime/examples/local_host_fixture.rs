//! CPU-only subprocess acceptance fixture; not a production Editor substitute.
use std::time::{Duration, Instant};
use zipzi_runtime::{local::*, protocol::*, *};
fn milliseconds(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
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
    let mut limits = RuntimeLimits::default();
    limits.jobs = milliseconds("ZIPZI_TEST_JOB_COUNT", 32) as usize;
    limits.events_per_session = milliseconds("ZIPZI_TEST_EVENT_COUNT", 512) as usize;
    let mut runtime = Runtime::new(limits);
    let (_server, messages) = owner.listen_with_budget(runtime.transfer_budget())?;
    let mut protocol = Protocol::new(ProtocolLimits {
        reconnect_grace: Duration::from_millis(milliseconds("ZIPZI_TEST_GRACE_MS", 30000)),
        result_ttl: Duration::from_millis(milliseconds("ZIPZI_TEST_RESULT_TTL_MS", 300000)),
        results_per_client: milliseconds("ZIPZI_TEST_RESULT_COUNT", 1024) as usize,
        ..Default::default()
    });
    let mut drop_response = std::env::var("ZIPZI_TEST_DROP_RESPONSE").unwrap_or_default();
    let mut drop_after = milliseconds("ZIPZI_TEST_DROP_RESPONSE_AFTER", 0);
    let (completed, completions) = std::sync::mpsc::channel();
    let mut workers =
        WorkerPool::new(runtime.limits().running_jobs, 16 << 20, move |completion| {
            let _ = completed.send(completion);
        })?;
    loop {
        match messages.try_recv() {
            Ok(Inbound::Frame {
                auth, value, reply, ..
            }) => {
                let op = value["operation"]["op"].as_str().unwrap_or("").to_owned();
                let response = protocol.receive_encoded(&mut runtime, auth, value, Instant::now());
                if !drop_response.is_empty() && drop_response == op && drop_after == 0 {
                    drop_response.clear();
                    drop(reply);
                } else {
                    if !drop_response.is_empty() && drop_response == op {
                        drop_after -= 1;
                    }
                    if let Some(response) = response {
                        let _ = reply.send(response);
                    }
                }
            }
            Ok(Inbound::Disconnected(auth)) => {
                protocol.disconnect(auth.client, auth.connection, Instant::now())
            }
            Ok(Inbound::Binary {
                auth,
                value,
                bytes,
                reply,
                _transfer,
            }) => {
                let op = value["operation"]["op"].as_str().unwrap_or("").to_owned();
                let mut response =
                    protocol.receive_binary(&mut runtime, auth, value, bytes, Instant::now());
                response.transfer = Some(_transfer);
                if !drop_response.is_empty() && drop_response == op && drop_after == 0 {
                    drop_response.clear();
                    drop(reply);
                } else {
                    if drop_response == op {
                        drop_after -= 1;
                    }
                    let _ = reply.send(response);
                }
            }
            Err(async_channel::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(1)),
            Err(async_channel::TryRecvError::Closed) => break,
        }
        while let Ok(completion) = completions.try_recv() {
            runtime
                .complete(completion)
                .expect("completion belongs to this fixture");
        }
        runtime.dispatch(&mut workers);
        protocol.expire(&mut runtime, Instant::now());
    }
    runtime.cancel_host_jobs();
    while runtime.outstanding_jobs() != 0 {
        runtime.dispatch(&mut workers);
        runtime
            .complete(completions.recv()?)
            .expect("shutdown completion");
    }
    Ok(())
}
