#![allow(dead_code)]

use std::time::Duration;

use calc::{Calculator, RaceConfig};
use std::fmt;
use strat::{EndsWith, Lap, LapState};

mod calc;
mod ir;
mod strat;

extern crate yaml_rust;

#[derive(Clone, Copy, Debug)]
struct CarState {
    session_num: i32,
    session_time: f64,
    is_on_track: bool,
    player_track_surface: ir::TrackLocation,
    session_state: ir::SessionState,
    session_flags: ir::Flags,
    session_time_remain: f64,
    session_laps_remain: i32,
    session_time_total: f64,
    session_laps_total: i32,
    lap: i32,
    lap_completed: i32,
    race_laps: i32,
    fuel_level: f32,
    lap_progress: f32,
}
impl CarState {
    fn time_remaining(&self) -> Duration {
        Duration::from_millis((self.session_time_remain * 1000.0) as u64)
    }
    fn ends(&self) -> EndsWith {
        // TODO deal with practice better
        if self.session_time_remain == ir::IRSDK_UNLIMITED_TIME {
            if self.session_laps_remain == ir::IRSDK_UNLIMITED_LAPS {
                EndsWith::Time(Duration::from_millis(
                    (30.0 * 60.0 - self.session_time) as u64 * 1000,
                ))
            } else {
                EndsWith::Laps(self.session_laps_remain)
            }
        } else if self.session_laps_remain == ir::IRSDK_UNLIMITED_LAPS {
            EndsWith::Time(self.time_remaining())
        } else {
            EndsWith::LapsOrTime(self.session_laps_remain, self.time_remaining())
        }
    }
    fn lap_state(&self) -> LapState {
        let mut s = LapState::empty();
        let f = self.session_flags;
        if f.intersects(
            ir::Flags::YELLOW
                | ir::Flags::YELLOW_WAVING
                | ir::Flags::CAUTION_WAVING
                | ir::Flags::CAUTION,
        ) {
            s = s | LapState::YELLOW
        }
        if self.player_track_surface == ir::TrackLocation::ApproachingPits
            || self.player_track_surface == ir::TrackLocation::InPitStall
        {
            s = s | LapState::PITTED
        }
        if self.session_state == ir::SessionState::ParadeLaps
            || self.session_state == ir::SessionState::Warmup
        {
            s = s | LapState::PACE_LAP
        }
        if f.intersects(ir::Flags::ONE_TO_GREEN) && s.intersects(LapState::YELLOW) {
            s = s | LapState::ONE_TO_GREEN
        }
        s
    }
}
impl fmt::Display for CarState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.3} {:?} {} {:?} {:.3} {} {:.3} {:.3} {:?}",
            self.session_time,
            self.session_state,
            self.is_on_track,
            self.player_track_surface,
            self.session_time_remain,
            self.session_laps_remain,
            //            self.session_time_total,
            //          self.session_laps_total,
            //        self.lap,
            //      self.lap_completed,
            //    self.race_laps,
            self.fuel_level,
            self.lap_progress,
            self.session_flags,
        )
    }
}

