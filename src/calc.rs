#![allow(dead_code)]

use super::strat::{EndsWith, Lap, LapState, Rate, StratRequest, Strategy};
use std::cmp;

#[derive(Clone, Copy, Debug)]
pub struct RaceConfig {
    fuel_tank_size: f32,
    ends: EndsWith,
    track_id: i32,
    layout_id: i32,
    car_id: i32,
}

pub struct Calculator {
    cfg: RaceConfig,
    laps: Vec<Lap>,
    race_lap: i32,
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
    // calculates a green lap fuel/time estimate from recently completed green laps.
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
    // calculates a yellow flag lap fuel/time estimate from prior yellow laps.
    fn recent_yellow(&self) -> Option<Rate> {
        // we want to ignore the lap of the set of yellow laps, as its a partial yellow lap
        // and not indicitive of a "normal" yellow lap.
        let mut yellow_start = false;
        let mut total = Rate::default();
        let mut count = 0;
        for lap in &self.laps {
            if lap.condition.intersects(LapState::YELLOW) {
                if !yellow_start {
                    yellow_start = true;
                } else {
                    total = total.add(&lap);
                    count += 1;
                }
            } else {
                yellow_start = false;
            }
        }
        if count == 0 {
            None
        } else {
            Some(Rate {
                fuel: total.fuel / (count as f32),
                time: total.time / count,
            })
        }
    }

    pub fn strat(&self) -> Vec<Strategy> {
        let green = self.recent_green();
        if green.is_none() {
            return vec![];
        }
        let yellow = self.recent_yellow().unwrap_or_else(|| Rate {
            fuel: green.unwrap().fuel / 4.0,
            time: green.unwrap().time * 4,
        });
        // currently the or to default to a full tank is never triggered because recent_green() will
        // return None if we haven't done any laps yet. [this will change in the future]
        let fuel_left = self
            .laps
            .last()
            .map_or(self.cfg.fuel_tank_size, |lap| lap.fuel_left);
        let laps_left = |laps| laps - self.laps.len();
        let time_left = |d| d - self.laps.iter().map(|lap| lap.time).sum();
        let ends = match self.cfg.ends {
            EndsWith::Laps(laps) => EndsWith::Laps(laps_left(laps)),
            EndsWith::Time(d) => EndsWith::Time(time_left(d)),
            EndsWith::LapsOrTime(laps, d) => EndsWith::LapsOrTime(laps_left(laps), time_left(d)),
        };
        let yellow_laps = self
            .laps
            .iter()
            .rev()
            .take_while(|lap| lap.condition.intersects(LapState::YELLOW))
            .count();
        let r = StratRequest {
            fuel_left: fuel_left,
            tank_size: self.cfg.fuel_tank_size,
            // a yellow flag is usually at least 3 laps.
            // TODO, can we detect the 2/1 togo state from iRacing?
            yellow_togo: if yellow_laps > 0 {
                cmp::max(0, 3 - yellow_laps)
            } else {
                0
            },
            ends: ends,
            green: green.unwrap(),
            yellow: yellow,
        };
        let fwd = r.compute();
        vec![fwd]
    }
}

#[cfg(test)]
mod tests {
    use super::super::strat::Pitstop;
    use super::*;
    use std::time::Duration;

    #[test]
    fn no_laps() {
        // Note in the future a previously calc/saved green rate would be loaded
        // and this would generate a starting strategy
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            ends: EndsWith::Laps(50),
            track_id: 1,
            layout_id: 1,
            car_id: 1,
        };
        let calc = Calculator::new(cfg);
        let strat = calc.strat();
        assert_eq!(0, strat.len());
    }

    #[test]
    fn one_lap() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            ends: EndsWith::Laps(50),
            track_id: 1,
            layout_id: 1,
            car_id: 1,
        };
        let mut calc = Calculator::new(cfg);
        calc.add_lap(
            Lap {
                fuel_left: 9.5,
                fuel_used: 0.5,
                time: Duration::new(30, 0),
                condition: LapState::empty(),
            },
            1,
        );
        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![19, 20, 10], strat[0].laps());
        assert_eq!(
            vec![Pitstop::new(9, 19), Pitstop::new(29, 39)],
            strat[0].stops
        );
    }

    #[test]
    fn five_laps() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            ends: EndsWith::Laps(50),
            track_id: 1,
            layout_id: 1,
            car_id: 1,
        };
        let mut calc = Calculator::new(cfg);
        let mut lap = Lap {
            fuel_left: 9.5,
            fuel_used: 0.5,
            time: Duration::new(30, 0),
            condition: LapState::empty(),
        };
        calc.add_lap(lap, 1);
        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![19, 20, 10], strat[0].laps());
        assert_eq!(
            vec![Pitstop::new(9, 19), Pitstop::new(29, 39)],
            strat[0].stops
        );
        lap.fuel_left -= 0.5;
        calc.add_lap(lap, 2);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap, 3);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap, 4);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap, 5);
        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![15, 20, 10], strat[0].laps());
        assert_eq!(
            vec![Pitstop::new(5, 15), Pitstop::new(25, 35)],
            strat[0].stops
        );
    }

    #[test]
    fn yellow() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            ends: EndsWith::Laps(50),
            track_id: 1,
            layout_id: 1,
            car_id: 1,
        };
        let mut calc = Calculator::new(cfg);
        let mut lap = Lap {
            fuel_left: 9.0,
            fuel_used: 1.0,
            time: Duration::new(30, 0),
            condition: LapState::empty(),
        };
        calc.add_lap(lap, 1);
        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![9, 10, 10, 10, 10], strat[0].laps());

        lap.fuel_left -= 1.0;
        calc.add_lap(lap, 2);
        lap.fuel_left -= 1.0;
        calc.add_lap(lap, 3);
        lap.fuel_left -= 1.0;
        calc.add_lap(lap, 4);
        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![6, 10, 10, 10, 10], strat[0].laps());

        lap.fuel_left -= 0.5;
        lap.condition = LapState::YELLOW;
        calc.add_lap(lap, 5);
        lap.fuel_left -= 0.1;
        lap.condition = LapState::YELLOW;
        calc.add_lap(lap, 6);

        let strat = calc.strat();
        assert_eq!(1, strat.len());
        assert_eq!(vec![5, 10, 10, 10, 9], strat[0].laps());
    }
}
