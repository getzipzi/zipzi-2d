//! Preserve editable curves while adapting Animator's inverse-step boundaries.
use super::*;

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}
fn prefix(start: &Key, end: &Key, frame: f64) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let a = (f64::from(start.frame), start.value);
    let d = (f64::from(end.frame), end.value);
    if start.curve != "BEZIER" {
        return (a, d, lerp(a, d, (frame - a.0) / (d.0 - a.0)));
    }
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..48 {
        let t = (low + high) * 0.5;
        let ab = lerp(a, start.next, t);
        let bc = lerp(start.next, end.prev, t);
        let cd = lerp(end.prev, d, t);
        let point = lerp(lerp(ab, bc, t), lerp(bc, cd, t), t);
        if point.0 < frame {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    let ab = lerp(a, start.next, t);
    let bc = lerp(start.next, end.prev, t);
    let cd = lerp(end.prev, d, t);
    let abc = lerp(ab, bc, t);
    (ab, abc, lerp(abc, lerp(bc, cd, t), t))
}

pub(super) fn repair(keys: Vec<Key>, id: &str, report: &mut Cmo3Report) -> Vec<Key> {
    let desired = |index: usize| {
        let mut key = keys[index].clone();
        if key.curve == "INVERSE_STEP" && index + 1 < keys.len() {
            key.value = keys[index + 1].value;
        }
        key
    };
    let mut result = vec![desired(0)];
    let mut repairs = 0;
    for index in 0..keys.len() - 1 {
        let start = &keys[index];
        let end = &keys[index + 1];
        let next = desired(index + 1);
        if start.curve == "INVERSE_STEP" && (start.value != end.value || next.value != end.value) {
            result.last_mut().unwrap().curve = "STEP";
            repairs += 1;
        } else if next.value != end.value && matches!(start.curve, "LINEAR" | "BEZIER") {
            let guard = end.frame - 1;
            if guard > start.frame {
                let (first, second, point) = prefix(start, end, f64::from(guard));
                result.last_mut().unwrap().next = first;
                result.push(Key {
                    frame: guard,
                    value: point.1,
                    curve: "STEP",
                    prev: second,
                    next: (f64::from(guard), point.1),
                });
            } else {
                result.last_mut().unwrap().curve = "STEP";
            }
        }
        result.push(next);
    }
    if repairs > 0 {
        bake::info(report, id, "CAN3_INVERSE_STEP_REPAIRED", &format!(
            "Adapted {repairs} inverse-step boundaries with local step keys; preceding Bezier prefixes retain their exact shape. Boundary anchor values and interpolation within the final frame before a jump can differ; other authored keys and curves are retained"
        ));
    }
    result
}
