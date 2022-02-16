use bitflags::bitflags;
use math::round;
use std::cmp;
use std::fmt;
use std::iter;
use std::time::Duration;

bitflags! {
    pub struct LapState:i32 {
        const YELLOW = 1;
        const PITTED = 2;
        const PACE_LAP = 4;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Lap {
    fuel_used: f32,
    fuel_left: f32,
    time: Duration,
    condition: LapState,
}

#[derive(Clone, Copy, Debug)]
pub struct RaceConfig {
    laps: Option<i32>,
    time: Option<Duration>,
    fuel_tank_size: f32,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rate {
    fuel: f32,
    time: Duration,
}
impl Default for Rate {
    fn default() -> Self {
        Rate {
            fuel: 0.0,
            time: Duration::new(0, 0),
        }
    }
}
impl Rate {
    fn add(&self, l: &Lap) -> Rate {
        Rate {
            fuel: self.fuel + l.fuel_used,
            time: self.time + l.time,
        }
    }
}
pub struct Calculator {
    cfg: RaceConfig,
    laps: Vec<Lap>,
    race_lap: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pitstop {
    open: i32,
    close: i32,
}
impl Pitstop {
    fn new(open: i32, close: i32) -> Pitstop {
        Pitstop {
            open: open,
            close: close,
        }
    }
}
impl fmt::Display for Pitstop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Pitstop window opens:{}, closes:{}",
            self.open, self.close
        )
    }
}

#[derive(Clone, Debug)]
pub struct Strategy {
    stints: Vec<i32>,
    stops: Vec<Pitstop>,
}

struct StratRequest {
    fuel_left: f32,
    tank_size: f32,
    yellow_togo: i32,
    green_laps_togo: Option<i32>,
    time_togo: Option<Duration>,
    green: Rate,
    yellow: Rate,
}
impl Calculator {
    pub fn new(cfg: RaceConfig) -> Calculator {
        Calculator {
            cfg: cfg,
            laps: Vec::with_capacity(16),
            race_lap: 0,
        }
    }
    pub fn add_lap(&mut self, l: Lap, race_lap: i32) {
        self.laps.push(l);
        self.race_lap = race_lap;
    }
    fn recent_green(&self) -> Option<Rate> {
        let (c, r) = self
            .laps
            .iter()
            .rev()
            .filter(|&l| l.condition.is_empty())
            .take(5)
            .fold((0, Rate::default()), |acc, lap| (acc.0 + 1, acc.1.add(lap)));
        if c == 0 {
            None
        } else {
            Some(Rate {
                fuel: r.fuel / (c as f32),
                time: r.time / c,
            })
        }
    }
    fn laps_remaining(&self) -> i32 {
        if self.cfg.laps.is_some() {
            self.cfg.laps.unwrap() - self.race_lap + 1
        } else if self.cfg.time.is_some() {
            13
        } else {
            42
        }
    }
    pub fn strat(&self) -> Vec<Strategy> {
        let green = self.recent_green();
        if green.is_none() {
            return vec![];
        }
        let yellow = Rate {
            fuel: green.unwrap().fuel / 4.0,
            time: green.unwrap().time.mul_f32(4.0),
        };
        let r = StratRequest {
            fuel_left: self.laps.last().unwrap().fuel_left,
            tank_size: self.cfg.fuel_tank_size,
            yellow_togo: 0,
            green_laps_togo: Some(self.laps_remaining()),
            time_togo: None,
            green: green.unwrap(),
            yellow: yellow,
        };
        let fwd = strat_fwd(&r);
        vec![fwd]
    }
}

// a strategy that starts now, and repeatedly runs the tank dry until we're done
fn strat_fwd(r: &StratRequest) -> Strategy {
    let mut stints = Vec::with_capacity(4);
    let mut f = r.fuel_left;
    let yellow = iter::repeat(r.yellow).take(r.yellow_togo as usize);
    let green = match r.green_laps_togo {
        // the take(i32:MAX) allows both match arms to return the same type (a Take<T>)
        // ideally green would be Iter<T> but that's a trait, and you'd need to box the value for that to be possible.
        // https://stackoverflow.com/questions/26378842/how-do-i-overcome-match-arms-with-incompatible-types-for-structs-implementing-sa
        None => iter::repeat(r.green).take(i32::MAX as usize),
        Some(g) => iter::repeat(r.green).take(g as usize),
    };
    let rates = yellow.chain(green);
    let mut stint = 0;
    for rt in rates {
        if f < rt.fuel {
            stints.push(stint);
            stint = 0;
            f = r.tank_size;
        }
        f -= rt.fuel;
        stint += 1;
    }
    if stint > 0 {
        stints.push(stint);
    }

    let full_stint_len = round::floor((r.tank_size / r.green.fuel) as f64, 0) as i32;
    let max_stint_laps = (stints.len() as i32 - 1) * full_stint_len;
    let act_stint_laps: i32 = stints.iter().skip(1).sum();
    let mut stops = Vec::with_capacity(stints.len());
    let mut lap_open = 0;
    let mut lap_close = 0;
    let mut ext = max_stint_laps - act_stint_laps;
    for i in 0..stints.len() - 1 {
        // we can bring this stop forward by extending a later stop
        let wdw_size = cmp::min(ext, stints[i]);
        stops.push(Pitstop {
            open: lap_open + stints[i] - wdw_size,
            close: lap_close + stints[i],
        });
        lap_open += stints[i] - wdw_size;
        lap_close += stints[i];
        ext -= wdw_size;
    }
    Strategy {
        stints: stints,
        stops: stops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_add() {
        let s = Rate {
            fuel: 0.5,
            time: Duration::new(3, 0),
        };
        let l = Lap {
            fuel_used: 0.3,
            fuel_left: 4.1,
            time: Duration::new(5, 0),
            condition: LapState::empty(),
        };
        let r = s.add(&l);
        assert_eq!(
            r,
            Rate {
                fuel: 0.8,
                time: Duration::new(8, 0)
            }
        );
    }

    #[test]
    fn strat_no_stops() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 20.0,
            yellow_togo: 0,
            green_laps_togo: Some(5),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![5], s.stints);
        assert_eq!(Vec::<Pitstop>::new(), s.stops);
    }

    #[test]
    fn strat_one_stop() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: Some(34),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![19, 15], s.stints);
        assert_eq!(vec![Pitstop::new(14, 19)], s.stops);
    }

    #[test]
    fn strat_one_stop_yellow() {
        let d = Duration::new(25, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            yellow_togo: 3,
            green_laps_togo: Some(20),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: d.mul_f32(5.0),
            },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![21, 2], s.stints);
        assert_eq!(vec![Pitstop::new(3, 21)], s.stops);
    }

    #[test]
    fn strat_two_stops() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: Some(49),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![18, 20, 11], s.stints);
        assert_eq!(vec![Pitstop::new(9, 18), Pitstop::new(29, 38)], s.stops);
    }

    #[test]
    fn strat_one_stop_big_window() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: Some(24),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![18, 6], s.stints);
        assert_eq!(vec![Pitstop::new(4, 18)], s.stops);
    }

    #[test]
    fn strat_two_stops_with_splash() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 1.5,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: Some(29),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![3, 20, 6], s.stints);
        assert_eq!(vec![Pitstop::new(0, 3), Pitstop::new(9, 23)], s.stops);
    }

    #[test]
    fn strat_two_stops_only_just() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.6,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: Some(58),
            time_togo: None,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![19, 20, 19], s.stints);
        assert_eq!(vec![Pitstop::new(18, 19), Pitstop::new(38, 39)], s.stops);
    }
}