struct CarStateFactory {
    session_num: ir::Var,
    session_time: ir::Var,
    is_on_track: ir::Var,
    player_track_surface: ir::Var,
    session_state: ir::Var,
    session_flags: ir::Var,
    session_time_remain: ir::Var,
    session_laps_remain: ir::Var,
    session_time_total: ir::Var,
    session_laps_total: ir::Var,
    lap: ir::Var,
    lap_completed: ir::Var,
    race_laps: ir::Var,
    fuel_level: ir::Var,
    lap_progress: ir::Var,
}
impl CarStateFactory {
    fn new(c: &ir::Client) -> CarStateFactory {
        CarStateFactory {
            session_num: c.find_var("SessionNum").unwrap(),
            session_time: c.find_var("SessionTime").unwrap(),
            is_on_track: c.find_var("IsOnTrack").unwrap(),
            player_track_surface: c.find_var("PlayerTrackSurface").unwrap(),
            session_state: c.find_var("SessionState").unwrap(),
            session_flags: c.find_var("SessionFlags").unwrap(),
            session_time_remain: c.find_var("SessionTimeRemain").unwrap(),
            session_laps_remain: c.find_var("SessionLapsRemainEx").unwrap(),
            session_time_total: c.find_var("SessionTimeTotal").unwrap(),
            session_laps_total: c.find_var("SessionLapsTotal").unwrap(),
            lap: c.find_var("Lap").unwrap(),
            lap_completed: c.find_var("LapCompleted").unwrap(),
            race_laps: c.find_var("RaceLaps").unwrap(),
            fuel_level: c.find_var("FuelLevel").unwrap(),
            lap_progress: c.find_var("LapDistPct").unwrap(),
        }
    }
    fn read(&self, c: &ir::Client) -> CarState {
        CarState {
            session_num: c.value(&self.session_num).unwrap(),
            session_time: c.value(&self.session_time).unwrap(),
            is_on_track: c.value(&self.is_on_track).unwrap(),
            player_track_surface: c.value(&self.player_track_surface).unwrap(),
            session_state: c.value(&self.session_state).unwrap(),
            session_flags: c.value(&self.session_flags).unwrap(),
            session_time_remain: c.value(&self.session_time_remain).unwrap(),
            session_laps_remain: c.value(&self.session_laps_remain).unwrap(),
            session_time_total: c.value(&self.session_time_total).unwrap(),
            session_laps_total: c.value(&self.session_laps_total).unwrap(),
            lap: c.value(&self.lap).unwrap(),
            lap_completed: c.value(&self.lap_completed).unwrap(),
            race_laps: c.value(&self.race_laps).unwrap(),
            fuel_level: c.value(&self.fuel_level).unwrap(),
            lap_progress: c.value(&self.lap_progress).unwrap(),
        }
    }
}

fn main() {
    iracing_main();
}

