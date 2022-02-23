#![allow(dead_code)]

use std::time::Duration;

use crate::strat::Strategy;

use super::calc::{Calculator, RaceConfig};
use super::ir;
use super::strat::{EndsWith, Lap, LapState, Pitstop};
use druid::{Data, Lens};
use std::fmt;

#[derive(Clone)]
pub struct ADuration {
    d: Duration,
}
impl Data for ADuration {
    fn same(&self, other: &Self) -> bool {
        self.d == other.d
    }
}
impl fmt::Display for ADuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}",
            self.d.as_secs() / 60,
            self.d.as_secs() % 60
        )
    }
}

#[derive(Clone, Data, Lens)]
pub struct AmountLeft {
    pub fuel: f32,
    pub laps: i32,
    pub time: ADuration,
}
impl Default for AmountLeft {
    fn default() -> Self {
        AmountLeft {
            fuel: 0.0,
            laps: 0,
            time: ADuration { d: Duration::ZERO },
        }
    }
}
#[derive(Clone, Data, Lens)]
pub struct State {
    pub connected: bool,            // connected to iracing
    pub car: AmountLeft,            // what's left in the car
    pub race: AmountLeft,           // what's left to go in the race
    pub fuel_last_lap: f32,         // fuel used on the last lap
    pub fuel_avg: f32,              // average per lap usage (green flag only)
    pub stops: i32,                 // pitstops needed to finish race
    pub next_stop: Option<Pitstop>, // details on the next pitstop
    pub save: f32,                  // save this much fuel to skip the last pitstop
}
impl Default for State {
    fn default() -> Self {
        State {
            connected: false,
            car: AmountLeft::default(),
            race: AmountLeft::default(),
            fuel_last_lap: 0.0,
            fuel_avg: 0.0,
            stops: 0,
            next_stop: None,
            save: 0.0,
        }
    }
}

pub struct IrCalc<'a> {
    client: ir::Client<'a>,
    state: Option<CalcState<'a>>,
}

