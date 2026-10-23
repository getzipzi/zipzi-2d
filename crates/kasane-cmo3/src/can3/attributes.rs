use super::*;

pub(super) struct Context {
    pub track: N,
    pub adapt: N,
}
pub(super) struct Attribute {
    pub id: String,
    pub name: String,
    pub guid: N,
    pub options: Vec<N>,
}
impl Attribute {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            guid: N::null(),
            options: Vec::new(),
        }
    }
}
impl Animation<'_> {
    fn attr_base(&self, context: &Context, attr: Attribute) -> N {
        N::new("ICMvAttr")
            .field("isShyMode", b(false))
            .field("isFreezeMode", b(false))
            .field("id", N::new("CAttrId").attr("idstr", attr.id))
            .field("name", s(attr.name))
            .field("guid", attr.guid)
            .field("isActive", b(true))
            .field("adaptType", context.adapt.clone())
            .field(
                "optionParam",
                N::list("hash_map", attr.options).attr("keyType", "string"),
            )
            .field("track", context.track.clone())
    }
    pub(super) fn point(&mut self, ctx: &Context, id: &str, name: &str) -> N {
        let r = self.graph.reserve("CMvAttrPt");
        let value = N::new("CMvAttrPt")
            .field("super", self.attr_base(ctx, Attribute::new(id, name)))
            .field(
                "valueDataXY",
                N::new("CXY_TNSSequence")
                    .field("points", array("CPtTNS", []))
                    .field(
                        "basePt",
                        N::new("GVector2").field("x", f(0)).field("y", f(0)),
                    )
                    .field("attr", r.clone()),
            );
        self.graph.set(&r, value);
        r
    }
    pub(super) fn frame_step(&mut self, ctx: &Context) -> N {
        let r = self.graph.reserve("CMvAttrI");
        let base = N::new("ACValueSequence")
            .field("curMin", d(0))
            .field("curMax", d(0))
            .field("posStart", i(0))
            .field("keyPts2", N::array("int-array", [] as [i32; 0]))
            .field("keyMin", i(i32::MAX))
            .field("keyMax", i(i32::MIN))
            .field("lastValue", d("NaN"))
            .field("lastPos", i(-1))
            .field("attr", r.clone())
            .field("baseValue", d(1));
        let value = N::new("CMvAttrI")
            .field(
                "super",
                self.attr_base(ctx, Attribute::new("frameStep", "Frame Step")),
            )
            .field(
                "valueData",
                N::new("CIntSequence")
                    .field("super", base)
                    .field("points", array("CSeqPt", [])),
            )
            .field("rangeMin", i(0))
            .field("rangeMax", i(100));
        self.graph.set(&r, value);
        r
    }
    pub(super) fn static_float(
        &mut self,
        ctx: &Context,
        id: &str,
        name: &str,
        value: f64,
        range: (f64, f64),
    ) -> N {
        self.float(ctx, Attribute::new(id, name), &[], range, value)
    }
    pub(super) fn float(
        &mut self,
        ctx: &Context,
        attr: Attribute,
        keys: &[Key],
        range: (f64, f64),
        default: f64,
    ) -> N {
        let r = self.graph.reserve("CMvAttrF");
        let low = keys
            .iter()
            .flat_map(|key| [key.prev.1, key.value, key.next.1])
            .reduce(f64::min)
            .unwrap_or(0.);
        let high = keys
            .iter()
            .flat_map(|key| [key.prev.1, key.value, key.next.1])
            .reduce(f64::max)
            .unwrap_or(0.);
        let first = keys.first();
        let last = keys.last();
        // Anchors and handles use absolute scene frames. A nonzero posStart
        // would subtract the first key time a second time during cubic evaluation.
        let base = N::new("ACValueSequence")
            .field("curMin", d(low))
            .field("curMax", d(high))
            .field("posStart", i(0))
            .field(
                "keyPts2",
                N::array("int-array", keys.iter().map(|k| k.frame)),
            )
            .field("keyMin", i(first.map_or(0, |k| k.frame)))
            .field("keyMax", i(last.map_or(0, |k| k.frame)))
            .field("lastValue", d(last.map_or(default, |k| k.value)))
            .field("lastPos", i(last.map_or(0, |k| k.frame)))
            .field("attr", r.clone())
            .field("baseValue", d(first.map_or(default, |k| k.value)));
        let sequence = N::new("CMutableSequence")
            .field("super", base)
            .field(
                "points",
                array(
                    "CBezierPt",
                    keys.iter().map(|key| {
                        N::new("CBezierPt")
                            .field(
                                "anchor",
                                N::new("CSeqPt")
                                    .field("isCorner", b(false))
                                    .field("pos", i(key.frame))
                                    .field("doubleValue", d(key.value)),
                            )
                            .field("prev", control(key.prev))
                            .field("next", control(key.next))
                    }),
                ),
            )
            .field(
                "curveTypes",
                list(keys.iter().map(|k| N::enumeration("CCurveType", k.curve))),
            );
        let value = N::new("CMvAttrF")
            .field("super", self.attr_base(ctx, attr))
            .field("valueData", sequence)
            .field("rangeMin", d(range.0.min(low)))
            .field("rangeMax", d(range.1.max(high)))
            .field("isRepeat", b(false))
            .field("repeatMin", d(-f64::MAX))
            .field("repeatMax", d(f64::MAX))
            .field("linked_keyFormsForObject", N::null());
        self.graph.set(&r, value);
        r
    }
}
fn control((frame, value): (f64, f64)) -> N {
    N::new("CBezierCtrlPt")
        .field("posF", f(frame))
        .field("pos", i(0))
        .field("doubleValue", d(value))
        .field("isPosOptimized", b(false))
}
pub(super) fn fades(track: &Track) -> Vec<N> {
    let mut result = Vec::new();
    if let Some(value) = track.fade_in {
        result.push(i(value).named("KEY_ATTR_FADE_IN"));
    }
    if let Some(value) = track.fade_out {
        result.push(i(value).named("KEY_ATTR_FADE_OUT"));
    }
    result
}