fn iracing_main() {
    let mut c = ir::Client::new();
    'main: loop {
        loop {
            // wait for iRacing
            let ready = c.wait_for_data(std::time::Duration::new(1, 0));
            if ready && c.connected() {
                break;
            }
        }
        println!("connected to iracing!");
        let session_info = match c.session_info() {
            Ok(s) => SessionInfo::parse(&s, 0),
            Err(e) => panic!("failed to decode session string {}", e),
        };
        println!("session_info {:?}", session_info);
        let cfg = RaceConfig {
            fuel_tank_size: (session_info.driver_car_fuel_max_ltr
                * session_info.driver_car_max_fuel_pct) as f32,
            max_fuel_save: 0.1,
            track_id: session_info.track_id,
            track_name: session_info.track_display_name,
            layout_name: session_info.track_config_name,
            car_id: session_info.car_id,
            db_file: dirs_next::document_dir().map(|dir| dir.join("naf_calc\\laps.db")),
        };
        println!("calculator config: {:?}", cfg);
        let mut calc = Calculator::new(cfg).unwrap();

        'session: loop {
            println!(
                "new session: session_info_update {}",
                c.session_info_update().unwrap()
            );
            let f = CarStateFactory::new(&c);
            let mut last = f.read(&c);
            let session_info = match c.session_info() {
                Ok(s) => SessionInfo::parse(&s, last.session_num),
                Err(e) => panic!("failed to decode session string {}", e),
            };
            println!("session info {:?}", session_info);
            let mut lap_start = last;
            let calc_interval = std::time::Duration::new(0, 16 * 1000 * 1000);
            loop {
                if c.wait_for_data(calc_interval) {
                    let this = f.read(&c);
                    if this.session_time < last.session_time {
                        // reset
                        calc.save_laps().unwrap(); //TODO
                        continue 'session;
                    }
                    if (!lap_start.is_on_track) && this.is_on_track {
                        lap_start = this;
                    }
                    if last.player_track_surface == ir::TrackLocation::InPitStall
                        && this.player_track_surface != last.player_track_surface
                    {
                        // reset lap start when we leave the pit box
                        lap_start = this;
                    }
                    if this.session_state == ir::SessionState::ParadeLaps
                        && last.session_state != this.session_state
                    {
                        // reset lap start when the parade lap starts.
                        lap_start = this;
                        // show the stratagy if there's one available
                        let s = calc.strat(this.ends());
                        if let Some(x) = s {
                            println!("Starting strat {:?} {:?}", x.laps(), x.stops);
                        }
                    }
                    if this.lap_progress < 0.1 && last.lap_progress > 0.9 {
                        let new_lap = Lap {
                            fuel_left: this.fuel_level,
                            fuel_used: lap_start.fuel_level - this.fuel_level,
                            // TODO, this is not interopolating the lap
                            time: Duration::from_millis(
                                ((this.session_time - lap_start.session_time) * 1000.0) as u64,
                            ),
                            condition: this.lap_state() | lap_start.lap_state(),
                        };
                        println!("\n\t{}\n\t{}", lap_start, this);
                        println!("lap {:?}", new_lap);
                        if this.session_state != ir::SessionState::Checkered
                            && this.session_state != ir::SessionState::CoolDown
                        {
                            calc.add_lap(new_lap);
                            let ends = this.ends();
                            println!("ends: {:?}", ends);
                            match calc.strat(ends) {
                                Some(strat) => {
                                    println!(
                                        "stints: {:?} stops:{:?} save:{}",
                                        strat.laps(),
                                        strat.stops,
                                        strat.fuel_to_save,
                                    );
                                }
                                None => {}
                            }
                        }
                        lap_start = this;
                    }
                    if this.session_flags != last.session_flags
                        || this.session_state != last.session_state
                        || this.player_track_surface != last.player_track_surface
                    {
                        println!("{}", this);
                    }
                    last = this;
                } else if !c.connected() {
                    println!("no longer connected to iracing");
                    calc.save_laps().unwrap();
                    continue 'main;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct SessionInfo {
    // WeekendInfo:
    track_id: i64,                    // 419
    track_display_name: String,       // Phoenix Raceway
    track_display_short_name: String, // Phoenix
    track_config_name: String,        // Oval w/open dogleg
    event_type: String,               // Race
    category: String,                 // Oval
    // DriverInfo:
    driver_car_fuel_max_ltr: f64, // 40.000
    driver_car_max_fuel_pct: f64, // 0.050
    driver_car_est_lap_time: f64, // 24.1922
    // Drivers
    car_id: i64,      // 120
    car_name: String, // Indy Pro 2000 PM-18
    // SessionInfo
    session_name: String, // QUALIFY
}

impl SessionInfo {
    fn parse(session_info: &str, session_num: i32) -> SessionInfo {
        let yamls = yaml_rust::YamlLoader::load_from_str(session_info).unwrap();
        println!("session_info\n{}", session_info);
        let si = &yamls[0];
        let di = &si["DriverInfo"];
        let wi = &si["WeekendInfo"];
        let driver = &di["Drivers"][di["DriverCarIdx"].as_i64().unwrap() as usize];
        let sessions = &si["SessionInfo"]["Sessions"];
        SessionInfo {
            track_id: wi["TrackID"].as_i64().unwrap(),
            track_display_name: wi["TrackDisplayName"].as_str().unwrap().to_string(),
            track_display_short_name: wi["TrackDisplayShortName"].as_str().unwrap().to_string(),
            track_config_name: wi["TrackConfigName"].as_str().unwrap().to_string(),
            event_type: wi["EventType"].as_str().unwrap().to_string(),
            category: wi["Category"].as_str().unwrap().to_string(),
            driver_car_fuel_max_ltr: di["DriverCarFuelMaxLtr"].as_f64().unwrap(),
            driver_car_max_fuel_pct: di["DriverCarMaxFuelPct"].as_f64().unwrap(),
            driver_car_est_lap_time: di["DriverCarEstLapTime"].as_f64().unwrap(),
            car_id: driver["CarID"].as_i64().unwrap(),
            car_name: driver["CarScreenName"].as_str().unwrap().to_string(),
            session_name: sessions[session_num as usize]["SessionName"]
                .as_str()
                .unwrap()
                .to_string(),
        }
    }
}
