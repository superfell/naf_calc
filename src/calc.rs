#![allow(dead_code)]

use super::strat::{EndsWith, Lap, Rate, StratRequest, Strategy};

#[derive(Clone, Copy, Debug)]
pub struct RaceConfig {
    fuel_tank_size: f32,
    ends: EndsWith,
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
            ends: self.cfg.ends,
            green: green.unwrap(),
            yellow: yellow,
        };
        let fwd = r.compute();
        vec![fwd]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
