//! Procedural blink state captured with the motion replay checkpoints.
use crate::parameter_values::{repeat_value, set_value};
use std::collections::{BTreeMap, BTreeSet};
use zipzi_core::state::DocumentState;

const CLOSING_SECONDS: f64 = 0.1;
const CLOSED_SECONDS: f64 = 0.05;
const OPENING_SECONDS: f64 = 0.15;
// Cubism Native 5 R5 draws a wait from 0 to (2 * interval - 1), with interval 4.
const MAX_WAIT_SECONDS: f64 = 7.;

#[derive(Clone, Copy)]
enum Phase {
    Waiting(f64),
    Closing(f64),
    Closed(f64),
    Opening(f64),
}

#[derive(Clone)]
pub(crate) struct AutoEyeBlink {
    seed: u32,
    random: u32,
    time: f64,
    phase: Phase,
    overridden: BTreeSet<String>,
}

impl AutoEyeBlink {
    pub(crate) fn new(seed: u32) -> Self {
        let mut blink = Self {
            seed,
            random: seed,
            time: 0.,
            phase: Phase::Waiting(0.),
            overridden: BTreeSet::new(),
        };
        blink.wait();
        blink
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    pub(crate) fn set_input_override(&mut self, id: &str, active: bool) {
        if active {
            if !self.overridden.contains(id) {
                self.overridden.insert(id.into());
            }
        } else {
            self.overridden.remove(id);
        }
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.overridden.iter().map(|id| 512 + id.capacity()).sum()
    }

    fn wait(&mut self) {
        // A local generator makes replay independent of UI frame cadence and
        // other previews sharing the process.
        self.random = self
            .random
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let wait = MAX_WAIT_SECONDS * f64::from(self.random) / f64::from(u32::MAX);
        self.phase = Phase::Waiting(self.time + wait);
    }

    fn sample(&mut self, dt: f32) -> f32 {
        self.time += f64::from(dt);
        let time = self.time;
        match self.phase {
            Phase::Waiting(until) => {
                if time > until {
                    self.phase = Phase::Closing(time);
                }
                1.
            }
            Phase::Closing(start) => {
                let fraction = ((time - start) / CLOSING_SECONDS).min(1.);
                if fraction == 1. {
                    self.phase = Phase::Closed(time);
                }
                (1. - fraction) as f32
            }
            Phase::Closed(start) => {
                if time - start >= CLOSED_SECONDS {
                    self.phase = Phase::Opening(time);
                }
                0.
            }
            Phase::Opening(start) => {
                let fraction = ((time - start) / OPENING_SECONDS).min(1.);
                if fraction == 1. {
                    self.wait();
                }
                fraction as f32
            }
        }
    }

    pub(crate) fn apply(
        &mut self,
        document: &DocumentState,
        parameters: &mut BTreeMap<String, f32>,
        dt: f32,
    ) {
        let Some(group) = document.model3_settings().parameter_group_named("EyeBlink") else {
            return;
        };
        let value = self.sample(dt);
        for id in group
            .parameters
            .iter()
            .filter_map(|entry| entry.target.object_id())
            .filter(|id| !self.overridden.contains(*id))
        {
            let parameter = document.parameter(id).expect("resolved blink member");
            set_value(
                parameters,
                id,
                repeat_value(parameter, value).clamp(parameter.minimum, parameter.maximum),
            );
        }
    }
}
