#![allow(dead_code)]

use bitflags::bitflags;
use druid::{Data, Lens};
use math::round;
use regex::Regex;
use std::cmp;
use std::fmt;
use std::iter;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};
use std::str::FromStr;
use std::time::Duration;

bitflags! {
    pub struct LapState:i32 {
        const YELLOW =      0x01;
        const PITTED =      0x02;
        const PACE_LAP =    0x04;
        const ONE_TO_GREEN = 0x08;
        const TWO_TO_GREEN = 0x10;
    }
}

/// This is a stupid wrapper around Duration so that we get the formatting of values we want
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct TimeSpan {
    d: Duration,
}
impl TimeSpan {
    pub const ZERO: TimeSpan = TimeSpan { d: Duration::ZERO };
    pub const ONE_MIN: TimeSpan = TimeSpan {
        d: Duration::new(60, 0),
    };
    pub const ONE_HR: TimeSpan = TimeSpan {
        d: Duration::new(60 * 60, 0),
    };

    pub fn of(d: Duration) -> TimeSpan {
        TimeSpan { d }
    }
    pub fn new(secs: u64, nanos: u32) -> TimeSpan {
        TimeSpan {
            d: Duration::new(secs, nanos),
        }
    }
    pub fn from_secs_f64(d: f64) -> TimeSpan {
        TimeSpan {
            d: Duration::from_secs_f64(d),
        }
    }
    pub fn from_secs_f32(d: f32) -> TimeSpan {
        TimeSpan {
            d: Duration::from_secs_f32(d),
        }
    }
    pub fn as_secs(&self) -> u64 {
        self.d.as_secs()
    }
    pub fn as_secs_f64(&self) -> f64 {
        self.d.as_secs_f64()
    }
    pub fn as_secs_f32(&self) -> f32 {
        self.d.as_secs_f32()
    }
    pub fn min(&self, rhs: Self) -> Self {
        TimeSpan {
            d: self.d.min(rhs.d),
        }
    }
}
impl Add for TimeSpan {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        TimeSpan { d: self.d + rhs.d }
    }
}
impl AddAssign for TimeSpan {
    fn add_assign(&mut self, rhs: Self) {
        self.d += rhs.d;
    }
}
impl Sum for TimeSpan {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut t = Self::ZERO;
        for rhs in iter {
            t.d += rhs.d;
        }
        t
    }
}
impl Sub for TimeSpan {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        TimeSpan { d: self.d - rhs.d }
    }
}
impl SubAssign for TimeSpan {
    fn sub_assign(&mut self, rhs: Self) {
        self.d = self.d - rhs.d
    }
}
impl Div<u32> for TimeSpan {
    type Output = Self;
    fn div(self, rhs: u32) -> Self {
        TimeSpan { d: self.d / rhs }
    }
}
impl Mul<u32> for TimeSpan {
    type Output = Self;
    fn mul(self, rhs: u32) -> TimeSpan {
        TimeSpan { d: self.d * rhs }
    }
}
impl From<TimeSpan> for Duration {
    fn from(a: TimeSpan) -> Self {
        a.d
    }
}
impl From<&TimeSpan> for Duration {
    fn from(a: &TimeSpan) -> Self {
        a.d
    }
}

use lazy_static::lazy_static;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParseError {
    Empty,
    Bogus,
}

impl FromStr for TimeSpan {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref DURATION_REGEX: Regex =
                Regex::new(r"^\s*(?:(\d{1,2}):)??(\d{2}):(\d{2})\s*$").unwrap();
        }
        match DURATION_REGEX.captures(s) {
            None => Err(ParseError::Empty),
            Some(cap) => {
                let secs = cap.get(3).map_or(0, |m| u64::from_str(m.as_str()).unwrap());
                let mins = cap.get(2).map_or(0, |m| u64::from_str(m.as_str()).unwrap()) * 60;
                let hours = cap.get(1).map_or(0, |m| u64::from_str(m.as_str()).unwrap()) * 60 * 60;
                Ok(TimeSpan::new(secs + mins + hours, 0))
            }
        }
    }
}
impl Data for TimeSpan {
    fn same(&self, other: &Self) -> bool {
        self.d == other.d
    }
}

