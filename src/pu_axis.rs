use super::{PuAxis, PuAxisConfig};
use std::collections::VecDeque;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HomePhase {
    Approach,
    Brake,
    Leaving,
    Reenter,
    SeekZ,
    Offset,
}
#[derive(Debug, Clone)]
pub(super) enum RunMode {
    Train,
    Ramp,
    Jog,
    Mpg,
    Home {
        mode: u8,
        phase: HomePhase,
        leave_negative: bool,
        jog: u32,
        z_left: u32,
        dog: bool,
    },
}
#[derive(Debug, Clone)]
struct Segment {
    left: f64,
    start: f64,
    end: f64,
}
#[derive(Debug, Clone)]
pub(super) struct AxisRun {
    pub mode: RunMode,
    pub negative: bool,
    pub target_speed: f64,
    pub remaining: Option<u64>,
    pub emitted: u64,
    pub fraction: f64,
    pub reverse_armed: bool,
    pub pending_speed: Option<(u64, i32)>,
    pub mpg_target: i64,
    pub mpg_emitted: i64,
    pub mpg_rate: f64,
    pub stop_on_zero: bool,
    segments: VecDeque<Segment>,
}
impl AxisRun {
    pub fn new(mode: RunMode, negative: bool, count: Option<u64>, speed: f64) -> Self {
        Self {
            mode,
            negative,
            target_speed: speed,
            remaining: count,
            emitted: 0,
            fraction: 0.,
            reverse_armed: speed == 0.,
            pending_speed: None,
            mpg_target: 0,
            mpg_emitted: 0,
            mpg_rate: 1.,
            stop_on_zero: false,
            segments: VecDeque::from([Segment {
                left: f64::INFINITY,
                start: speed,
                end: speed,
            }]),
        }
    }
    pub fn set_constant(&mut self, speed: f64) {
        self.target_speed = speed;
        self.segments = VecDeque::from([Segment {
            left: f64::INFINITY,
            start: speed,
            end: speed,
        }]);
    }
    pub fn speed(&self) -> f64 {
        self.segments.front().map_or(0., |s| s.start)
    }
    fn slope(config: &PuAxisConfig, up: bool) -> f64 {
        let ms = if up {
            config.acceleration_ms
        } else {
            config.deceleration_ms
        };
        if ms == 0 {
            f64::INFINITY
        } else {
            f64::from(config.maximum_hz.saturating_sub(config.start_hz)) / f64::from(ms) * 1000.
        }
    }
    fn segment(start: f64, end: f64, slope: f64) -> Option<Segment> {
        if (start - end).abs() < 1e-9 || !slope.is_finite() || slope <= 0. {
            None
        } else {
            Some(Segment {
                left: (start - end).abs() / slope,
                start,
                end,
            })
        }
    }
    pub fn ramp(&mut self, config: &PuAxisConfig, requested: f64) {
        self.target_speed = requested;
        let current = self.speed();
        self.segments.clear();
        let target = requested.min(f64::from(config.maximum_hz));
        let up = Self::slope(config, true);
        let down = Self::slope(config, false);
        if target == 0. {
            if let Some(s) = Self::segment(current, 0., down) {
                self.segments.push_back(s);
            }
            self.segments.push_back(Segment {
                left: f64::INFINITY,
                start: 0.,
                end: 0.,
            });
            return;
        }
        let end = f64::from(config.start_hz).min(target);
        if let Some(n) = self.remaining {
            let distance = (n as f64 - self.fraction).max(0.);
            let updist = if up.is_finite() && up > 0. {
                (target * target - current * current).max(0.) / (2. * up)
            } else {
                0.
            };
            let downdist = if down.is_finite() && down > 0. {
                (target * target - end * end).max(0.) / (2. * down)
            } else {
                0.
            };
            let mut peak = target;
            if updist + downdist > distance
                && up.is_finite()
                && down.is_finite()
                && up > 0.
                && down > 0.
            {
                peak = ((2. * distance + current * current / up + end * end / down)
                    / (1. / up + 1. / down))
                    .max(0.)
                    .sqrt()
                    .min(target)
                    .max(current.min(target));
            }
            let first_slope = if peak >= current { up } else { down };
            if let Some(s) = Self::segment(current, peak, first_slope) {
                self.segments.push_back(s);
            }
            let d1 = if first_slope.is_finite() && first_slope > 0. {
                (peak * peak - current * current).abs() / (2. * first_slope)
            } else {
                0.
            };
            let d2 = if down.is_finite() && down > 0. {
                (peak * peak - end * end).max(0.) / (2. * down)
            } else {
                0.
            };
            let cruise = (distance - d1 - d2).max(0.);
            if cruise > 0. && peak > 0. {
                self.segments.push_back(Segment {
                    left: cruise / peak,
                    start: peak,
                    end: peak,
                });
            }
            if let Some(s) = Self::segment(peak, end, down) {
                self.segments.push_back(s);
            }
            self.segments.push_back(Segment {
                left: f64::INFINITY,
                start: end,
                end,
            });
        } else {
            if let Some(s) =
                Self::segment(current, target, if target >= current { up } else { down })
            {
                self.segments.push_back(s);
            }
            self.segments.push_back(Segment {
                left: f64::INFINITY,
                start: target,
                end: target,
            });
        }
    }
    fn time_for(segment: &Segment, pulses: f64) -> Option<f64> {
        if pulses <= 0. {
            return Some(0.);
        }
        let a = if segment.left.is_finite() && segment.left > 0. {
            (segment.end - segment.start) / segment.left
        } else {
            0.
        };
        if a.abs() < 1e-12 {
            (segment.start > 0.).then_some(pulses / segment.start)
        } else {
            let disc = segment.start * segment.start + 2. * a * pulses;
            if disc < 0. {
                None
            } else {
                let denominator = segment.start + disc.sqrt();
                (denominator > 0.).then_some(2. * pulses / denominator)
            }
        }
    }
    pub fn next_deadline(&self, axis: &PuAxis, now: u64) -> Option<u64> {
        if self.remaining == Some(0) {
            return None;
        }
        let segment = self.segments.front()?;
        let seconds = self
            .remaining
            .and_then(|n| Self::time_for(segment, (n as f64 - self.fraction).max(0.)))
            .filter(|t| *t <= segment.left + 1e-9)
            .into_iter()
            .chain(segment.left.is_finite().then_some(segment.left))
            .filter(|t| *t >= 0.)
            .min_by(f64::total_cmp);
        let boundary = if self.target_speed > 0.
            && !matches!(self.mode, RunMode::Home { .. })
            && (axis.config.negative_limit != 0 || axis.config.positive_limit != 0)
        {
            let distance = if self.negative {
                i64::from(axis.position) - i64::from(axis.config.negative_limit)
            } else {
                i64::from(axis.config.positive_limit) - i64::from(axis.position)
            };
            Self::time_for(segment, (distance.max(0) as f64 - self.fraction).max(0.))
                .filter(|t| *t <= segment.left + 1e-9)
                .map(|t| now.saturating_add((t * 1e6).ceil().max(1.) as u64))
        } else {
            None
        };
        seconds
            .map(|t| now.saturating_add((t * 1e6).ceil().max(1.) as u64))
            .into_iter()
            .chain(boundary)
            .chain(self.pending_speed.map(|p| p.0))
            .min()
    }
    pub fn next_pulse_us(&self, now: u64) -> u64 {
        let t = self
            .segments
            .front()
            .and_then(|s| Self::time_for(s, 1. - self.fraction))
            .unwrap_or(0.);
        now.saturating_add((t * 1e6).ceil().max(1.) as u64)
    }
    pub fn advance(&mut self, axis: &mut PuAxis, dt: u64) {
        let mut seconds = dt as f64 / 1e6;
        let mut iterations = 0;
        while seconds > 1e-12 && self.remaining != Some(0) {
            iterations += 1;
            if iterations > 16 {
                break;
            }
            let Some(segment) = self.segments.front_mut() else {
                break;
            };
            let used = seconds.min(segment.left);
            let a = if segment.left.is_finite() && segment.left > 0. {
                (segment.end - segment.start) / segment.left
            } else {
                0.
            };
            let integrated = (segment.start * used + 0.5 * a * used * used).max(0.);
            let total = self.fraction + integrated;
            let mut count = (total + 1e-9).floor() as u64;
            if let Some(n) = self.remaining {
                count = count.min(n);
            }
            self.fraction = (total - count as f64).clamp(0., 0.999999999999);
            if let Some(n) = &mut self.remaining {
                *n = n.saturating_sub(count);
            }
            self.emitted = self.emitted.saturating_add(count);
            let signed = if self.negative {
                (count as i64).wrapping_neg()
            } else {
                count as i64
            };
            axis.position = axis.position.wrapping_add(signed as i32);
            self.mpg_emitted = self.mpg_emitted.wrapping_add(signed);
            seconds -= used;
            segment.start = (segment.start + a * used).max(0.);
            segment.left -= used;
            if segment.left <= 1e-10 {
                self.segments.pop_front();
            }
        }
        axis.state_negative = self.negative;
        axis.speed_hz = if self.negative {
            -self.speed()
        } else {
            self.speed()
        };
        axis.paused = self.speed() == 0. && self.remaining != Some(0);
        let high =
            axis.busy && self.speed() > 0. && self.remaining != Some(0) && self.fraction < 0.5;
        axis.outputs = match axis.config.mode {
            0 => [high, false],
            1 => [high, self.negative],
            2 => [high && !self.negative, high && self.negative],
            _ => {
                let q = self.fraction;
                let b = axis.busy && self.speed() > 0. && (0.25..0.75).contains(&q);
                if self.negative { [b, high] } else { [high, b] }
            }
        };
    }
    pub fn completed(&self) -> bool {
        self.remaining == Some(0) || self.stop_on_zero && self.speed() == 0.
    }
}
