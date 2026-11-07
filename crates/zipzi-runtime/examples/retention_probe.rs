//! Allocator-attributed fixed-state probe. Build with bench-instrumentation.
use std::{path::PathBuf, time::Instant};
use zipzi_contract::kernel_wire::ImportFormat;
use zipzi_runtime::{metrics, Runtime};
use zipzi_sdk::NameTarget;
fn main() {
    assert!(metrics::snapshot().enabled, "enable bench-instrumentation");
    let source = PathBuf::from(std::env::args().nth(1).expect("model3 path"));
    let steps = std::env::args()
        .nth(2)
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(1200);
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            "00000000-0000-4000-8000-000000000001",
            Default::default(),
        )
        .unwrap();
    let mut version = rt.session_read(client, session).unwrap().version();
    let job = rt
        .request_import(
            client,
            version,
            &source,
            ImportFormat::Model3,
            Default::default(),
        )
        .unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.complete(completion).unwrap();
    rt.job(client, job)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    rt.release_job(client, job).unwrap();
    version = rt.session_read(client, session).unwrap().version();
    let mesh = rt
        .session_read(client, session)
        .unwrap()
        .snapshot()
        .document()
        .mesh_order()[0]
        .clone();
    let start = Instant::now();
    metrics::reset();
    println!(
        "{}",
        serde_json::json!({"step":0,"metrics":metrics::snapshot()})
    );
    for i in 0..steps {
        let draft = rt.begin_draft(client, version).unwrap();
        let now = Instant::now();
        let (draft, ()) = rt
            .batch(client, draft, |c| {
                c.rename(
                    NameTarget::Mesh(&mesh),
                    if i % 2 == 0 { "Fixed A" } else { "Fixed B" }.into(),
                )
            })
            .unwrap();
        let batch = now.elapsed().as_secs_f64() * 1000.;
        let receipt = rt.publish(client, draft, "Fixed target").unwrap();
        version = receipt.version;
        if (i + 1) % 100 == 0 {
            println!(
                "{}",
                serde_json::json!({"step":i+1,"elapsed_s":start.elapsed().as_secs_f64(),"batch_ms":batch,"metrics":metrics::snapshot()})
            );
        }
    }
    rt.close_session(client, session, true).unwrap();
    println!(
        "{}",
        serde_json::json!({"closed":true,"metrics":metrics::snapshot()})
    );
    drop(rt);
    println!(
        "{}",
        serde_json::json!({"dropped":true,"metrics":metrics::snapshot()})
    );
}
