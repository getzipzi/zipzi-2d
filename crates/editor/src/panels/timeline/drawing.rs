use super::*;
use kasane_sdk::{CompiledSegment, MotionBezierHandle, sample_motion_curve};

#[derive(Clone)]
pub(super) enum Hit {
    Key(TimelineKeyRef),
    Summary(Vec<TimelineKeyRef>),
    Event(String),
    Handle {
        end: TimelineKeyRef,
        handle: MotionBezierHandle,
    },
}
#[derive(Clone)]
pub(super) struct Marker {
    pub position: Point<Pixels>,
    pub hit: Hit,
}
struct Stroke {
    points: Vec<Point<Pixels>>,
    color: usize,
}
#[derive(Clone, PartialEq)]
struct SceneKey {
    projection: usize,
    rows: usize,
    size: Size<Pixels>,
    time: (f64, f64),
    scroll: f64,
    mode: TimelineDisplayMode,
    channels: Vec<TimelineRowId>,
    keys: Vec<TimelineKeyRef>,
    events: Vec<String>,
    range: (f64, f64),
    normalized: bool,
}
pub(super) struct Scene {
    key: SceneKey,
    _projection: Arc<TimelineProjection>,
    _rows: Arc<[TimelineRow]>,
    strokes: Vec<Stroke>,
    pub markers: Vec<Marker>,
}
#[derive(Default)]
pub(super) struct SceneCache {
    pub scene: Option<Scene>,
    pub builds: u64,
    pub paints: u64,
}
impl Scene {
    pub fn hit(&self, position: Point<Pixels>) -> Result<Option<Hit>, ()> {
        hit_markers(&self.markers, position)
    }
    pub fn marquee(
        &self,
        a: Point<Pixels>,
        b: Point<Pixels>,
    ) -> (Vec<TimelineKeyRef>, Vec<String>) {
        let bounds = Bounds::from_corners(
            point(a.x.min(b.x), a.y.min(b.y)),
            point(a.x.max(b.x), a.y.max(b.y)),
        );
        let mut keys = vec![];
        let mut events = vec![];
        for marker in &self.markers {
            if bounds.contains(&marker.position) {
                match &marker.hit {
                    Hit::Key(key) => keys.push(key.clone()),
                    Hit::Event(event) => events.push(event.clone()),
                    _ => {}
                }
            }
        }
        (keys, events)
    }
}
fn hit_markers(markers: &[Marker], position: Point<Pixels>) -> Result<Option<Hit>, ()> {
    let mut closest: Option<(&Marker, f32)> = None;
    let mut ambiguous = false;
    for marker in markers {
        let dx = (marker.position.x - position.x).abs().as_f32();
        let dy = (marker.position.y - position.y).abs().as_f32();
        if dx > 6. || dy > 6. {
            continue;
        }
        let distance = dx * dx + dy * dy;
        let handle = matches!(marker.hit, Hit::Handle { .. });
        match closest {
            Some((previous, best)) => {
                let previous_handle = matches!(previous.hit, Hit::Handle { .. });
                if (handle && !previous_handle) || (handle == previous_handle && distance < best) {
                    closest = Some((marker, distance));
                    ambiguous = false;
                } else if handle == previous_handle && marker.position == previous.position {
                    ambiguous = true;
                }
            }
            None => closest = Some((marker, distance)),
        }
    }
    if ambiguous {
        Err(())
    } else {
        Ok(closest.map(|(marker, _)| marker.hit.clone()))
    }
}

