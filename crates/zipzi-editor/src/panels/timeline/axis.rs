//! Ticks are chosen in readable units, then mapped into the shared curve space.
use super::drawing::{channel_range, curve_height, display_value, value_y, y_value};
use super::*;

pub(super) fn channels<'a>(
    projection: &'a TimelineProjection,
    state: &TimelineViewState,
) -> Vec<&'a TimelineChannel> {
    let mut channels = Vec::new();
    for row in projection
        .rows
        .iter()
        .filter(|row| state.channels.contains(&row.id))
    {
        if let Some(channel) = row.channel.as_ref().and_then(|id| projection.channel(id)) {
            if !channels
                .iter()
                .any(|selected: &&TimelineChannel| selected.id == channel.id)
            {
                channels.push(channel);
            }
        }
    }
    channels
}

pub(super) fn reference<'a>(
    channels: &[&'a TimelineChannel],
    state: &TimelineViewState,
) -> Option<&'a TimelineChannel> {
    if channels.len() == 1 {
        channels.first().copied()
    } else {
        channels
            .iter()
            .copied()
            .find(|channel| Some(&channel.id) == state.curve_focus.as_ref())
    }
}

#[derive(PartialEq)]
pub(super) struct RangeKey {
    clip: String,
    selection: Vec<TimelineRowId>,
    ranges: Vec<(zipzi_editor_core::TimelineChannelId, Option<(f32, f32)>)>,
    normalized: bool,
}
impl RangeKey {
    pub fn new(projection: &TimelineProjection, state: &TimelineViewState) -> Self {
        Self {
            clip: projection.clip_id.clone(),
            selection: state.channels.iter().cloned().collect(),
            ranges: channels(projection, state)
                .iter()
                .map(|channel| (channel.id.clone(), channel.range))
                .collect(),
            normalized: state.normalized,
        }
    }
}

pub(super) struct Tick {
    pub value: f64,
    pub label: String,
    pub boundary: bool,
}

pub(super) fn number(value: f64) -> String {
    let value = value as f32;
    if value == 0. {
        "0".into()
    } else {
        value.to_string()
    }
}

/// Affine conversion from curve coordinates to the units printed on the ruler.
#[derive(Clone, Copy)]
struct Units {
    offset: f64,
    factor: f64,
    percent: bool,
}
impl Units {
    fn new(normalized: bool, reference: Option<&TimelineChannel>) -> Self {
        if !normalized {
            Self {
                offset: 0.,
                factor: 1.,
                percent: false,
            }
        } else if let Some(channel) = reference {
            let (min, max) = channel_range(channel);
            Self {
                offset: min,
                factor: max - min,
                percent: false,
            }
        } else {
            Self {
                offset: 0.,
                factor: 100.,
                percent: true,
            }
        }
    }
    fn read(self, value: f64) -> f64 {
        self.offset + value * self.factor
    }
    fn world(self, value: f64) -> f64 {
        (value - self.offset) / self.factor
    }
    fn label(self, value: f64) -> String {
        if self.percent {
            format!("{}%", number(value))
        } else {
            number(value)
        }
    }
}

fn positions(range: (f64, f64), height: f32, row_height: f32) -> Vec<f64> {
    let count = (curve_height(height, row_height) / (36. * row_height / ROW)).max(1.);
    let raw = (range.1 - range.0) / f64::from(count);
    let power = 10f64.powf(raw.max(1e-12).log10().floor());
    let step = [1., 2., 2.5, 5., 10.]
        .into_iter()
        .map(|n| n * power)
        .min_by(|a, b| (a / raw).ln().abs().total_cmp(&(b / raw).ln().abs()))
        .unwrap();
    let start = (range.0 / step - 1e-10).ceil();
    (0..256)
        .map(|index| (start + f64::from(index)) * step)
        .take_while(|value| *value <= range.1 + step * 1e-10)
        .collect()
}

