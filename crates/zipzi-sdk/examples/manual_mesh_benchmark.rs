//! Reproducible UV transfer latency/allocation probe; no renderer or licensed assets.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};
use zipzi_core::*;
use zipzi_sdk::*;
struct Allocator;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every allocation/deallocation is delegated unchanged to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            BYTES.fetch_add(n as u64, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn id(n: u32) -> String {
    format!("b8210000-0000-4000-8000-{n:012x}")
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = AuthoringSession::new(&id(1), ModelSetup::default())?;
    let grid = 32usize;
    let count = (grid + 1) * (grid + 1);
    let uvs: Vec<_> = (0..count)
        .map(|i| {
            Vec2::new(
                (i % (grid + 1)) as f32 / grid as f32,
                (i / (grid + 1)) as f32 / grid as f32,
            )
        })
        .collect();
    let positions: Vec<_> = uvs
        .iter()
        .map(|p| Vec2::new(p.x + 0.2 * p.x * p.y, p.y))
        .collect();
    let mut triangles = vec![];
    for y in 0..grid {
        for x in 0..grid {
            let a = (y * (grid + 1) + x) as u32;
            let b = a + 1;
            let c = a + (grid + 1) as u32;
            triangles.extend([[a, b, c + 1], [a, c + 1, c]]);
        }
    }
    s.edit("benchmark fixture", None, |e| {
        e.create_mesh(Mesh {
            id: id(2),
            vertex_ids: (0..count as u32).collect(),
            uvs: uvs.clone(),
            base_positions: positions.clone(),
            triangles,
            ..Default::default()
        })?;
        e.create_parameter(Parameter {
            id: id(3),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_binding(MeshBinding {
            id: id(4),
            mesh_id: id(2),
            axes: vec![BindingAxis {
                parameter_id: id(3),
                keys: (0..49).map(|i| i as f32 / 48.).collect(),
                ..Default::default()
            }],
            keyforms: (0..49)
                .map(|i| MeshKeyform {
                    keys: vec![i as f32 / 48.],
                    positions: positions
                        .iter()
                        .map(|p| Vec2::new(p.x + i as f32 * 0.01 * p.y, p.y))
                        .collect(),
                    ..Default::default()
                })
                .collect(),
        })?;
        for n in [10, 20, 30, 40] {
            e.create_parameter(Parameter {
                id: id(n),
                kind: ParameterKind::BlendShape,
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })?;
            e.create_blend_key_table(BlendShapeKeyTable {
                id: id(n + 1),
                parameter_id: id(n),
                samples: vec![],
                base_sample_id: String::new(),
                keys: vec![0., 1.],
                base_key_idx: 0,
            })?;
            e.create_blend_binding(BlendShapeBinding {
                id: id(n + 2),
                target_id: id(2),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(n + 1),
                constraint_ids: vec![],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); count],
                        ..Default::default()
                    },
                    DeltaMeshKeyform {
                        positions: vec![Vec2::new(0.01, 0.02); count],
                        ..Default::default()
                    },
                ]),
            })?;
        }
        Ok(())
    })?;
    let mut e = s.begin_owned_edit("drag benchmark", None)?;
    let mut times = vec![];
    let mut costs = None;
    for frame in 0..101 {
        let operation = ManualMeshOperation::MoveVertices {
            targets: vec![ManualVertexTarget {
                id: (16 * (grid + 1) + 16) as u32,
                uv: Vec2::new(0.5 + frame as f32 * 0.00005, 0.502),
            }],
            motion_from: None,
        };
        ACTIVE.store(frame > 0, Ordering::Relaxed);
        let started = Instant::now();
        let p = e.plan_manual_mesh(&id(2), operation)?;
        costs = Some(p.report.cost);
        e.apply_manual_mesh_plan(p)?;
        let us = started.elapsed().as_micros();
        ACTIVE.store(false, Ordering::Relaxed);
        if frame > 0 {
            times.push(us);
        }
    }
    times.sort_unstable();
    println!(
        "{}",
        serde_json::json!({"vertices":count,"triangles":grid*grid*2,"stored_shapes":58,"plans":times.len(),"median_us":times[times.len()/2],"p95_us":times[times.len()*95/100],"allocated_bytes_per_plan":BYTES.load(Ordering::Relaxed)/times.len()as u64,"allocations_per_plan":ALLOCS.load(Ordering::Relaxed)/times.len()as u64,"cost":costs})
    );
    drop(e);
    assert_eq!(s.history_lengths(), (1, 0));
    Ok(())
}