#[cfg(test)]
mod hit_tests {
    use super::{Hit, Marker, hit_markers, tick_label, ticks};
    use gpui_kit::{point, px};
    fn marker(x: f32, id: &str) -> Marker {
        Marker {
            position: point(px(x), px(10.)),
            hit: Hit::Event(id.into()),
        }
    }
    #[test]
    fn dense_points_choose_the_nearest_in_either_draw_order() {
        for markers in [
            vec![marker(10., "nearest"), marker(14., "far")],
            vec![marker(14., "far"), marker(10., "nearest")],
        ] {
            assert!(
                matches!(hit_markers(&markers, point(px(10.5), px(10.))), Ok(Some(Hit::Event(id))) if id == "nearest")
            );
        }
    }
    #[test]
    fn coincident_points_require_disambiguation_only_when_nearest() {
        let markers = vec![marker(10., "a"), marker(10., "b"), marker(14., "c")];
        assert!(hit_markers(&markers, point(px(10.), px(10.))).is_err());
        assert!(
            matches!(hit_markers(&markers, point(px(14.), px(10.))), Ok(Some(Hit::Event(id))) if id == "c")
        );
    }
    #[test]
    fn zoomed_ruler_preserves_subframe_and_subsecond_labels() {
        for seconds in [false, true] {
            let marks = ticks((0., 0.001), 600., 30., seconds);
            let step = marks[1] - marks[0];
            let labels: std::collections::HashSet<_> = marks
                .iter()
                .map(|t| tick_label(*t, step, 30., seconds))
                .collect();
            assert_eq!(labels.len(), marks.len());
        }
    }
}
pub(super) fn time_x(time: f64, range: (f64, f64), width: f32) -> f32 {
    ((time - range.0) / (range.1 - range.0) * width as f64) as f32
}
pub(super) fn x_time(x: f32, range: (f64, f64), width: f32) -> f64 {
    range.0 + x as f64 / width.max(1.) as f64 * (range.1 - range.0)
}
pub(super) fn value_y(value: f64, range: (f64, f64), height: f32) -> f32 {
    8. + ((range.1 - value) / (range.1 - range.0) * curve_height(height) as f64) as f32
}
pub(super) fn y_value(y: f32, range: (f64, f64), height: f32) -> f64 {
    range.1 - (y - 8.) as f64 / curve_height(height) as f64 * (range.1 - range.0)
}
fn curve_height(height: f32) -> f32 {
    // Preserve the normal event lane, but share the limited height in short docks.
    (height - ROW.min(height * 0.2) - 16.).max(1.)
}
pub(super) fn track_range(projection: &TimelineProjection, track: &str) -> (f64, f64) {
    projection
        .channel(&projection.tracks[track].channel)
        .and_then(|channel| channel.range)
        .map_or((0., 1.), |(a, b)| {
            if a < b {
                (a as f64, b as f64)
            } else {
                (a as f64 - 0.5, b as f64 + 0.5)
            }
        })
}
fn display_value(value: f64, normalized: bool, range: (f64, f64)) -> f64 {
    if normalized {
        (value - range.0) / (range.1 - range.0)
    } else {
        value
    }
}
fn selected_tracks<'a>(
    projection: &'a TimelineProjection,
    state: &TimelineViewState,
) -> Vec<&'a TimelineTrack> {
    let mut tracks = vec![];
    for row in &projection.rows {
        if state.channels.contains(&row.id) {
            if let Some(id) = &row.track_id {
                if !tracks.iter().any(|track: &&TimelineTrack| track.id == *id) {
                    tracks.push(&projection.tracks[id]);
                }
            }
        }
    }
    tracks
}

