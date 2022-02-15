use bitflags::bitflags;
use math::round;
use std::cmp;
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

#[derive(Clone, Copy, Debug)]
pub struct Pitstop {
    open: i32,
    close: i32,
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
    green_laps_togo: i32,
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
            green_laps_togo: self.laps_remaining(),
            green: green.unwrap(),
            yellow: yellow,
        };
        let fwd = strat_fwd(&r);
        let rev = strat_rev(&r);
        vec![fwd, rev]
    }
}

// a strategy that starts now, and repeatedly runs the tank dry until we're done
fn strat_fwd(r: &StratRequest) -> Strategy {
    let mut stints = Vec::with_capacity(4);
    let mut f = r.fuel_left;
    let mut togo = r.green_laps_togo;
    while togo > 0 {
        let len = cmp::min(togo, round::floor((f / r.green.fuel) as f64, 0) as i32);
        stints.push(len);
        togo -= len;
        f = r.tank_size;
    }
    Strategy {
        stints: stints,
        stops: vec![],
    }
}

// a strategy that backtimes from the finish, i.e. the last stint is a full tank.
fn strat_rev(r: &StratRequest) -> Strategy {
    let mut stints = Vec::with_capacity(4);
    let mut togo = r.green_laps_togo;
    let full_stint_len = round::floor((r.tank_size / r.green.fuel) as f64, 0) as i32;
    let first_stint_max_len = round::floor((r.fuel_left / r.green.fuel) as f64, 0) as i32;
    while togo > 0 {
        let this_stint = cmp::min(togo, full_stint_len);
        stints.push(this_stint);
        togo -= this_stint;
    }
    if let Some(&l) = stints.last() {
        if l > first_stint_max_len {
            stints.pop();
            stints.push(l - first_stint_max_len);
            stints.push(first_stint_max_len);
            togo = 0;
        }
    }
    if togo > 0 {
        stints.push(togo);
    }
    stints.reverse();
    let mut stops = Vec::with_capacity(stints.len());
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
            green_laps_togo: 5,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![5], s.stints);
        let s = strat_rev(&r);
        assert_eq!(vec![5], s.stints);
    }

    #[test]
    fn strat_one_stop() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: 34,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![19, 15], s.stints);
        let s = strat_rev(&r);
        assert_eq!(vec![14, 20], s.stints);
    }

    #[test]
    fn strat_two_stops() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: 49,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![18, 20, 11], s.stints);
        let s = strat_rev(&r);
        assert_eq!(vec![9, 20, 20], s.stints);
    }
    #[test]
    fn strat_one_stop_big_window() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: 24,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![18, 6], s.stints);
        let s = strat_rev(&r);
        assert_eq!(vec![4, 20], s.stints);
    }
    #[test]
    fn strat_two_stops_with_splash() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 1.5,
            tank_size: 10.0,
            yellow_togo: 0,
            green_laps_togo: 29,
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = strat_fwd(&r);
        assert_eq!(vec![3, 20, 6], s.stints);
        let s = strat_rev(&r);
        assert_eq!(vec![3, 6, 20], s.stints);
    }
}