// state needed by a running calculator
struct CalcState<'a> {
    calc: Calculator,
    ir: ir::Session<'a>,
    f: CarStateFactory,
    last: CarState,
    lap_start: CarState,
}
impl<'a> CalcState<'a> {
    fn new(session: ir::Session<'a>) -> Result<CalcState<'a>, ir::Error> {
        let mut session_info;
        unsafe {
            session_info = match session.session_info() {
                Ok(s) => SessionInfo::parse(&s, 0),
                Err(e) => return Err(ir::Error::SessionExpired),
            };
        }
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
        let calc = Calculator::new(cfg).unwrap();
        let mut f = CarStateFactory::new(&session)?;
        let last = f.read(&session)?;
        Ok(CalcState {
            calc,
            ir: session,
            f,
            last,
            lap_start: last,
        })
    }
    fn read(&self) -> Result<CarState, ir::Error> {
        self.f.read(&self.ir)
    }
    fn update(&mut self, result: &mut State) -> Result<(), ir::Error> {
        let this = self.read()?;
        if this.session_time < self.last.session_time {
            // reset
            self.calc.save_laps().unwrap(); //TODO
            self.last = this;
            self.lap_start = this;
        }
        if (!self.lap_start.is_on_track) && this.is_on_track {
            // ensure lap_start is from when we're in the car.
            self.lap_start = this;
        }
        if self.last.player_track_surface == ir::TrackLocation::InPitStall
            && this.player_track_surface != self.last.player_track_surface
        {
            // reset lap start when we leave the pit box
            self.lap_start = this;
        }
        if this.session_state == ir::SessionState::ParadeLaps
            && self.last.session_state != this.session_state
        {
            // reset lap start when the parade lap starts.
            self.lap_start = this;
            // show the stratagy if there's one available
            if let Some(x) = self.calc.strat(this.ends()) {
                strat_to_result(&x, result);
            }
        }
        if this.lap_progress < 0.1 && self.last.lap_progress > 0.9 {
            let new_lap = Lap {
                fuel_left: this.fuel_level,
                fuel_used: self.lap_start.fuel_level - this.fuel_level,
                // TODO, this is not interopolating the lap
                time: Duration::from_millis(
                    ((this.session_time - self.lap_start.session_time) * 1000.0) as u64,
                ),
                condition: this.lap_state() | self.lap_start.lap_state(),
            };
            if this.session_state != ir::SessionState::Checkered
                && this.session_state != ir::SessionState::CoolDown
            {
                self.calc.add_lap(new_lap);
                if let Some(strat) = self.calc.strat(this.ends()) {
                    strat_to_result(&strat, result)
                }
            }
            result.fuel_last_lap = new_lap.fuel_used;
            self.lap_start = this;
        }
        // update status info in result
        result.car.fuel = this.fuel_level;
        // update race time/laps left from source, not strat
        let tick = this.session_time - self.last.session_time;
        let dtick = Duration::from_millis((tick * 1000.0) as u64);
        if result.car.time.d > dtick {
            result.car.time.d -= dtick;
        } else {
            result.car.time.d = Duration::ZERO;
        }
        match this.ends() {
            EndsWith::Laps(l) => {
                result.race.laps = l;
                result.race.time.d -= dtick;
            }
            EndsWith::Time(d) => {
                result.race.time.d = d;
            }
            EndsWith::LapsOrTime(l, d) => {
                result.race.laps = l;
                result.race.time.d = d;
            }
        }
        self.last = this;
        Ok(())
    }
}
impl<'a> Drop for CalcState<'a> {
    fn drop(&mut self) {
        self.calc.save_laps().unwrap(); //TODO
    }
}
impl<'a> IrCalc<'a> {
    pub fn new() -> IrCalc<'a> {
        IrCalc {
            client: unsafe { ir::Client::new() },
            state: None,
        }
    }
    pub fn update(&mut self, result: &mut State) {
        unsafe {
            if self.state.is_some() {
                if self.state.unwrap().ir.expired() {
                    *result = State::default();
                    self.state = None
                }
            }
            if self.state.is_none() {
                match self.client.session() {
                    None => {
                        *result = State::default();
                        return;
                    }
                    Some(session) => match CalcState::new(session) {
                        Err(e) => {
                            *result = State::default();
                            return;
                        }
                        Ok(cs) => {
                            self.state = Some(cs);
                            result.connected = true;
                        }
                    },
                }
            }
        }
        if let Some(cs) = &mut self.state {
            match cs.update(result) {
                Ok(_) => {}
                Err(ir::Error::SessionExpired) => {
                    *result = State::default();
                    self.state = None;
                }
                Err(e) => {
                    panic!("programmer error {:?}", e);
                }
            }
        }
    }
}
fn strat_to_result(strat: &Strategy, result: &mut State) {
    result.save = strat.fuel_to_save;
    if strat.stops.is_empty() {
        result.next_stop = None;
    } else {
        result.next_stop = Some(*strat.stops.first().unwrap());
    }
    result.stops = strat.stops.len() as i32;
    result.fuel_avg = strat.green.fuel;
    result.car.laps = strat.stints.first().map(|s| s.laps).unwrap_or_default();
    result.car.time.d = strat.stints.first().map(|s| s.time).unwrap_or_default();
    result.race.laps = strat.stints.iter().map(|s| s.laps).sum();
    result.race.fuel = strat.stints.iter().map(|s| s.fuel).sum();
    result.race.time.d = strat.stints.iter().map(|s| s.time).sum();
}

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
            s |= LapState::YELLOW
        }
        if self.player_track_surface == ir::TrackLocation::ApproachingPits
            || self.player_track_surface == ir::TrackLocation::InPitStall
        {
            s |= LapState::PITTED
        }
        if self.session_state == ir::SessionState::ParadeLaps
            || self.session_state == ir::SessionState::Warmup
        {
            s |= LapState::PACE_LAP
        }
        if f.intersects(ir::Flags::ONE_TO_GREEN) && s.intersects(LapState::YELLOW) {
            s |= LapState::ONE_TO_GREEN
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

#[derive(Debug)]
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
    fn new(c: &ir::Session) -> Result<CarStateFactory, ir::VariableError> {
        unsafe {
            Ok(CarStateFactory {
                session_num: c.find_var("SessionNum")?,
                session_time: c.find_var("SessionTime")?,
                is_on_track: c.find_var("IsOnTrack")?,
                player_track_surface: c.find_var("PlayerTrackSurface")?,
                session_state: c.find_var("SessionState")?,
                session_flags: c.find_var("SessionFlags")?,
                session_time_remain: c.find_var("SessionTimeRemain")?,
                session_laps_remain: c.find_var("SessionLapsRemainEx")?,
                session_time_total: c.find_var("SessionTimeTotal")?,
                session_laps_total: c.find_var("SessionLapsTotal")?,
                lap: c.find_var("Lap")?,
                lap_completed: c.find_var("LapCompleted")?,
                race_laps: c.find_var("RaceLaps")?,
                fuel_level: c.find_var("FuelLevel")?,
                lap_progress: c.find_var("LapDistPct")?,
            })
        }
    }
    fn read(&mut self, c: &ir::Session) -> Result<CarState, ir::Error> {
        unsafe {
            Ok(CarState {
                session_num: c.value(&mut self.session_num)?,
                session_time: c.value(&mut self.session_time)?,
                is_on_track: c.value(&mut self.is_on_track)?,
                player_track_surface: c.value(&mut self.player_track_surface)?,
                session_state: c.value(&mut self.session_state)?,
                session_flags: c.value(&mut self.session_flags)?,
                session_time_remain: c.value(&mut self.session_time_remain)?,
                session_laps_remain: c.value(&mut self.session_laps_remain)?,
                session_time_total: c.value(&mut self.session_time_total)?,
                session_laps_total: c.value(&mut self.session_laps_total)?,
                lap: c.value(&mut self.lap)?,
                lap_completed: c.value(&mut self.lap_completed)?,
                race_laps: c.value(&mut self.race_laps)?,
                fuel_level: c.value(&mut self.fuel_level)?,
                lap_progress: c.value(&mut self.lap_progress)?,
            })
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
        let yamls = yaml_rust::YamlLoader::load_from_str(session_info).unwrap(); // TODO
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
