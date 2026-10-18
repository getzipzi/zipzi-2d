use super::{
    attributes::{fades, Attribute, Context},
    *,
};
const PARAM: &str = "live2dParam_";
const PART: &str = "live2DPartsOpacity_";
const UNBOUNDED: (f64, f64) = (-f64::MAX, f64::MAX);
impl Animation<'_> {
    pub(super) fn scene(&mut self, scene: &Scene) -> (N, N) {
        let width = self.canvas.width.max(1.).round();
        let height = self.canvas.height.max(1.).round();
        let source = self.graph.reserve("CSceneSource");
        let guid = self.graph.add(
            N::new("CSceneGuid")
                .attr("uuid", &scene.guid)
                .attr("note", &scene.name),
        );
        let root = self.graph.reserve("CMvTrack_Group_Source");
        let model = self.graph.reserve("CMvTrack_Live2DModel_Source");
        let root_guid = self.guid("CTrackGuid", "Root");
        let model_guid = self.guid("CTrackGuid", &self.options.animation_name.clone());
        let visual = self.graph.reserve("CMvEffect_VisualDefault");
        let parameters = self.graph.reserve("CMvEffect_Live2DParameter");
        let parts = self.graph.reserve("CMvEffect_Live2DPartsVisible");
        let lip = self.graph.reserve("CMvEffect_LipSync");
        let blink = self.graph.reserve("CMvEffect_EyeBlink");
        let adapt = self
            .graph
            .add(N::new("AdaptType").field("name", s("DEFAULT")));
        let ctx = Context {
            track: model.clone(),
            adapt,
        };
        let effects = vec![
            blink.clone(),
            lip.clone(),
            parameters.clone(),
            parts.clone(),
            visual.clone(),
        ];
        let root_base = track_base(
            &root,
            &source,
            &root_guid,
            "Root",
            scene,
            N::null(),
            Vec::new(),
            N::null(),
            BTreeMap::new(),
        );
        self.graph.set(
            &root,
            N::new("CMvTrack_Group_Source")
                .field("super", root_base)
                .field("_childTrackGuids", list([model_guid.clone()]))
                .field("bounds", rect(width, height)),
        );
        let movie = N::new("CMvMovieInfo")
            .attr("durationLock", false)
            .field("width", i(width as i32))
            .field("height", i(height as i32))
            .field("duration", i(scene.frames))
            .field("fps", d(scene.fps))
            .field("workspaceStart", i(0))
            .field("workspaceEnd", i(scene.frames))
            .field("background", N::new("CColor"))
            .field("fadeInMSec", i(scene.fade_in))
            .field("fadeOutMSec", i(scene.fade_out))
            .field("isBezierRestricted", b(scene.restricted))
            .field("isLoopMotion", b(scene.looping))
            .field("startFrame", i(0))
            .field(
                "frameIndexType",
                N::enumeration("CFrameIndexType", "ZERO_INDEX"),
            );
        let value = N::new("CSceneSource")
            .attr("exportMotionFile", true)
            .attr("loopMotionEditAssist", scene.looping)
            .field("sceneName", s(&scene.name))
            .field(
                "canvas",
                N::new("CImageCanvas")
                    .field("pixelWidth", i(width as i32))
                    .field("pixelHeight", i(height as i32))
                    .field("background", N::new("CColor")),
            )
            .field("guid", guid.clone())
            .field("tag", s(&scene.name))
            .field(
                "trackSourceSet",
                N::new("CTrackSourceSet").field("_sources", list([root.clone(), model.clone()])),
            )
            .field("rootTrack", root)
            .field("movieInfo", movie)
            .field("_animation", self.root.clone())
            .field("marker", N::empty_map())
            .field(
                "defaultParameterCurveType",
                N::enumeration("CCurveType", "SMOOTH"),
            )
            .field("defaultPartCurveType", N::enumeration("CCurveType", "STEP"))
            .field("fixAspect", b(false))
            .field(
                "targetVersion",
                N::enumeration("Animation", "FOR_UNITY_SDK"),
            )
            .field("lockMarker", b(false))
            .field("onionSkinMarker", N::list("array_list", []))
            .field("lockOnionSkinMarker", b(false))
            .field(
                "parameterBookmarkCarrierTrackSet",
                N::new("ParameterBookmarkLabelCarrierTrackSet")
                    .field("_owner", source.clone())
                    .field("_trackSortInfo", N::empty_list()),
            );
        self.graph.set(&source, value);
        let base = track_base(
            &model,
            &source,
            &model_guid,
            &self.options.animation_name,
            scene,
            visual.clone(),
            effects,
            root_guid,
            scene.events.clone(),
        );
        self.graph.set(
            &model,
            N::new("CMvTrack_Live2DModel_Source")
                .field(
                    "super",
                    N::new("ICMvTrack_Linked")
                        .field("super", base)
                        .field("_resourceGuid", self.resource.clone()),
                )
                .field("keyParamEffect", parameters.clone())
                .field("partsVisibleEffect", parts.clone())
                .field("lipSyncEffect", lip.clone())
                .field("eyeBlinkEffect", blink.clone())
                .field("formEditEffect", N::null())
                .field(
                    "formAnimationSet",
                    N::new("FormAnimationSet")
                        .field("formMapOnGlobal", N::empty_map())
                        .field("formMapOnLocal", N::empty_map())
                        .field("trackSource", model.clone()),
                )
                .field("bounds", rect(width, height))
                .field(
                    "parameterBookmarkLabelSet",
                    N::new("ParameterBookmarkLabelSet").field("labels", N::empty_list()),
                ),
        );
        self.visual(&ctx, &visual, scene);
        let mut param_attrs = Vec::new();
        let mut part_attrs = Vec::new();
        for track in &scene.tracks {
            match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => {
                    let parameter = self.doc.get_parameter(parameter_id).unwrap();
                    let id = format!("{PARAM}{}", parameter.runtime_id);
                    let mut attr = Attribute::new(&id, &parameter.name);
                    attr.guid = N::new("CParameterGuid")
                        .attr("uuid", parameter_id)
                        .attr("note", &parameter.name);
                    attr.options = fades(track);
                    attr.options.push(
                        s(format!("live2dParam:{}", parameter.runtime_id)).named("KEY_PARAM_ID"),
                    );
                    let r = self.float(
                        &ctx,
                        attr,
                        &track.keys,
                        (f64::from(parameter.minimum), f64::from(parameter.maximum)),
                        f64::from(parameter.default_value),
                    );
                    param_attrs.push((id, r));
                }
                MotionTrackTarget::PartOpacity { part_id } => {
                    let part = self.doc.get_part(part_id).unwrap();
                    let mut attr = Attribute::new(format!("{PART}{}", part.runtime_id), &part.name);
                    attr.guid = N::new("CPartGuid")
                        .attr("uuid", part_id)
                        .attr("note", &part.name);
                    attr.options = fades(track);
                    attr.options
                        .push(s(&part.runtime_id).named("KEY_PARTS_VISIBLE_ID"));
                    let map_key = format!("{PART}{}", part_key(&part.runtime_id));
                    let r = self.float(&ctx, attr, &track.keys, (0., 1.), 1.);
                    part_attrs.push((map_key, r));
                }
                _ => {}
            }
        }
        self.graph.set(
            &parameters,
            effect(
                "CMvEffect_Live2DParameter",
                "Effects:Live2DParam",
                false,
                &model,
                &param_attrs,
            ),
        );
        self.graph.set(
            &parts,
            effect(
                "CMvEffect_Live2DPartsVisible",
                PART,
                false,
                &model,
                &part_attrs,
            ),
        );
        // These rest effects keep Animator's standard editable model-track shape.
        // Runtime aggregate EyeBlink/LipSync curves are reported as unsupported.
        let mut lip_attrs = Vec::new();
        for (id, name, value) in [
            ("soundLevel", "Sound Level", 0.),
            ("lipSyncScale", "Lip Sync Scale", 1.),
            ("lipSyncBase", "Lip Sync Base", 0.),
            ("lipSyncLevel", "Lip Sync Level", 0.),
        ] {
            lip_attrs.push((
                id.into(),
                self.static_float(&ctx, id, name, value, UNBOUNDED),
            ));
        }
        self.graph.set(
            &lip,
            effect(
                "CMvEffect_LipSync",
                "Effects:LipSync",
                true,
                &model,
                &lip_attrs,
            )
            .field("effectParameterAttrIds", N::empty_list())
            .field("syncTrackGuid", N::null())
            .field("isInvert", b(false))
            .field("isRelative", b(true)),
        );
        let eye_open = self.static_float(&ctx, "eyeOpen", "Eye Open", 1., UNBOUNDED);
        let level = self.static_float(&ctx, "effectLevel", "Effect Level", 1., UNBOUNDED);
        self.graph.set(
            &blink,
            effect(
                "CMvEffect_EyeBlink",
                "Effects:EyeBlink",
                true,
                &model,
                &[("eyeOpen".into(), eye_open), ("effectLevel".into(), level)],
            )
            .field("effectParameterAttrIds", N::empty_list())
            .field("invert", b(false))
            .field("relative", b(true)),
        );
        (source, guid)
    }
    fn visual(&mut self, ctx: &Context, visual: &N, scene: &Scene) {
        let mut attrs = Vec::new();
        attrs.push(("xy".into(), self.point(ctx, "xy", "XY")));
        attrs.push(("anchor".into(), self.point(ctx, "anchor", "Anchor")));
        for (id, name, value) in [
            ("scalex", "Scale X", 100.),
            ("scaley", "Scale Y", 100.),
            ("rotate", "Rotate", 0.),
            ("shear", "Shear", 0.),
        ] {
            attrs.push((
                id.into(),
                self.static_float(ctx, id, name, value, UNBOUNDED),
            ));
        }
        let opacity = if let Some(track) = scene.tracks.iter().find(
            |t| matches!(&t.target,MotionTrackTarget::Model{runtime_id} if runtime_id=="Opacity"),
        ) {
            let mut attr = Attribute::new("opacity", "Opacity");
            attr.options = fades(track);
            self.float(ctx, attr, &track.keys, (0., 1.), 1.)
        } else {
            self.static_float(ctx, "opacity", "Opacity", 1., (0., 1.))
        };
        attrs.push(("opacity".into(), opacity));
        attrs.push((
            "artPathWidth".into(),
            self.static_float(ctx, "artPathWidth", "Art Path Width", 100., (0., f64::MAX)),
        ));
        attrs.push(("frameStep".into(), self.frame_step(ctx)));
        let mut value = effect(
            "CMvEffect_VisualDefault",
            "VisualDefault",
            false,
            &ctx.track,
            &attrs,
        );
        for (id, field) in [
            ("xy", "attrXY"),
            ("anchor", "attrAnchorXY"),
            ("scalex", "attrScaleX"),
            ("scaley", "attrScaleY"),
            ("rotate", "attrRotate"),
            ("shear", "attrShear"),
            ("opacity", "attrOpacity"),
            ("artPathWidth", "attrArtPathWidth"),
            ("frameStep", "attrFrameStep"),
        ] {
            value = value.field(
                field,
                attrs.iter().find(|(key, _)| key == id).unwrap().1.clone(),
            );
        }
        self.graph.set(visual, value);
    }
}
#[allow(clippy::too_many_arguments)]
fn track_base(
    track: &N,
    source: &N,
    guid: &N,
    name: &str,
    scene: &Scene,
    visual: N,
    effects: Vec<N>,
    parent: N,
    events: BTreeMap<i32, String>,
) -> N {
    N::new("ICMvTrack_Source")
        .field("name", s(name))
        .field("isUserRenamed", b(false))
        .field("guid", guid.clone())
        .field("start", i(0))
        .field("internalOffset", i(0))
        .field("duration", i(scene.frames))
        .field("editable", b(true))
        .field("visible", b(true))
        .field("mute", b(false))
        .field("isGuide", b(false))
        .field("isRepeat", b(false))
        .field("soloSwitch", b(false))
        .field(
            "visualHandler",
            N::new("CVisualHandler").field("track", track.clone()),
        )
        .field(
            "soundHandler",
            N::new("CSoundHandler").field("track", track.clone()),
        )
        .field("soundEffect", N::null())
        .field("visualEffect", visual)
        .field(
            "effectManager",
            N::new("CMvEffectManager").field("effectList", array("ICMvEffect", effects)),
        )
        .field("parentGuid", parent)
        .field("_sceneSource", source.clone())
        .field(
            "userData",
            map(events.into_iter().map(|(frame, value)| {
                N::new("entry")
                    .field("key", i(frame))
                    .field("value", s(value))
            })),
        )
        .field("keys", N::null())
}
fn effect(tag: &str, id: &str, can_delete: bool, track: &N, attrs: &[(String, N)]) -> N {
    N::new(tag).field(
        "super",
        N::new("ICMvEffect")
            .field("id", N::new("CEffectId").attr("idstr", id))
            .field("isActive", b(true))
            .field("canDelete", b(can_delete))
            .field(
                "attrList",
                array("ICMvAttr", attrs.iter().map(|(_, r)| r.clone())),
            )
            .field(
                "attrMap",
                map(attrs.iter().map(|(id, r)| {
                    let key = N::new("CAttrId").attr("idstr", id).named("key");
                    N::new("entry").child(key).field("value", r.clone())
                })),
            )
            .field("track", track.clone()),
    )
}
/// Animator's map normalizes part runtime IDs (PartArm -> PARTS_ARM).
pub(super) fn part_key(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if !c.is_ascii_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        if !word.is_empty() && index > 0 {
            let prev = chars[index - 1];
            let next = chars.get(index + 1);
            if prev.is_ascii_digit() != c.is_ascii_digit()
                || (prev.is_ascii_lowercase() && c.is_ascii_uppercase())
                || (prev.is_ascii_uppercase()
                    && c.is_ascii_uppercase()
                    && next.is_some_and(char::is_ascii_lowercase))
            {
                words.push(std::mem::take(&mut word));
            }
        }
        word.push(c.to_ascii_uppercase());
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.first().is_some_and(|s| s == "PART") {
        words[0] = "PARTS".into();
    }
    words.join("_")
}
