use super::{
    archive,
    graph::{Graph, Node},
    invalid, Result,
};
use kasane_core::{
    document::{
        ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget, MotionClip,
        MotionEvent, MotionGroup, MotionKey, MotionPoint, MotionRegistration, MotionSegment,
        MotionTrack, MotionTrackTarget,
    },
    Document,
};
use std::collections::{BTreeMap, BTreeSet};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn xml(bytes: &[u8]) -> Result<String> {
    let files = archive::read(bytes)?;
    String::from_utf8(
        files
            .get("main.xml")
            .ok_or_else(|| invalid("CAN3 missing main.xml"))?
            .clone(),
    )
    .map_err(|e| invalid(e.to_string()))
}
pub fn paths(bytes: &[u8]) -> Result<Vec<String>> {
    let xml = xml(bytes)?;
    let parsed = roxmltree::Document::parse(&xml).map_err(|e| invalid(e.to_string()))?;
    let g = Graph::new(&parsed)?;
    let mut paths = BTreeSet::new();
    for n in parsed
        .descendants()
        .filter(|n| n.has_tag_name("CResource_Linked_Model") && n.attribute("xs.ref").is_none())
    {
        let file = g.req(n, "srcFile")?;
        let path = file
            .attribute("path")
            .or(file.text())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("CAN3 has no associated model path"))?;
        paths.insert(path.into());
    }
    if paths.is_empty() {
        return Err(invalid("CAN3 has no associated CMO3"));
    }
    Ok(paths.into_iter().collect())
}
pub fn read(bytes: &[u8], document: &mut Document) -> Result<()> {
    let xml = xml(bytes)?;
    let parsed = roxmltree::Document::parse(&xml).map_err(|e| invalid(e.to_string()))?;
    let g = Graph::new(&parsed)?;
    let root = parsed
        .root_element()
        .children()
        .find(|n| n.has_tag_name("main"))
        .and_then(|n| n.first_element_child())
        .ok_or_else(|| invalid("CAN3 missing animation root"))?;
    let root = g.resolve(root)?;
    if !root.has_tag_name("CAnimation") {
        return Err(invalid("Expected CAN3 animation"));
    }
    let groups = motion_groups(&g, root)?;
    let registered: BTreeSet<_> = groups
        .iter()
        .flat_map(|group| group.entries.iter().map(|entry| entry.clip_id.as_str()))
        .collect();
    let mut clips = Vec::new();
    for scene in g.list(root, "_scenes")? {
        let info = g.req(scene, "movieInfo")?;
        let fps: f32 = g.number(info, "fps")?;
        if !fps.is_finite() || fps <= 0. {
            return Err(invalid("Invalid CAN3 FPS"));
        }
        let duration_frames: f32 = g.number(info, "duration")?;
        let mut tracks = Vec::new();
        let mut events = Vec::new();
        let mut other_animation = false;
        let mut has_model = false;
        let mut expression_operations = BTreeMap::new();
        for source in g.list(g.req(scene, "trackSourceSet")?, "_sources")? {
            if source.has_tag_name("CMvTrack_Group_Source") {
                continue;
            }
            if !source.has_tag_name("CMvTrack_Live2DModel_Source") {
                return Err(invalid(format!(
                    "Unsupported CAN3 track {}",
                    source.tag_name().name()
                )));
            }
            if std::mem::replace(&mut has_model, true) {
                return Err(invalid(
                    "Multiple model instances in a CAN3 scene are not supported",
                ));
            }
            if g.optional::<i32>(source, "start", 0)? != 0
                || g.optional::<i32>(source, "internalOffset", 0)? != 0
                || g.optional(source, "isRepeat", false)?
            {
                return Err(invalid(
                    "CAN3 model track offsets or repetition are not supported",
                ));
            }
            if let Some(forms) = g.field(source, "formAnimationSet")? {
                if !g.list(forms, "formMapOnGlobal")?.is_empty()
                    || !g.list(forms, "formMapOnLocal")?.is_empty()
                {
                    return Err(invalid("CAN3 form animation is not supported"));
                }
            }
            // Inspect all channels, including preview transforms not mapped to
            // runtime motion tracks. An animated layout is not an expression.
            for effect in [
                "keyParamEffect",
                "partsVisibleEffect",
                "eyeBlinkEffect",
                "lipSyncEffect",
                "visualEffect",
            ] {
                if let Some(effect) = g.field(source, effect)? {
                    for attr in g.list(effect, "attrList")? {
                        if !g.optional(attr, "isActive", true)? {
                            continue;
                        }
                        for data in ["valueData", "valueDataXY"] {
                            if let Some(sequence) = g.field(attr, data)? {
                                let points = g.list(sequence, "points")?;
                                other_animation |= points.len() > 1;
                                if let Some(point) = points.first() {
                                    let anchor = g.field(*point, "anchor")?.unwrap_or(*point);
                                    other_animation |= g.optional::<f32>(anchor, "pos", 0.)? != 0.;
                                }
                            }
                        }
                    }
                }
            }
            for (effect, parameter) in [("keyParamEffect", true), ("partsVisibleEffect", false)] {
                let Some(effect) = g.field(source, effect)? else {
                    continue;
                };
                for attr in g.list(effect, "attrList")? {
                    if !g.optional(attr, "isActive", true)? {
                        continue;
                    }
                    let sequence = g.req(attr, "valueData")?;
                    let operation = g
                        .field(attr, "optionParam")?
                        .map(|options| g.field(options, crate::expression_semantics::KEY))
                        .transpose()?
                        .flatten()
                        .map(|value| {
                            serde_json::from_str::<crate::expression_semantics::Entry>(
                                value.text().unwrap_or(""),
                            )
                            .map_err(|error| {
                                invalid(format!("Invalid CAN3 expression operation: {error}"))
                            })
                        })
                        .transpose()?;
                    let unkeyed = g.list(sequence, "points")?.is_empty();
                    // An unkeyed Part still has a scene-local initial value.
                    // Only the untouched default (fully visible) can be omitted.
                    if unkeyed && !parameter && g.number::<f32>(sequence, "baseValue")? == 1. {
                        continue;
                    }
                    let guid = g.guid(attr, "guid")?;
                    let target = if parameter {
                        let parameter = document.get_parameter(&guid).ok_or_else(|| {
                            invalid(format!(
                                "CAN3 parameter identity does not match CMO3: {guid}"
                            ))
                        })?;
                        // Untouched attributes repeat the model default. An
                        // edited baseValue is a real initial override even
                        // when the source contains no explicit key at all.
                        if unkeyed
                            && operation.is_none()
                            && g.number::<f32>(sequence, "baseValue")? == parameter.default_value
                        {
                            continue;
                        }
                        MotionTrackTarget::Parameter { parameter_id: guid }
                    } else {
                        if document.get_part(&guid).is_none() {
                            return Err(invalid(format!(
                                "CAN3 Part identity does not match CMO3: {guid}"
                            )));
                        }
                        MotionTrackTarget::PartOpacity { part_id: guid }
                    };
                    if let Some(operation) = operation {
                        let MotionTrackTarget::Parameter { parameter_id } = &target else {
                            return Err(invalid(
                                "Expression operation is attached to a non-parameter channel",
                            ));
                        };
                        if expression_operations
                            .insert(parameter_id.clone(), operation)
                            .is_some()
                        {
                            return Err(invalid("Duplicate CAN3 expression operation"));
                        }
                    }
                    tracks.push(track(&g, attr, target, fps)?);
                }
            }
            // Model-level animation effects have explicit runtime targets.
            for (name, target) in [("eyeBlinkEffect", "EyeBlink"), ("lipSyncEffect", "LipSync")] {
                if let Some(effect) = g.field(source, name)? {
                    for attr in g.list(effect, "attrList")? {
                        if g.field(attr, "valueData")?.is_some()
                            && !g.list(g.req(attr, "valueData")?, "points")?.is_empty()
                        {
                            tracks.push(track(
                                &g,
                                attr,
                                MotionTrackTarget::Model {
                                    runtime_id: target.into(),
                                },
                                fps,
                            )?);
                        }
                    }
                }
            }
            if let Some(effect) = g.field(source, "visualEffect")? {
                for attr in g.list(effect, "attrList")? {
                    if g.runtime(attr)? == "opacity" {
                        let sequence = g.req(attr, "valueData")?;
                        if g.list(sequence, "points")?.is_empty()
                            && g.number::<f32>(sequence, "baseValue")? == 1.
                        {
                            continue;
                        }
                        let t = track(
                            &g,
                            attr,
                            MotionTrackTarget::Model {
                                runtime_id: "Opacity".into(),
                            },
                            fps,
                        )?;
                        tracks.push(t);
                    }
                }
            }
            if let Some(data) = g.field(source, "userData")? {
                for entry in data.children().filter(|n| n.is_element()) {
                    let frame: f32 = g.number(entry, "key")?;
                    events.push(MotionEvent {
                        id: id(),
                        time: frame / fps,
                        value: g.text(entry, "value")?,
                        extensions: Default::default(),
                    });
                }
            }
        }
        let fade = |name| -> Result<Option<f32>> {
            let value: f32 = g.optional(info, name, -1.)?;
            Ok((value >= 0.).then_some(value / 1000.))
        };
        clips.push((
            MotionClip {
                id: g.guid(scene, "guid")?,
                name: g.text(scene, "sceneName")?,
                duration: duration_frames / fps,
                fps,
                looping: g.optional(info, "isLoopMotion", false)?,
                restricted_beziers: g.optional(info, "isBezierRestricted", false)?,
                fade_in: fade("fadeInMSec")?,
                fade_out: fade("fadeOutMSec")?,
                tracks,
                events,
                extensions: Default::default(),
                meta_extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            },
            other_animation,
            expression_operations,
        ));
    }
    // Validate in a candidate: a late bad scene cannot partially modify a caller.
    let mut candidate = document.fork_candidate();
    for (mut clip, other_animation, expression_operations) in clips {
        clip.preserve_source_key_range();
        let status = candidate.validate_motion(&clip);
        if !status.is_ok() {
            return Err(invalid(format!("{}: {}", status.code, status.message)));
        }
        let edit = if !registered.contains(clip.id.as_str())
            && !other_animation
            && is_expression_scene(&clip)
        {
            let mut expression = ExpressionAsset {
                id: clip.id.clone(),
                name: clip.name.clone(),
                file_type: Some("Live2D Expression".into()),
                fade_in: clip.fade_in,
                fade_out: clip.fade_out,
                entries: clip
                    .tracks
                    .iter()
                    .filter(|track| matches!(track.target, MotionTrackTarget::Parameter { .. }))
                    .map(|track| {
                        let MotionTrackTarget::Parameter { parameter_id } = &track.target else {
                            unreachable!();
                        };
                        let default = candidate.get_parameter(parameter_id).unwrap().default_value;
                        if let Some(operation) = expression_operations.get(parameter_id) {
                            return operation.restore(parameter_id, default, track.initial.value);
                        }
                        Ok(ExpressionEntry {
                            target: ExpressionTarget::Resolved {
                                parameter_id: parameter_id.clone(),
                            },
                            value: track.initial.value - default,
                            blend: Some(ExpressionBlend::Add),
                            extensions: Default::default(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            };
            expression.preserve_source_scene(&clip);
            candidate.create_expression(expression)
        } else {
            candidate.create_motion(clip)
        };
        if !edit.status.is_ok() {
            return Err(invalid(format!(
                "{}: {}",
                edit.status.code, edit.status.message
            )));
        }
    }
    if !groups.is_empty() {
        let mut combined = candidate.motion_groups().to_vec();
        combined.extend(groups);
        let result = candidate.set_motion_groups(combined);
        if !result.status.is_ok() {
            return Err(invalid(format!(
                "{}: {}",
                result.status.code, result.status.message
            )));
        }
    }
    *document = candidate;
    Ok(())
}

fn motion_groups(g: &Graph<'_>, root: Node<'_>) -> Result<Vec<MotionGroup>> {
    let Some(settings) = g.field(root, "sceneBlendingSettings")? else {
        return Ok(Vec::new());
    };
    let mut groups = Vec::new();
    let mut seen = BTreeSet::new();
    for playlist in g.list(settings, "playlists")? {
        let guid = g.guid(playlist, "guid")?;
        if !guid.starts_with(crate::can3::MOTION_GROUP_PLAYLIST_PREFIX) {
            continue;
        }
        if uuid::Uuid::parse_str(&guid).is_err() || !seen.insert(guid) {
            return Err(invalid(
                "Invalid or duplicate motion group playlist identity",
            ));
        }
        let mut entries = Vec::new();
        for item in g.list(playlist, "list")? {
            if !item.has_tag_name("PlaylistItemData") {
                return Err(invalid("Unsupported motion group playlist item"));
            }
            entries.push(MotionRegistration {
                clip_id: g.guid(item, "guid")?,
                fade_in: None,
                fade_out: None,
                sound: None,
                extensions: Default::default(),
            });
        }
        groups.push(MotionGroup {
            name: g.text(playlist, "name")?,
            entries,
        });
    }
    Ok(groups)
}

// Duration and names do not classify scenes. One initial key is a static pose;
// even an identical second key is authored animation and remains a motion.
// Initial part visibility is scene setup, retained in source-scene metadata.
fn is_expression_scene(clip: &MotionClip) -> bool {
    clip.events.is_empty()
        && clip
            .tracks
            .iter()
            .any(|track| matches!(track.target, MotionTrackTarget::Parameter { .. }))
        && clip
            .tracks
            .iter()
            .all(|track| track.initial.time == 0. && track.segments.is_empty())
}
fn track(
    g: &Graph<'_>,
    attribute: Node<'_>,
    target: MotionTrackTarget,
    fps: f32,
) -> Result<MotionTrack> {
    let sequence = g.req(attribute, "valueData")?;
    let points = g.list(sequence, "points")?;
    let curves = g.list(sequence, "curveTypes")?;
    let key = |n| -> Result<MotionKey> {
        let n = g.req(n, "anchor")?;
        Ok(MotionKey {
            id: id(),
            time: g.number::<f32>(n, "pos")? / fps,
            value: g.number(n, "doubleValue")?,
        })
    };
    let control = |n, name| -> Result<MotionPoint> {
        let n = g.req(n, name)?;
        let frame = g.number::<f32>(n, "posF")?;
        Ok(MotionPoint {
            time: frame / fps,
            value: g.number(n, "doubleValue")?,
        })
    };
    let initial = if let Some(first) = points.first() {
        key(*first)?
    } else {
        MotionKey {
            id: id(),
            time: 0.,
            value: g.number(sequence, "baseValue")?,
        }
    };
    let mut segments = Vec::new();
    for (index, pair) in points.windows(2).enumerate() {
        let end = key(pair[1])?;
        let mode = curves
            .get(index)
            .and_then(|n| n.attribute("v"))
            .unwrap_or("SMOOTH");
        segments.push(match mode {
            "LINEAR" => MotionSegment::Linear { end },
            "STEP" => MotionSegment::Stepped { end },
            "INVERSE_STEP" => MotionSegment::InverseStepped { end },
            "SMOOTH" | "BEZIER" => MotionSegment::Bezier {
                control1: control(pair[0], "next")?,
                control2: control(pair[1], "prev")?,
                end,
            },
            _ => return Err(invalid(format!("Unsupported CAN3 curve type {mode}"))),
        });
    }
    let fade = |name| -> Result<Option<f32>> {
        if let Some(options) = g.field(attribute, "optionParam")? {
            if let Some(n) = g.field(options, name)? {
                let value: f32 = super::graph::parse(n.text().unwrap_or(""))?;
                return Ok((value >= 0.).then_some(value / 1000.));
            }
        }
        Ok(None)
    };
    Ok(MotionTrack {
        id: id(),
        target,
        initial,
        segments: segments.into(),
        fade_in: fade("KEY_ATTR_FADE_IN")?,
        fade_out: fade("KEY_ATTR_FADE_OUT")?,
        extensions: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_expression_scene_restores_operations_with_a_new_guid() {
        let parameter = id();
        let original_scene = id();
        let copied_scene = id();
        let mut base = Document::new();
        assert!(base
            .initialize(&id(), kasane_core::ModelSetup::default())
            .is_ok());
        assert!(base
            .create_parameter(kasane_core::Parameter {
                id: parameter.clone(),
                runtime_id: "P".into(),
                name: "P".into(),
                default_value: 0.5,
                ..Default::default()
            })
            .status
            .is_ok());
        let entries = vec![ExpressionEntry {
            target: ExpressionTarget::Resolved {
                parameter_id: parameter,
            },
            blend: Some(ExpressionBlend::Multiply),
            value: -5.,
            extensions: Default::default(),
        }];
        let mut source = base.clone();
        assert!(source
            .create_expression(ExpressionAsset {
                id: original_scene.clone(),
                name: "Expression".into(),
                file_type: None,
                fade_in: None,
                fade_out: None,
                entries: entries.clone(),
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            })
            .status
            .is_ok());
        let artifact = crate::encode_can3(&source, &Default::default()).unwrap();
        let archive = super::super::archive::read(&artifact.bytes).unwrap();
        let xml = std::str::from_utf8(&archive["main.xml"]).unwrap();
        // A native copy changes the scene identity but retains attribute options.
        let xml = xml.replace(
            &format!("uuid=\"{original_scene}\""),
            &format!("uuid=\"{copied_scene}\""),
        );
        let bytes = crate::caff::encode(Vec::new(), xml.as_bytes(), 0).unwrap();
        let mut reopened = base.clone();
        read(&bytes, &mut base).unwrap();
        assert_eq!(base.expression_order(), [copied_scene.clone()]);
        assert_eq!(base.get_expression(&copied_scene).unwrap().entries, entries);
        // Re-export must attach the operations to the new identity as well.
        let bytes = crate::encode_can3(&base, &Default::default())
            .unwrap()
            .bytes;
        read(&bytes, &mut reopened).unwrap();
        assert_eq!(
            reopened.get_expression(&copied_scene).unwrap().entries,
            entries
        );
    }

    #[test]
    fn unkeyed_parameter_base_value_imports_as_an_additive_expression() {
        let parameter = "ca030001-0000-4000-8000-000000000001";
        let scene = "ca030001-0000-4000-8000-000000000002";
        let xml = format!(
            r#"<root><main><CAnimation>
          <carray_list xs.n="_scenes"><CSceneSource>
            <CSceneGuid xs.n="guid" uuid="{scene}"/>
            <s xs.n="sceneName">Unkeyed pose</s>
            <CMvMovieInfo xs.n="movieInfo"><d xs.n="fps">30</d><i xs.n="duration">8</i></CMvMovieInfo>
            <CTrackSourceSet xs.n="trackSourceSet"><carray_list xs.n="_sources">
              <CMvTrack_Live2DModel_Source><CMvEffect_Live2DParameter xs.n="keyParamEffect">
                <carray_list xs.n="attrList"><CMvAttrF>
                  <CParameterGuid xs.n="guid" uuid="{parameter}"/>
                  <CMutableSequence xs.n="valueData"><d xs.n="baseValue">0.7</d><array xs.n="points"/></CMutableSequence>
                </CMvAttrF></carray_list>
              </CMvEffect_Live2DParameter></CMvTrack_Live2DModel_Source>
            </carray_list></CTrackSourceSet>
          </CSceneSource></carray_list>
        </CAnimation></main></root>"#
        );
        let bytes = crate::caff::encode(Vec::new(), xml.as_bytes(), 0).unwrap();
        let mut document = Document::new();
        assert!(document
            .initialize(
                &id(),
                kasane_core::Canvas::new(100., 100., kasane_core::Vec2::new(0., 0.), 1.)
            )
            .is_ok());
        assert!(document
            .create_parameter(kasane_core::Parameter {
                id: parameter.into(),
                runtime_id: "ParamX".into(),
                name: "X".into(),
                default_value: 0.5,
                ..Default::default()
            })
            .status
            .is_ok());
        read(&bytes, &mut document).unwrap();
        assert!(document.motion_order().is_empty());
        let expression = document.get_expression(scene).unwrap().clone();
        assert_eq!(expression.entries[0].value, 0.7 - 0.5);
        assert_eq!(expression.entries[0].blend, Some(ExpressionBlend::Add));
        let mut invalid = expression.clone();
        invalid
            .extensions
            .insert("KasaneSourceScene".into(), serde_json::Value::Null);
        assert_eq!(
            document.replace_expression(invalid).status.code,
            "INVALID_EXPRESSION_SOURCE_SCENE"
        );
        assert_eq!(document.get_expression(scene), Some(&expression));
    }

    fn animation_with_sources(sources: &str, scene: &str) -> Vec<u8> {
        let xml = format!(
            r#"<root><main><CAnimation>
          <CResource_Linked_Model><File xs.n="srcFile" path="model.cmo3"/></CResource_Linked_Model>
          <carray_list xs.n="_scenes"><CSceneSource>
            <CSceneGuid xs.n="guid" uuid="{scene}"/><s xs.n="sceneName">Pose</s>
            <CMvMovieInfo xs.n="movieInfo"><d xs.n="fps">30</d><i xs.n="duration">8</i></CMvMovieInfo>
            <CTrackSourceSet xs.n="trackSourceSet"><carray_list xs.n="_sources">{sources}</carray_list></CTrackSourceSet>
          </CSceneSource></carray_list>
        </CAnimation></main></root>"#
        );
        crate::caff::encode(Vec::new(), xml.as_bytes(), 0).unwrap()
    }

    fn unkeyed_effect(effect: &str, guid: &str, value: f32) -> String {
        format!(
            r#"<effect xs.n="{effect}"><carray_list xs.n="attrList"><CMvAttrF>
          <guid xs.n="guid" uuid="{guid}"/>
          <CMutableSequence xs.n="valueData"><d xs.n="baseValue">{value}</d><array xs.n="points"/></CMutableSequence>
        </CMvAttrF></carray_list></effect>"#
        )
    }

    #[test]
    fn unkeyed_model_opacity_preserves_its_initial_override() {
        let scene = id();
        let source = r#"<CMvTrack_Live2DModel_Source>
          <effect xs.n="visualEffect"><carray_list xs.n="attrList"><CMvAttrF>
            <id xs.n="id" idstr="opacity"/>
            <CMutableSequence xs.n="valueData"><d xs.n="baseValue">0.25</d><array xs.n="points"/></CMutableSequence>
          </CMvAttrF></carray_list></effect>
        </CMvTrack_Live2DModel_Source>"#;
        let mut bytes = animation_with_sources(source, &scene);
        for _ in 0..2 {
            let mut doc = Document::new();
            assert!(doc
                .initialize(&id(), kasane_core::ModelSetup::default())
                .is_ok());
            read(&bytes, &mut doc).unwrap();
            let tracks = &doc.get_motion(&scene).unwrap().tracks;
            assert_eq!(tracks.len(), 1);
            assert_eq!(
                tracks[0].target,
                MotionTrackTarget::Model {
                    runtime_id: "Opacity".into()
                }
            );
            assert_eq!(tracks[0].initial.value, 0.25);
            bytes = crate::encode_can3(&doc, &Default::default()).unwrap().bytes;
        }
    }

    #[test]
    fn unkeyed_part_visibility_survives_expression_engineering_roundtrip() {
        let parameter = id();
        let part = id();
        let scene = id();
        let mut base = Document::new();
        assert!(base
            .initialize(
                &id(),
                kasane_core::Canvas::new(100., 100., Default::default(), 1.)
            )
            .is_ok());
        assert!(base
            .create_parameter(kasane_core::Parameter {
                id: parameter.clone(),
                runtime_id: "P".into(),
                name: "P".into(),
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(base
            .create_part(kasane_core::Part {
                id: part.clone(),
                runtime_id: "Part".into(),
                name: "Part".into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let sources = format!(
            "<CMvTrack_Live2DModel_Source>{}{}</CMvTrack_Live2DModel_Source>",
            unkeyed_effect("keyParamEffect", &parameter, 0.5),
            unkeyed_effect("partsVisibleEffect", &part, 0.)
        );
        let mut bytes = animation_with_sources(&sources, &scene);
        for _ in 0..2 {
            let mut doc = base.clone();
            read(&bytes, &mut doc).unwrap();
            let source = doc.get_expression(&scene).unwrap().source_scene().unwrap();
            let setup = source
                .tracks
                .iter()
                .find(|t| {
                    t.target
                        == MotionTrackTarget::PartOpacity {
                            part_id: part.clone(),
                        }
                })
                .unwrap();
            assert_eq!(setup.initial.time, 0.);
            assert_eq!(setup.initial.value, 0.);
            bytes = crate::encode_can3(&doc, &Default::default()).unwrap().bytes;
        }
    }

    #[test]
    fn repeated_instances_of_one_model_are_rejected_without_changing_document() {
        let parameter = id();
        let source = format!(
            "<CMvTrack_Live2DModel_Source>{}</CMvTrack_Live2DModel_Source>",
            unkeyed_effect("keyParamEffect", &parameter, 0.5)
        );
        let bytes = animation_with_sources(&source.repeat(2), &id());
        assert_eq!(paths(&bytes).unwrap(), ["model.cmo3"]);
        let mut doc = Document::new();
        assert!(doc
            .initialize(&id(), kasane_core::ModelSetup::default())
            .is_ok());
        assert!(doc
            .create_parameter(kasane_core::Parameter {
                id: parameter,
                runtime_id: "P".into(),
                name: "P".into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let revision = doc.revision();
        let error = read(&bytes, &mut doc).unwrap_err();
        assert!(error.to_string().contains("Multiple model"), "{error}");
        assert_eq!(doc.revision(), revision);
        assert!(doc.expression_order().is_empty());
        assert!(doc.motion_order().is_empty());
    }
}
