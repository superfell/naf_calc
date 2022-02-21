#![allow(dead_code)]

use r2d2::ManageConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, Error};

use super::strat::{EndsWith, Lap, LapState, Rate, StratRequest, Strategy};
use std::{cmp, path::PathBuf};

#[derive(Clone, Debug)]
pub struct RaceConfig {
    pub fuel_tank_size: f32,
    pub max_fuel_save: f32,
    pub track_id: i64,
    pub track_name: String,
    pub layout_name: String,
    pub car_id: i64,
    pub db_file: Option<PathBuf>,
}

pub struct Calculator {
    cfg: RaceConfig,
    laps: Vec<Lap>,
    db: Option<Db>,
}
struct Db {
    con_mgr: SqliteConnectionManager,
    con: Connection,
    laps_written: usize,
    id: Option<i64>,
}
impl Calculator {
    pub fn new(cfg: RaceConfig) -> Result<Calculator, Error> {
        let db = match &cfg.db_file {
            None => Ok(None),
            Some(f) => {
                let c = r2d2_sqlite::SqliteConnectionManager::file(f);
                let con = c.connect();
                con.map(|con| {
                    Some(Db {
                        con_mgr: c,
                        con: con,
                        laps_written: 0,
                        id: None,
                    })
                })
            }
        }?;
        let mut c = Calculator {
            cfg: cfg,
            laps: Vec::with_capacity(16),
            db: db,
        };
        c.init_schema()?;
        c.insert_session().expect("failed to insert session");
        Ok(c)
    }
    pub fn add_lap(&mut self, l: Lap) {
        self.laps.push(l);
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
        // we want to ignore the first lap of the set of yellow laps, as its a partial yellow lap
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

    pub fn strat(&self, ends: EndsWith) -> Option<Strategy> {
        let green = self.recent_green();
        if green.is_none() {
            return None;
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

        let yellow_laps = self
            .laps
            .iter()
            .rev()
            .take_while(|lap| lap.condition.intersects(LapState::YELLOW))
            .count() as isize;
        let r = StratRequest {
            fuel_left: fuel_left,
            tank_size: self.cfg.fuel_tank_size,
            max_fuel_save: self.cfg.max_fuel_save,
            // a yellow flag is usually at least 3 laps.
            // TODO, can we detect the 2/1 togo state from iRacing?
            yellow_togo: if yellow_laps > 0 {
                cmp::max(0, 3 - yellow_laps) as i32
            } else {
                0
            },
            ends: ends,
            green: green.unwrap(),
            yellow: yellow,
        };
        // match ends {
        //     EndsWith::Laps(l) => println!("{} laps togo", l),
        //     EndsWith::Time(d) => println!("{:?} time to go", d),
        //     EndsWith::LapsOrTime(l, d) => println!("{} laps or {:?} to go", l, d),
        // }
        r.compute()
    }

    fn init_schema(&self) -> Result<(), Error> {
        if let Some(db) = &self.db {
            let s = "CREATE TABLE IF NOT EXISTS Session(
                                id              integer  primary key,
                                time            text,
                                car_id          int,
                                track_id        int,
                                track_name      text,
                                track_layout    text,
                                tank_size       float,
                                max_fuel_save   float)";
            db.con.execute(s, [])?;
            let s = "CREATE TABLE IF NOT EXISTS Lap(
                                id              integer primary key,
                                session         integer references session(id),
                                time            text,
                                fuel_used       float,
                                fuel_left       float,
                                lap_time        float,
                                condition       int,
                                condition_str   text)";
            db.con.execute(s, [])?;
        }
        Ok(())
    }
    fn insert_session(&mut self) -> Result<(), Error> {
        if let Some(db) = &mut self.db {
            let mut stmt = db.con.prepare("INSERT INTO Session(time,car_id,track_id,track_name,track_layout,tank_size,max_fuel_save) 
                VALUES(datetime('now'),?,?,?,?,?,?)")?;
            let c = &self.cfg;
            let id = stmt.insert(params![
                c.car_id,
                c.track_id,
                c.track_name,
                c.layout_name,
                c.fuel_tank_size,
                c.max_fuel_save,
            ])?;
            db.id = Some(id);
        }
        Ok(())
    }
    pub fn save_laps(&mut self) -> Result<(), Error> {
        if let Some(db) = &mut self.db {
            let tx = db.con.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO Lap(session,time,fuel_used,fuel_left,lap_time,condition,condition_str)
                    VALUES (?,datetime('now'),?,?,?,?,?)",
                )?;
                for l in self.laps[db.laps_written..].iter() {
                    stmt.insert(params![
                        db.id.unwrap(),
                        l.fuel_used,
                        l.fuel_left,
                        l.time.as_secs_f64(),
                        l.condition.bits(),
                        format!("{:?}", l.condition),
                    ])?;
                }
            }
            tx.commit()?;
            db.laps_written = self.laps.len();
        }
        Ok(())
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
            max_fuel_save: 0.0,
            track_id: 1,
            track_name: "Test".to_string(),
            layout_name: "Oval".to_string(),
            car_id: 1,
            db_file: None,
        };
        let calc = Calculator::new(cfg).unwrap();
        let strat = calc.strat(EndsWith::Laps(50));
        assert!(strat.is_none());
    }

    #[test]
    fn one_lap() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            max_fuel_save: 0.0,
            track_id: 1,
            track_name: "Test".to_string(),
            layout_name: "Oval".to_string(),
            car_id: 1,
            db_file: None,
        };
        let mut calc = Calculator::new(cfg).unwrap();
        calc.add_lap(Lap {
            fuel_left: 9.5,
            fuel_used: 0.5,
            time: Duration::new(30, 0),
            condition: LapState::empty(),
        });
        let strat = calc.strat(EndsWith::Laps(49)).unwrap();
        assert_eq!(vec![19, 20, 10], strat.laps());
        assert_eq!(vec![Pitstop::new(9, 19), Pitstop::new(29, 39)], strat.stops);
    }

    #[test]
    fn five_laps() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            max_fuel_save: 0.0,
            track_id: 1,
            track_name: "Test".to_string(),
            layout_name: "Oval".to_string(),
            car_id: 1,
            db_file: None,
        };
        let mut calc = Calculator::new(cfg).unwrap();
        let mut lap = Lap {
            fuel_left: 9.5,
            fuel_used: 0.5,
            time: Duration::new(30, 0),
            condition: LapState::empty(),
        };
        calc.add_lap(lap);
        let strat = calc.strat(EndsWith::Laps(49)).unwrap();
        assert_eq!(vec![19, 20, 10], strat.laps());
        assert_eq!(vec![Pitstop::new(9, 19), Pitstop::new(29, 39)], strat.stops);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap);
        lap.fuel_left -= 0.5;
        calc.add_lap(lap);
        let strat = calc.strat(EndsWith::Laps(45)).unwrap();
        assert_eq!(vec![15, 20, 10], strat.laps());
        assert_eq!(vec![Pitstop::new(5, 15), Pitstop::new(25, 35)], strat.stops);
    }

    #[test]
    fn yellow() {
        let cfg = RaceConfig {
            fuel_tank_size: 10.0,
            max_fuel_save: 0.0,
            track_id: 1,
            track_name: "Test".to_string(),
            layout_name: "Oval".to_string(),
            car_id: 1,
            db_file: None,
        };
        let mut calc = Calculator::new(cfg).unwrap();
        let mut lap = Lap {
            fuel_left: 9.0,
            fuel_used: 1.0,
            time: Duration::new(30, 0),
            condition: LapState::empty(),
        };
        calc.add_lap(lap);
        let strat = calc.strat(EndsWith::Laps(49)).unwrap();
        assert_eq!(vec![9, 10, 10, 10, 10], strat.laps());

        lap.fuel_left -= 1.0;
        calc.add_lap(lap);
        lap.fuel_left -= 1.0;
        calc.add_lap(lap);
        lap.fuel_left -= 1.0;
        calc.add_lap(lap);
        let strat = calc.strat(EndsWith::Laps(46)).unwrap();
        assert_eq!(vec![6, 10, 10, 10, 10], strat.laps());

        lap.fuel_left -= 0.5;
        lap.condition = LapState::YELLOW;
        calc.add_lap(lap);
        lap.fuel_left -= 0.1;
        lap.condition = LapState::YELLOW;
        calc.add_lap(lap);

        let strat = calc.strat(EndsWith::Laps(44)).unwrap();
        assert_eq!(vec![5, 10, 10, 10, 9], strat.laps());
    }
}
