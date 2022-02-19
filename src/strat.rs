#![allow(dead_code)]

use bitflags::bitflags;
use math::round;
use std::cmp;
use std::fmt;
use std::iter;
use std::ops::Add;
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
    pub fuel_used: f32,
    pub fuel_left: f32,
    pub time: Duration,
    pub condition: LapState,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rate {
    pub fuel: f32,
    pub time: Duration,
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
    pub fn add(&self, l: &Lap) -> Rate {
        Rate {
            fuel: self.fuel + l.fuel_used,
            time: self.time + l.time,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pitstop {
    pub open: usize,
    pub close: usize,
}
impl Pitstop {
    pub fn new(open: usize, close: usize) -> Pitstop {
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

#[derive(Clone, Copy, Debug)]
pub struct Stint {
    pub laps: usize,
    pub fuel: f32,
    pub time: Duration,
}
impl Stint {
    fn new() -> Stint {
        Stint {
            laps: 0,
            fuel: 0.0,
            time: Duration::ZERO,
        }
    }
    fn add(&mut self, lap: &Rate) {
        self.laps += 1;
        self.fuel += lap.fuel;
        self.time += lap.time;
    }
    fn formatted_time(&self) -> String {
        if self.time < Duration::new(60, 0) {
            format!("{}s", self.time.as_secs())
        } else {
            let s = self.time.as_secs();
            format!("{}:{:2}", s / 60, s % 60)
        }
    }
}
impl fmt::Display for Stint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Stint {} laps, uses {} fuel, takes {}",
            self.laps,
            self.fuel,
            self.formatted_time()
        )
    }
}

#[derive(Clone, Debug)]
pub struct Strategy {
    pub stints: Vec<Stint>,
    pub stops: Vec<Pitstop>,
    pub fuel_to_save: f32, // ammount of fuel to save to reduce # of pitstops needed
    pub green: Rate,
    pub yellow: Rate,
}
impl Default for Strategy {
    fn default() -> Strategy {
        Strategy {
            stints: vec![],
            stops: vec![],
            fuel_to_save: 0.0,
            green: Rate::default(),
            yellow: Rate::default(),
        }
    }
}
impl Strategy {
    pub fn laps(&self) -> Vec<usize> {
        self.stints.iter().map(|s| s.laps).collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum EndsWith {
    Laps(i32),                 // race ends after this many more laps
    Time(Duration),            // race ends after this much more time
    LapsOrTime(i32, Duration), // first of the above 2 to happen
}

pub struct StratRequest {
    pub fuel_left: f32,
    pub tank_size: f32,
    pub max_fuel_save: f32,
    pub yellow_togo: usize,
    pub ends: EndsWith, // for a laps race, EndsWith laps is total laps to go, regardless of yellow/green.
    pub green: Rate,
    pub yellow: Rate,
}

impl StratRequest {
    // Compute fuel strategy. The base strategy repeatedly runs the tank dry until the end of the race.
    // Pit stop windows are extended based on the size of the last stint. If the last stint isn't a full
    // tank then you can stop earlier and still complete the last stint. This cascades back into all
    // the pit windows.
    pub fn compute(&self) -> Strategy {
        let stints = self.stints();
        if stints.is_empty() {
            Strategy::default()
        } else {
            Strategy {
                fuel_to_save: self.fuel_save(&stints),
                stops: self.stops(&stints),
                stints: stints,
                green: self.green,
                yellow: self.yellow,
            }
        }
    }

    fn stints(&self) -> Vec<Stint> {
        let yellow = iter::repeat(self.yellow).take(self.yellow_togo as usize);
        let mut tm = Duration::ZERO;
        let mut laps = 0;
        let laps = yellow.chain(iter::repeat(self.green)).take_while(|lap| {
            // for laps the race ends when Laps(l) are done
            // for timed races, the race ends on the lap after time runs out
            let res = match self.ends {
                EndsWith::Laps(l) => laps < l,
                EndsWith::Time(d) => tm <= d,
                EndsWith::LapsOrTime(l, d) => laps < l && tm <= d,
            };
            tm = tm.add(lap.time);
            laps += 1;
            res
        });
        // the laps iterator will return the sequence of predicted laps until the conclusion of the race

        let mut stints = Vec::with_capacity(4);
        let mut f = self.fuel_left;
        let mut stint = Stint::new();
        for lap in laps {
            if f < lap.fuel {
                stints.push(stint);
                stint = Stint::new();
                f = self.tank_size;
            }
            stint.add(&lap);
            f -= lap.fuel;
        }
        if stint.laps > 0 {
            stints.push(stint);
        }
        stints
    }

    fn stops(&self, stints: &Vec<Stint>) -> Vec<Pitstop> {
        let mut stops = Vec::with_capacity(stints.len());
        let full_stint_len = round::floor((self.tank_size / self.green.fuel) as f64, 0) as usize;
        let mut lap_open = 0;
        let mut lap_close = 0;
        let mut ext = full_stint_len - stints.last().unwrap().laps;
        for i in 0..stints.len() - 1 {
            // we can bring this stop forward by extending a later stop
            let wdw_size = cmp::min(ext, stints[i].laps);
            lap_open += stints[i].laps - wdw_size;
            lap_close += stints[i].laps;
            stops.push(Pitstop::new(lap_open, lap_close));
            ext -= wdw_size;
        }
        stops
    }

    fn fuel_save(&self, stints: &Vec<Stint>) -> f32 {
        let total: f32 = stints.iter().map(|s| s.fuel).sum();
        let max_save = total * self.max_fuel_save;
        let last_stint_fuel = stints.last().unwrap().fuel;
        if last_stint_fuel < max_save {
            last_stint_fuel
        } else {
            0.0
        }
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
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(5),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![5], s.laps());
        assert_eq!(Vec::<Pitstop>::new(), s.stops);
    }

    #[test]
    fn strat_timed_race() {
        let d = Duration::new(25, 0);
        let r = StratRequest {
            fuel_left: 20.0,
            tank_size: 20.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Time(Duration::new(105, 0)),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![5], s.laps());
    }

    #[test]
    fn strat_race_over() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 0.9,
            tank_size: 20.0,
            max_fuel_save: 0.1,
            yellow_togo: 0,
            ends: EndsWith::Laps(0),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert!(s.stints.is_empty());
        assert!(s.stops.is_empty());
        assert_eq!(0.0, s.fuel_to_save);
    }

    #[test]
    fn strat_one_stop_laps() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(34),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![19, 15], s.laps());
        assert_eq!(vec![Pitstop::new(14, 19)], s.stops);
    }

    #[test]
    fn strat_one_stop_time() {
        let d = Duration::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 2,
            ends: EndsWith::Time(Duration::new(300, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: Duration::new(55, 0),
            },
        };
        // after lap 1 t=55     f=4.9
        // after lap 2 t=110    f=4.8
        // after lap 3 t=140    f=3.8
        //           4 t=170    f=2.8
        //           5 t=200    f=1.8
        //           6 t=230    box f=10
        //           7 t=260    f=9
        //           8 t=290    f=8
        //           9 t=320    f=7
        let s = r.compute();
        assert_eq!(vec![6, 3], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_laps_or_time_ends_on_time() {
        let d = Duration::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 2,
            ends: EndsWith::LapsOrTime(100, Duration::new(300, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: Duration::new(55, 0),
            },
        };
        // after lap 1 t=55     f=4.9
        // after lap 2 t=110    f=4.8
        // after lap 3 t=140    f=3.8
        //           4 t=170    f=2.8
        //           5 t=200    f=1.8
        //           6 t=230    box f=10
        //           7 t=260    f=9
        //           8 t=290    f=8
        //           9 t=320    f=7
        let s = r.compute();
        assert_eq!(vec![6, 3], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_laps_or_time_ends_on_laps() {
        let d = Duration::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 2,
            ends: EndsWith::LapsOrTime(10, Duration::new(3000, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: Duration::new(60, 0),
            },
        };
        let s = r.compute();
        assert_eq!(vec![6, 4], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_yellow() {
        let d = Duration::new(25, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 3,
            ends: EndsWith::Laps(23),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: d.mul_f32(5.0),
            },
        };
        let s = r.compute();
        assert_eq!(vec![21, 2], s.laps());
        assert_eq!(vec![Pitstop::new(3, 21)], s.stops);
    }

    #[test]
    fn strat_two_stops() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(49),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![18, 20, 11], s.laps());
        assert_eq!(vec![Pitstop::new(9, 18), Pitstop::new(29, 38)], s.stops);
    }

    #[test]
    fn strat_one_stop_big_window() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(24),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![18, 6], s.laps());
        assert_eq!(vec![Pitstop::new(4, 18)], s.stops);
    }

    #[test]
    fn strat_two_stops_with_splash() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 1.5,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(29),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![3, 20, 6], s.laps());
        assert_eq!(vec![Pitstop::new(0, 3), Pitstop::new(9, 23)], s.stops);
    }

    #[test]
    fn strat_two_stops_only_just() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.6,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(58),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert_eq!(vec![19, 20, 19], s.laps());
        assert_eq!(vec![Pitstop::new(18, 19), Pitstop::new(38, 39)], s.stops);
    }

    #[test]
    fn strat_two_stops_fuel_save() {
        let d = Duration::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.0,
            tank_size: 20.0,
            max_fuel_save: 0.1, //10%
            yellow_togo: 0,
            ends: EndsWith::Laps(50),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: d * 4,
            },
        };
        let s = r.compute();
        assert_eq!(vec![9, 20, 20, 1], s.laps());
        assert_eq!(
            vec![
                Pitstop::new(0, 9),
                Pitstop::new(10, 29),
                Pitstop::new(30, 49)
            ],
            s.stops
        );
        // if you can save 1 liter, you can skip the last pit stop
        assert_eq!(1.0, s.fuel_to_save);
    }
}
