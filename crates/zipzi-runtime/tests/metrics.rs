#![cfg(feature = "bench-instrumentation")]

#[test]
fn probes_observe_rust_codec_allocations_and_real_rgba_storage_copy() {
    use zipzi_runtime::metrics;
    metrics::reset();
    let bytes = metrics::encode(&vec![1u32, 2, 3]).unwrap();
    let decoded: Vec<u32> = metrics::decode(&bytes).unwrap();
    assert_eq!(decoded, vec![1, 2, 3]);
    let storage = std::hint::black_box(vec![13u8; 65536]);
    let budget = zipzi_project::ImageMemoryBudget::new(1024);
    let image =
        zipzi_project::PreparedImageContent::from_rgba_bytes(1, 1, vec![12, 34, 56, 255], &budget)
            .unwrap();
    assert_eq!(image.texture().rgba.as_ref(), &[12, 34, 56, 255]);
    let probe = metrics::snapshot();
    assert!(probe.enabled);
    assert_eq!(probe.json_encodes, 1);
    assert_eq!(probe.json_decodes, 1);
    assert_eq!(probe.copied_payload_bytes, 4);
    assert!(probe.rust_allocations > 0);
    assert!(probe.rust_allocated_bytes >= storage.len());
    assert!(probe.rust_peak_bytes >= probe.rust_live_bytes);
    let live = probe.rust_live_bytes;
    drop(storage);
    assert!(metrics::snapshot().rust_live_bytes < live);
}