fn unanimated_channels<'a>(
    projection: &'a TimelineProjection,
    state: &'a TimelineViewState,
) -> impl Iterator<Item = &'a TimelineChannel> {
    projection
        .rows
        .iter()
        .filter(|row| state.channels.contains(&row.id))
        .filter_map(|row| row.channel.as_ref().and_then(|id| projection.channel(id)))
        .filter(|channel| channel.tracks.is_empty())
}
fn channel_range(channel: &TimelineChannel) -> (f64, f64) {
    channel.range.map_or((0., 1.), |(a, b)| {
        if a < b {
            (a as f64, b as f64)
        } else {
            (a as f64 - 0.5, b as f64 + 0.5)
        }
    })
}
impl Timeline {
    pub(super) fn update_range(
        &mut self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
    ) {
        let selected: Vec<_> = state.channels.iter().cloned().collect();
        if selected == self.range_selection && state.normalized == self.normalized {
            return;
        }
        if self.gesture.is_some() || self.number_edit.is_some() {
            return;
        }
        self.range_selection = selected;
        self.normalized = state.normalized;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for track in selected_tracks(projection, state) {
            let range = track_range(projection, &track.id);
            for value in [range.0, range.1]
                .into_iter()
                .chain(track.keys.iter().map(|key| key.value as f64))
                .chain(
                    track
                        .curve
                        .segments
                        .iter()
                        .flat_map(|segment| match segment {
                            CompiledSegment::Bezier {
                                control1, control2, ..
                            } => vec![control1.value as f64, control2.value as f64],
                            _ => vec![],
                        }),
                )
            {
                let value = display_value(value, state.normalized, range);
                min = min.min(value);
                max = max.max(value);
            }
        }
        for channel in unanimated_channels(projection, state) {
            let scale = channel_range(channel);
            for value in [scale.0, scale.1, channel.default_value as f64] {
                let value = display_value(value, state.normalized, scale);
                min = min.min(value);
                max = max.max(value);
            }
        }
        if !min.is_finite() {
            min = 0.;
            max = 1.;
        }
        let padding = ((max - min) * 0.1).max(0.01);
        self.value_range = (min - padding, max + padding);
    }
}
fn build_scene(
    key: SceneKey,
    projection: &Arc<TimelineProjection>,
    state: &TimelineViewState,
    rows: &Arc<[TimelineRow]>,
) -> Scene {
    let width = key.size.width.as_f32();
    let height = key.size.height.as_f32();
    let mut scene = Scene {
        key,
        _projection: projection.clone(),
        _rows: rows.clone(),
        strokes: vec![],
        markers: vec![],
    };
    let x = |time| px(time_x(time, state.time_range, width));
    if state.mode == TimelineDisplayMode::Overview {
        let first = (state.scroll / ROW as f64).floor() as usize;
        for (index, row) in rows
            .iter()
            .enumerate()
            .skip(first)
            .take((height / ROW).ceil() as usize + 2)
        {
            let y = px(index as f32 * ROW + ROW / 2. - state.scroll as f32);
            if let Some(track) = row
                .track_id
                .as_ref()
                .and_then(|id| projection.tracks.get(id))
            {
                for key in track.visible_keys(state.time_range.0, state.time_range.1) {
                    scene.markers.push(Marker {
                        position: point(x(key.time), y),
                        hit: Hit::Key(key.reference.clone()),
                    });
                }
            }
            for summary in &row.summary {
                if (state.time_range.0..=state.time_range.1).contains(&summary.time) {
                    scene.markers.push(Marker {
                        position: point(x(summary.time), y),
                        hit: Hit::Summary(summary.keys.clone()),
                    });
                }
            }
            if row.id == TimelineRowId::Events {
                for event in &projection.events {
                    if (state.time_range.0..=state.time_range.1).contains(&f64::from(event.time)) {
                        scene.markers.push(Marker {
                            position: point(x(f64::from(event.time)), y),
                            hit: Hit::Event(event.id.clone()),
                        });
                    }
                }
            }
        }
    } else {
        for (index, track) in selected_tracks(projection, state).into_iter().enumerate() {
            let scale = track_range(projection, &track.id);
            let y = |value: f32| {
                px(value_y(
                    display_value(value as f64, state.normalized, scale),
                    scene.key.range,
                    height,
                ))
            };
            let position = |time: f64, value: f32| point(x(time), y(value));
            // Read through the runtime sampler, including its non-monotonic branch.
            // Step segments get explicit vertical edges rather than diagonal approximations.
            let mut points = vec![];
            let start = state.time_range.0;
            let end = state.time_range.1;
            points.push(position(
                start,
                sample_motion_curve(&track.curve, start as f32, projection.restricted_beziers),
            ));
            let first = track
                .keys
                .partition_point(|key| key.time <= start)
                .saturating_sub(1);
            let count = track
                .keys
                .partition_point(|key| key.time <= end)
                .saturating_sub(first)
                + 1;
            for (segment_index, segment) in track
                .curve
                .segments
                .iter()
                .enumerate()
                .skip(first)
                .take(count)
            {
                let a = &track.keys[segment_index];
                let b = &track.keys[segment_index + 1];
                if b.time < start || a.time > end {
                    continue;
                }
                let left = a.time.max(start);
                let right = b.time.min(end);
                match segment {
                    CompiledSegment::Stepped { .. } => {
                        points.push(position(left, a.value));
                        points.push(position(right, a.value));
                        if b.time <= end {
                            points.push(position(b.time, b.value));
                        }
                    }
                    CompiledSegment::InverseStepped { .. } => {
                        if a.time >= start {
                            points.push(position(a.time, a.value));
                        }
                        points.push(position(left, b.value));
                        points.push(position(right, b.value));
                    }
                    CompiledSegment::Linear { .. } => {
                        points.push(position(
                            left,
                            sample_motion_curve(
                                &track.curve,
                                left as f32,
                                projection.restricted_beziers,
                            ),
                        ));
                        points.push(position(
                            right,
                            sample_motion_curve(
                                &track.curve,
                                right as f32,
                                projection.restricted_beziers,
                            ),
                        ));
                    }
                    CompiledSegment::Bezier { .. } => {
                        let samples = ((time_x(right, state.time_range, width)
                            - time_x(left, state.time_range, width))
                            / 2.)
                            .ceil()
                            .max(2.) as usize;
                        for i in 0..=samples {
                            let time = left + (right - left) * i as f64 / samples as f64;
                            points.push(position(
                                time,
                                sample_motion_curve(
                                    &track.curve,
                                    time as f32,
                                    projection.restricted_beziers,
                                ),
                            ));
                        }
                    }
                }
            }
            points.push(position(
                end,
                sample_motion_curve(&track.curve, end as f32, projection.restricted_beziers),
            ));
            scene.strokes.push(Stroke {
                points,
                color: index,
            });
            for key in track.visible_keys(start, end) {
                scene.markers.push(Marker {
                    position: position(key.time, key.value),
                    hit: Hit::Key(key.reference.clone()),
                });
            }
            for pair in track.keys.windows(2) {
                if !state.keys.contains(&pair[0].reference)
                    && !state.keys.contains(&pair[1].reference)
                {
                    continue;
                }
                if let Some(handles) = projection.display_handles(&pair[1].reference) {
                    for (handle, control, anchor) in [
                        (MotionBezierHandle::Control1, handles.control1, &pair[0]),
                        (MotionBezierHandle::Control2, handles.control2, &pair[1]),
                    ] {
                        let point = position(f64::from(control.time), control.value);
                        scene.strokes.push(Stroke {
                            points: vec![position(anchor.time, anchor.value), point],
                            color: index,
                        });
                        scene.markers.push(Marker {
                            position: point,
                            hit: Hit::Handle {
                                end: pair[1].reference.clone(),
                                handle,
                            },
                        });
                    }
                }
            }
        }
        for channel in unanimated_channels(projection, state) {
            let value = display_value(
                channel.default_value as f64,
                state.normalized,
                channel_range(channel),
            );
            let y = px(value_y(value, scene.key.range, height));
            scene.strokes.push(Stroke {
                points: vec![point(px(0.), y), point(px(width), y)],
                color: scene.strokes.len(),
            });
        }
        for event in &projection.events {
            if (state.time_range.0..=state.time_range.1).contains(&f64::from(event.time)) {
                scene.markers.push(Marker {
                    position: point(x(f64::from(event.time)), px(height - ROW / 2.)),
                    hit: Hit::Event(event.id.clone()),
                });
            }
        }
    }
    scene
}
fn color(index: usize) -> Hsla {
    rgb([0x55a9ed, 0xe7a65e, 0x79bb83, 0xc58de7, 0xe77e98, 0x61bfc3][index % 6]).into()
}
fn line(window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, color: Hsla, width: f32) {
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(a);
    path.line_to(b);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
fn diamond(window: &mut Window, p: Point<Pixels>, r: f32, color: Hsla) {
    let mut path = PathBuilder::fill();
    path.move_to(p + point(px(0.), px(-r)));
    path.line_to(p + point(px(r), px(0.)));
    path.line_to(p + point(px(0.), px(r)));
    path.line_to(p + point(px(-r), px(0.)));
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
fn label(window: &mut Window, cx: &mut App, origin: Point<Pixels>, text: String, color: Hsla) {
    let style = window.text_style();
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text.into(), px(10.), &[run], None);
    let _ = shaped.paint(origin, px(14.), TextAlign::Left, None, window, cx);
}
fn ticks(range: (f64, f64), width: f32, fps: f32, seconds: bool) -> Vec<f64> {
    let raw = (range.1 - range.0) / (width / 70.).max(1.) as f64;
    let units = if seconds { 1. } else { fps as f64 };
    let power = 10f64.powf((raw * units).max(1e-9).log10().floor());
    let step = [1., 2., 5., 10.]
        .into_iter()
        .map(|n| power * n)
        .find(|step| *step >= raw * units)
        .unwrap()
        / units;
    let start = (range.0 / step).ceil() * step;
    (0..256)
        .map(|i| start + i as f64 * step)
        .take_while(|time| *time <= range.1)
        .collect()
}
fn tick_label(time: f64, step: f64, fps: f32, seconds: bool) -> String {
    let units = if seconds { 1. } else { fps as f64 };
    let precision = (-(step * units).max(1e-9).log10().floor()).clamp(0., 9.) as usize;
    if seconds {
        format!("{time:.precision$}s", precision = precision.max(2))
    } else {
        format!("{:.precision$}", time * units)
    }
}
pub(super) fn paint_ruler(
    bounds: Bounds<Pixels>,
    projection: &TimelineProjection,
    state: &TimelineViewState,
    head: f32,
    seconds: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let foreground = cx.theme().muted_foreground;
    let primary = cx.theme().primary;
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let marks = ticks(
            state.time_range,
            bounds.size.width.as_f32(),
            projection.fps,
            seconds,
        );
        let step = marks
            .get(1)
            .zip(marks.first())
            .map_or(state.time_range.1 - state.time_range.0, |(b, a)| b - a);
        for time in marks {
            let x = time_x(time, state.time_range, bounds.size.width.as_f32());
            label(
                window,
                cx,
                bounds.origin + point(px(x + 3.), px(1.)),
                tick_label(time, step, projection.fps, seconds),
                foreground,
            );
            line(
                window,
                bounds.origin + point(px(x), px(RULER - 5.)),
                bounds.origin + point(px(x), px(RULER)),
                foreground,
                1.,
            );
        }
        let x = time_x(head as f64, state.time_range, bounds.size.width.as_f32());
        diamond(
            window,
            bounds.origin + point(px(x), px(RULER - 5.)),
            4.,
            primary,
        );
    });
}