impl fmt::Display for TimeSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self >= &Self::ONE_HR {
            write!(
                f,
                "{:}:{:02}:{:02}",
                self.d.as_secs() / 3600,
                (self.d.as_secs() % 3600) / 60,
                self.d.as_secs() % 60
            )
        } else {
            write!(
                f,
                "{:02}:{:02}",
                self.d.as_secs() / 60,
                self.d.as_secs() % 60
            )
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Lap {
    pub fuel_used: f32,
    pub fuel_left: f32,
    pub time: TimeSpan,
    pub condition: LapState,
}

#[derive(Clone, Copy, Debug, PartialEq, Data, Lens)]
pub struct Rate {
    pub fuel: f32,
    pub time: TimeSpan,
}
impl Default for Rate {
    fn default() -> Self {
        Rate {
            fuel: 0.0,
            time: TimeSpan::ZERO,
        }
    }
}
impl Add<&Lap> for Rate {
    type Output = Self;
    fn add(self, rhs: &Lap) -> Self {
        Rate {
            fuel: self.fuel + rhs.fuel_used,
            time: self.time + rhs.time,
        }
    }
}
impl AddAssign<&Lap> for Rate {
    fn add_assign(&mut self, rhs: &Lap) {
        self.fuel += rhs.fuel_used;
        self.time += rhs.time;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Data)]
pub struct Pitstop {
    pub open: i32,
    pub close: i32,
}
impl Pitstop {
    pub fn new(open: i32, close: i32) -> Pitstop {
        Pitstop { open, close }
    }
    pub fn is_open(&self) -> bool {
        self.open <= 0
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stint {
    pub laps: i32,
    pub fuel: f32,
    pub time: TimeSpan,
}
impl Stint {
    fn new() -> Stint {
        Stint {
            laps: 0,
            fuel: 0.0,
            time: TimeSpan::ZERO,
        }
    }
    fn add(&mut self, lap: &Rate) {
        self.laps += 1;
        self.fuel += lap.fuel;
        self.time += lap.time;
    }
}
impl fmt::Display for Stint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Stint {} laps, uses {} fuel, takes {}",
            self.laps, self.fuel, self.time
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    pub fn laps(&self) -> Vec<i32> {
        self.stints.iter().map(|s| s.laps).collect()
    }
    pub fn total_laps(&self) -> i32 {
        self.stints.iter().map(|s| s.laps).sum()
    }
    pub fn total_fuel(&self) -> f32 {
        self.stints.iter().map(|s| s.fuel).sum()
    }
    pub fn total_time(&self) -> TimeSpan {
        self.stints.iter().map(|s| s.time).sum()
    }
    pub fn fuel_target(&self) -> f32 {
        if self.fuel_to_save > 0.0 {
            let laps_til_last_stop: i32 = self.stints.iter().rev().skip(1).map(|s| s.laps).sum();
            if laps_til_last_stop > 0 {
                let fuel_to_last_stop: f32 = self.stints.iter().rev().skip(1).map(|s| s.fuel).sum();
                return (fuel_to_last_stop - self.fuel_to_save) / (laps_til_last_stop as f32);
            }
        }
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EndsWith {
    Laps(i32),                 // race ends after this many more laps
    Time(TimeSpan),            // race ends after this much more time
    LapsOrTime(i32, TimeSpan), // first of the above 2 to happen
}

#[derive(Clone, Debug, PartialEq)]
pub struct StratRequest {
    pub fuel_left: f32,
    pub tank_size: f32,
    pub max_fuel_save: f32,
    pub min_fuel: f32,
    pub yellow_togo: i32,
    pub ends: EndsWith, // for a laps race, EndsWith laps is total laps to go, regardless of yellow/green.
    pub green: Rate,
    pub yellow: Rate,
}

impl StratRequest {
    // Compute fuel strategy. The base strategy repeatedly runs the tank dry until the end of the race.
    // Pit stop windows are extended based on the size of the last stint. If the last stint isn't a full
    // tank then you can stop earlier and still complete the last stint. This cascades back into all
    // the pit windows.
    pub fn compute(&self) -> Option<Strategy> {
        let stints = self.stints();
        if stints.is_empty() {
            None
        } else {
            Some(Strategy {
                fuel_to_save: self.fuel_save(&stints),
                stops: self.stops(&stints),
                stints,
                green: self.green,
                yellow: self.yellow,
            })
        }
    }

    fn stints(&self) -> Vec<Stint> {
        let yellow = iter::repeat(self.yellow).take(self.yellow_togo as usize);
        let mut tm = TimeSpan::ZERO;
        let mut laps = 0;
        let laps = yellow.chain(iter::repeat(self.green)).take_while(|lap| {
            // for laps the race ends when Laps(l) are done
            // for timed races, the race ends on the lap after time runs out
            let continu = match self.ends {
                EndsWith::Laps(l) => laps < l,
                EndsWith::Time(d) => tm <= d,
                EndsWith::LapsOrTime(l, d) => laps < l && tm <= d,
            };
            tm += lap.time;
            laps += 1;
            continu
        });
        // the laps iterator will return the sequence of predicted laps until the conclusion of the race

        let mut stints = Vec::with_capacity(4);
        let mut f = self.fuel_left;
        let mut stint = Stint::new();
        for lap in laps {
            if f < lap.fuel + self.min_fuel {
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

    fn stops(&self, stints: &[Stint]) -> Vec<Pitstop> {
        let mut stops = Vec::with_capacity(stints.len());
        let full_stint_len = round::floor((self.tank_size / self.green.fuel) as f64, 0) as i32;
        let mut lap_open = 0;
        let mut lap_close = 0;
        let mut ext = full_stint_len - stints.last().unwrap().laps;
        for stint in stints.iter().take(stints.len() - 1) {
            // we can bring this stop forward by extending a later stop
            let wdw_size = cmp::min(ext, stint.laps);
            lap_open += stint.laps - wdw_size;
            lap_close += stint.laps;
            stops.push(Pitstop::new(lap_open, lap_close));
            ext -= wdw_size;
        }
        stops
    }

    fn fuel_save(&self, stints: &[Stint]) -> f32 {
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
            time: TimeSpan::new(3, 0),
        };
        let l = Lap {
            fuel_used: 0.3,
            fuel_left: 4.1,
            time: TimeSpan::new(5, 0),
            condition: LapState::empty(),
        };
        let r = s.add(&l);
        assert_eq!(
            r,
            Rate {
                fuel: 0.8,
                time: TimeSpan::new(8, 0)
            }
        );
    }

    #[test]
    fn strat_no_stops() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 20.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(5),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![5], s.laps());
        assert_eq!(Vec::<Pitstop>::new(), s.stops);
    }

    #[test]
    fn strat_timed_race() {
        let d = TimeSpan::new(25, 0);
        let r = StratRequest {
            fuel_left: 20.0,
            tank_size: 20.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Time(TimeSpan::new(105, 0)),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![5], s.laps());
    }

    #[test]
    fn strat_race_over() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 0.9,
            tank_size: 20.0,
            max_fuel_save: 0.1,
            min_fuel: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(0),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute();
        assert!(s.is_none());
    }

    #[test]
    fn strat_one_stop_laps() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(34),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![19, 15], s.laps());
        assert_eq!(vec![Pitstop::new(14, 19)], s.stops);
    }

    #[test]
    fn strat_one_stop_time() {
        let d = TimeSpan::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 2,
            ends: EndsWith::Time(TimeSpan::new(300, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: TimeSpan::new(55, 0),
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
        let s = r.compute().unwrap();
        assert_eq!(vec![6, 3], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_laps_or_time_ends_on_time() {
        let d = TimeSpan::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 2,
            ends: EndsWith::LapsOrTime(100, TimeSpan::new(300, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: TimeSpan::new(55, 0),
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
        let s = r.compute().unwrap();
        assert_eq!(vec![6, 3], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_laps_or_time_ends_on_laps() {
        let d = TimeSpan::new(30, 0);
        let r = StratRequest {
            fuel_left: 5.0,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 2,
            ends: EndsWith::LapsOrTime(10, TimeSpan::new(3000, 0)),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: TimeSpan::new(60, 0),
            },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![6, 4], s.laps());
        assert_eq!(vec![Pitstop::new(0, 6)], s.stops);
    }

    #[test]
    fn strat_one_stop_yellow() {
        let d = TimeSpan::new(25, 0);
        let r = StratRequest {
            fuel_left: 9.5,
            tank_size: 10.0,
            max_fuel_save: 0.0,
            min_fuel: 0.0,
            yellow_togo: 3,
            ends: EndsWith::Laps(23),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: d * 5,
            },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![21, 2], s.laps());
        assert_eq!(vec![Pitstop::new(3, 21)], s.stops);
    }

    #[test]
    fn strat_two_stops() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            min_fuel: 0.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(49),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![18, 20, 11], s.laps());
        assert_eq!(vec![Pitstop::new(9, 18), Pitstop::new(29, 38)], s.stops);
    }

    #[test]
    fn strat_one_stop_big_window() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.3,
            tank_size: 10.0,
            min_fuel: 0.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(24),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![18, 6], s.laps());
        assert_eq!(vec![Pitstop::new(4, 18)], s.stops);
    }

    #[test]
    fn strat_two_stops_with_splash() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 1.5,
            tank_size: 10.0,
            min_fuel: 0.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(29),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![3, 20, 6], s.laps());
        assert_eq!(vec![Pitstop::new(0, 3), Pitstop::new(9, 23)], s.stops);
    }

    #[test]
    fn strat_two_stops_only_just() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.6,
            tank_size: 10.0,
            min_fuel: 0.0,
            max_fuel_save: 0.0,
            yellow_togo: 0,
            ends: EndsWith::Laps(58),
            green: Rate { fuel: 0.5, time: d },
            yellow: Rate { fuel: 0.1, time: d },
        };
        let s = r.compute().unwrap();
        assert_eq!(vec![19, 20, 19], s.laps());
        assert_eq!(vec![Pitstop::new(18, 19), Pitstop::new(38, 39)], s.stops);
    }

    #[test]
    fn strat_two_stops_fuel_save() {
        let d = TimeSpan::new(40, 0);
        let r = StratRequest {
            fuel_left: 9.0,
            tank_size: 20.0,
            min_fuel: 0.0,
            max_fuel_save: 0.1, //10%
            yellow_togo: 0,
            ends: EndsWith::Laps(50),
            green: Rate { fuel: 1.0, time: d },
            yellow: Rate {
                fuel: 0.1,
                time: d * 4,
            },
        };
        let s = r.compute().unwrap();
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
        // 49l to last stint, minus the 1.0 we need to save
        // div by the 49 laps to get to the last stint gives us
        // our per lap fuel target
        assert_eq!((9.0 + 20.0 + 20.0 - 1.0) / 49.0, s.fuel_target());
    }

    #[test]
    fn test_timespan_parse() {
        assert_eq!(TimeSpan::from_str("00:10").unwrap().d.as_secs(), 10);
        assert_eq!(TimeSpan::from_str("05:10").unwrap().d.as_secs(), 310);
        assert_eq!(TimeSpan::from_str("01:05:10").unwrap().d.as_secs(), 3910);
        assert_eq!(TimeSpan::from_str(" 05:10 ").unwrap().d.as_secs(), 310);
        assert_eq!(
            TimeSpan::from_str("    01:05:10 ").unwrap().d.as_secs(),
            3910
        );
        assert!(TimeSpan::from_str("").is_err());
        assert!(TimeSpan::from_str("bob").is_err());
    }

    #[test]
    fn test_timespan_display() {
        assert_eq!(format!("{}", TimeSpan::of(Duration::ZERO)), "00:00");
        assert_eq!(format!("{}", TimeSpan::new(5, 0)), "00:05");
        assert_eq!(format!("{}", TimeSpan::new(35, 0)), "00:35");
        assert_eq!(format!("{}", TimeSpan::new(59, 0)), "00:59");
        assert_eq!(format!("{}", TimeSpan::new(60, 0)), "01:00");
        assert_eq!(format!("{}", TimeSpan::new(65, 0)), "01:05");
        assert_eq!(format!("{}", TimeSpan::new(60 * 59, 0)), "59:00");
        assert_eq!(format!("{}", TimeSpan::new(3600, 0)), "1:00:00");
        assert_eq!(format!("{}", TimeSpan::new(3600 * 5 + 5, 0)), "5:00:05");
    }
}