pub(super) fn ticks(
    projection: &TimelineProjection,
    state: &TimelineViewState,
    range: (f64, f64),
    height: f32,
    row_height: f32,
) -> Vec<Tick> {
    let channels = channels(projection, state);
    let reference = reference(&channels, state);
    let units = Units::new(state.normalized, reference);
    // Include the plot's padding. A bound must not disappear while its line is
    // still visible just beyond the camera's nominal range during a pan.
    let visible = (
        y_value(
            height - row_height.min(height * 0.2),
            range,
            height,
            row_height,
        ),
        y_value(0., range, height, row_height),
    );
    let mut ticks: Vec<_> = positions(
        (units.read(visible.0), units.read(visible.1)),
        height,
        row_height,
    )
    .into_iter()
    .map(|value| Tick {
        value: units.world(value),
        label: units.label(value),
        boundary: false,
    })
    .collect();
    // Shared normalized bounds need no concatenated per-channel values. Their
    // actual ranges live on the rows; the focused ruler has just its own bounds.
    let bounds = reference
        .and_then(|channel| channel.range)
        .map(|(min, max)| (f64::from(min), f64::from(max)))
        .or_else(|| (state.normalized && reference.is_none()).then_some((0., 1.)));
    if let Some((min, max)) = bounds {
        let scale = reference.map(channel_range).unwrap_or((0., 1.));
        for (raw, key) in [(min, "properties.minimum"), (max, "properties.maximum")] {
            let value = display_value(raw, state.normalized, scale);
            if value < visible.0 - 1e-9 || value > visible.1 + 1e-9 {
                continue;
            }
            let label = format!("{} · {}", units.label(units.read(value)), t!(key));
            if let Some(tick) = ticks
                .iter_mut()
                .find(|tick| (tick.value - value).abs() < 1e-9)
            {
                if tick.boundary {
                    tick.label.push_str(&format!(" / {}", t!(key)));
                } else {
                    tick.label = label;
                    tick.boundary = true;
                }
            } else {
                ticks.push(Tick {
                    value,
                    label,
                    boundary: true,
                });
            }
        }
    }
    // Keep every grid line. Only suppress a normal label too close to a bound.
    let boundaries: Vec<_> = ticks
        .iter()
        .filter(|tick| tick.boundary)
        .map(|tick| value_y(tick.value, range, height, row_height))
        .collect();
    for tick in ticks.iter_mut().filter(|tick| !tick.boundary) {
        if boundaries.iter().any(|y| {
            (value_y(tick.value, range, height, row_height) - y).abs() < 16. * row_height / ROW
        }) {
            tick.label.clear();
        }
    }
    ticks.sort_by(|a, b| a.value.total_cmp(&b.value));
    ticks
}

#[cfg(test)]
mod tests {
    use super::{ROW, Units, positions, value_y};

    #[test]
    fn readable_steps_are_chosen_before_normalized_mapping() {
        let units = Units {
            offset: -30.,
            factor: 60.,
            percent: false,
        };
        let ticks = positions((-36., 36.), 300., ROW);
        assert!(
            ticks.contains(&-20.)
                && ticks.contains(&-10.)
                && ticks.contains(&10.)
                && ticks.contains(&20.)
        );
        for value in ticks {
            assert!((units.read(units.world(value)) - value).abs() < 1e-9);
        }
        assert_eq!(units.label(10.), "10");
        assert_eq!(Units::new(true, None).label(50.), "50%");
    }

    #[test]
    fn panning_keeps_tick_values_anchored_to_curve_coordinates() {
        let a = positions((-36., 36.), 300., ROW);
        let b = positions((-32.4, 39.6), 300., ROW);
        assert!(a.contains(&10.) && b.contains(&10.));
        let delta = value_y(10., (-32.4, 39.6), 300., ROW) - value_y(10., (-36., 36.), 300., ROW);
        let key_delta =
            value_y(15., (-32.4, 39.6), 300., ROW) - value_y(15., (-36., 36.), 300., ROW);
        assert!((delta - key_delta).abs() < 0.0001);
    }
}