/// Cached GPUI subtree: playback invalidates the overlay, not these tracks.
pub(super) struct TracksLayer {
    cache: Rc<std::cell::RefCell<SceneCache>>,
    input: Option<LayerInput>,
}
struct LayerInput {
    projection: Arc<TimelineProjection>,
    state: TimelineViewState,
    rows: Arc<[TimelineRow]>,
    range: (f64, f64),
    seconds: bool,
    hint: String,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    colors: (Hsla, Hsla, Hsla, Hsla, Hsla),
}
impl TracksLayer {
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.input.take().is_some() {
            self.cache.borrow_mut().scene = None;
            cx.notify();
        }
    }
    pub fn new(cache: Rc<std::cell::RefCell<SceneCache>>) -> Self {
        Self { cache, input: None }
    }
    pub fn configure(
        &mut self,
        projection: Arc<TimelineProjection>,
        state: TimelineViewState,
        rows: Arc<[TimelineRow]>,
        range: (f64, f64),
        seconds: bool,
        bounds: Rc<Cell<Bounds<Pixels>>>,
        cx: &mut Context<Self>,
    ) {
        let theme = cx.theme();
        let colors = (
            theme.background,
            theme.border,
            theme.primary,
            theme.muted,
            theme.muted_foreground,
        );
        let hint = t!("timeline.select_curves");
        let same = self.input.as_ref().is_some_and(|old| {
            Arc::ptr_eq(&old.projection, &projection)
                && old.state.time_range == state.time_range
                && old.state.scroll == state.scroll
                && old.state.mode == state.mode
                && old.state.channels == state.channels
                && old.state.keys == state.keys
                && old.state.events == state.events
                && old.state.normalized == state.normalized
                && old.range == range
                && old.seconds == seconds
                && old.hint == hint.as_ref()
                && old.colors == colors
                && Arc::ptr_eq(&old.rows, &rows)
        });
        if !same {
            self.input = Some(LayerInput {
                projection,
                state,
                rows,
                range,
                seconds,
                hint: hint.into_owned(),
                bounds,
                colors,
            });
            cx.notify();
        }
    }
}
impl Render for TracksLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let Some(input) = &self.input else {
            return div().into_any_element();
        };
        let cache = self.cache.clone();
        let measuring = input.bounds.clone();
        let projection = input.projection.clone();
        let state = input.state.clone();
        let rows = input.rows.clone();
        let range = input.range;
        let seconds = input.seconds;
        canvas(
            move |bounds, _, _| measuring.set(bounds),
            move |bounds, _, window, cx| {
                paint_scene(
                    &mut cache.borrow_mut(),
                    bounds,
                    &projection,
                    &state,
                    &rows,
                    range,
                    seconds,
                    window,
                    cx,
                );
            },
        )
        .size_full()
        .into_any_element()
    }
}
pub(super) fn paint_head(
    bounds: Bounds<Pixels>,
    range: (f64, f64),
    head: f32,
    marquee: Option<(Point<f32>, Point<f32>)>,
    color: Hsla,
    window: &mut Window,
) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let x = time_x(f64::from(head), range, bounds.size.width.as_f32());
        line(
            window,
            bounds.origin + point(px(x), px(0.)),
            bounds.origin + point(px(x), bounds.size.height),
            color,
            1.5,
        );
        if let Some((a, b)) = marquee {
            let min = point(px(a.x.min(b.x)), px(a.y.min(b.y)));
            let max = point(px(a.x.max(b.x)), px(a.y.max(b.y)));
            window.paint_quad(fill(
                Bounds::from_corners(bounds.origin + min, bounds.origin + max),
                color.opacity(0.15),
            ));
        }
    });
}

fn paint_scene(
    cache: &mut SceneCache,
    bounds: Bounds<Pixels>,
    projection: &Arc<TimelineProjection>,
    state: &TimelineViewState,
    rows: &Arc<[TimelineRow]>,
    range: (f64, f64),
    seconds: bool,
    window: &mut Window,
    cx: &mut App,
) {
    cache.paints += 1;
    let key = SceneKey {
        projection: Arc::as_ptr(projection) as usize,
        rows: rows.as_ptr() as usize,
        size: bounds.size,
        time: state.time_range,
        scroll: state.scroll,
        mode: state.mode,
        channels: state.channels.iter().cloned().collect(),
        keys: state.keys.iter().cloned().collect(),
        events: state.events.iter().cloned().collect(),
        range,
        normalized: state.normalized,
    };
    if cache.scene.as_ref().is_none_or(|scene| scene.key != key) {
        cache.scene = Some(build_scene(key, projection, state, rows));
        cache.builds += 1;
    }
    let scene = cache.scene.as_ref().unwrap();
    let theme = cx.theme().clone();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let width = bounds.size.width.as_f32();
        let height = bounds.size.height.as_f32();
        let end = time_x(projection.duration, state.time_range, width);
        if end < width {
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + point(px(end.max(0.)), px(0.)),
                    size(px(width - end.max(0.)), bounds.size.height),
                ),
                theme.muted.opacity(0.4),
            ));
        }
        for tick in ticks(state.time_range, width, projection.fps, seconds) {
            let x = time_x(tick, state.time_range, width);
            line(
                window,
                bounds.origin + point(px(x), px(0.)),
                bounds.origin + point(px(x), px(height)),
                theme.border.opacity(0.45),
                1.,
            );
        }
        if state.mode == TimelineDisplayMode::Overview {
            let first = (state.scroll / ROW as f64).floor() as usize;
            for index in first..rows.len().min(first + (height / ROW).ceil() as usize + 2) {
                let y = index as f32 * ROW - state.scroll as f32;
                if state.channels.contains(&rows[index].id) {
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(0.), px(y)),
                            size(bounds.size.width, px(ROW)),
                        ),
                        theme.accent.opacity(0.08),
                    ));
                }
                line(
                    window,
                    bounds.origin + point(px(0.), px(y + ROW)),
                    bounds.origin + point(px(width), px(y + ROW)),
                    theme.border.opacity(0.4),
                    1.,
                );
            }
        } else {
            let intervals = (curve_height(height) / 28.).floor().clamp(1., 4.) as usize;
            for n in 0..=intervals {
                let value = range.0 + (range.1 - range.0) * n as f64 / intervals as f64;
                let y = value_y(value, range, height);
                line(
                    window,
                    bounds.origin + point(px(0.), px(y)),
                    bounds.origin + point(px(width), px(y)),
                    theme.border.opacity(0.4),
                    1.,
                );
                label(
                    window,
                    cx,
                    bounds.origin + point(px(4.), px((y - 7.).clamp(0., (height - 14.).max(0.)))),
                    format!("{value:.2}"),
                    theme.muted_foreground,
                );
            }
            if scene.strokes.is_empty() {
                label(
                    window,
                    cx,
                    bounds.origin + point(px(12.), px(12.)),
                    t!("timeline.select_curves").into_owned(),
                    theme.muted_foreground,
                );
            }
        }
        for stroke in &scene.strokes {
            let mut path = PathBuilder::stroke(px(1.5));
            if let Some(first) = stroke.points.first() {
                path.move_to(bounds.origin + *first);
                for point in &stroke.points[1..] {
                    path.line_to(bounds.origin + *point);
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color(stroke.color));
                }
            }
        }
        for marker in &scene.markers {
            let selected = match &marker.hit {
                Hit::Key(key) => state.keys.contains(key),
                Hit::Event(id) => state.events.contains(id),
                Hit::Handle { .. } => true,
                Hit::Summary(_) => false,
            };
            let color = if selected {
                theme.primary
            } else {
                theme.muted_foreground
            };
            if matches!(marker.hit, Hit::Handle { .. }) {
                window.paint_quad(
                    fill(
                        Bounds::new(
                            bounds.origin + marker.position - point(px(3.5), px(3.5)),
                            size(px(7.), px(7.)),
                        ),
                        color,
                    )
                    .corner_radii(px(3.5)),
                );
                window.paint_quad(
                    fill(
                        Bounds::new(
                            bounds.origin + marker.position - point(px(2.), px(2.)),
                            size(px(4.), px(4.)),
                        ),
                        theme.background,
                    )
                    .corner_radii(px(2.)),
                );
            } else {
                diamond(window, bounds.origin + marker.position, 4.5, color);
            }
        }
    });
}
