#![allow(dead_code)]

use super::calc::{Calculator, RaceConfig};
use super::ir;
use super::ir::flags::{Flags, SessionState, TrackLocation};
use super::ir::DataUpdateResult;
use super::strat::{EndsWith, Lap, LapState, Pitstop, Rate, Strategy};
use druid::{Data, Lens};
use std::fmt;
use std::time::Duration;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Data, Lens)]
pub struct AmountLeft {
    pub fuel: f32,
    pub laps: i32,
    #[data(same_fn = "PartialEq::eq")]
    pub time: Duration,
}
impl Default for AmountLeft {
    fn default() -> Self {
        AmountLeft {
            fuel: 0.0,
            laps: 0,
            time: Duration::ZERO,
        }
    }
}
#[derive(Clone, Debug, Data, Lens)]
pub struct Estimation {
    pub connected: bool,            // connected to iracing
    pub car: AmountLeft,            // what's left in the car
    pub race: AmountLeft,           // what's left to go in the race
    pub fuel_last_lap: f32,         // fuel used on the last lap
    pub green: Rate,                // average per lap usage (green flag only)
    pub stops: i32,                 // pitstops needed to finish race
    pub next_stop: Option<Pitstop>, // details on the next pitstop
    pub save: f32,                  // save this much fuel to skip the last pitstop
    pub save_target: f32,           // target fuel usage per lap to meet save target
}
impl Default for Estimation {
    fn default() -> Self {
        Estimation {
            connected: false,
            car: AmountLeft::default(),
            race: AmountLeft::default(),
            fuel_last_lap: 0.0,
            green: Rate::default(),
            stops: 0,
            next_stop: None,
            save: 0.0,
            save_target: 0.0,
        }
    }
}

pub struct Estimator {
    client: ir::Client,
    state: Option<SessionProgress>,
}

#[derive(Debug)]
enum Error {
    TypeMismatch(ir::Error),
    SessionExpired,
}
impl From<ir::Error> for Error {
    fn from(x: ir::Error) -> Self {
        Error::TypeMismatch(x)
    }
}

// state needed by a running calculator
struct SessionProgress {
    ir: ir::Session,
    calc: Calculator,
    f: TelemetryFactory,
    last: IRacingTelemetryRow,
    lap_start: IRacingTelemetryRow,
}
impl SessionProgress {
    fn new(session: ir::Session) -> Result<SessionProgress, ir::Error> {
        let session_info = IrSessionInfo::parse(unsafe { &session.session_info() }, 0);
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
        let f = TelemetryFactory::new(&session);
        let last = f.read(&session)?;
        Ok(SessionProgress {
            calc,
            f,
            ir: session,
            last,
            lap_start: last,
        })
    }
    fn read(&mut self) -> Result<IRacingTelemetryRow, ir::Error> {
        self.f.read(&self.ir)
    }
    fn update(&mut self, result: &mut Estimation) -> Result<(), Error> {
        unsafe {
            if self.ir.get_new_data() == DataUpdateResult::SessionExpired {
                return Err(Error::SessionExpired);
            }
        };
        let this = self.read()?;
        if this.session_time < self.last.session_time {
            // If the session time goes backwards then we've moved between
            // different sessions inside a single race, e.g. practice -> qualy
            self.calc.save_laps().unwrap(); //TODO
            self.last = this;
            self.lap_start = this;
        }
        if (!self.lap_start.is_on_track) && this.is_on_track {
            // ensure lap_start is from when we're in the car.
            self.lap_start = this;
        }
        if self.last.player_track_surface == TrackLocation::InPitStall
            && this.player_track_surface != self.last.player_track_surface
        {
            // reset lap start when we leave the pit box
            self.lap_start = this;
            // show the stratagy if there's one available
            if let Some(x) = self.calc.strat(this.ends()) {
                strat_to_result(&x, result);
            }
        }
        if this.session_state == SessionState::ParadeLaps
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
                time: Duration::from_secs_f64(this.session_time - self.lap_start.session_time),
                condition: this.lap_state() | self.lap_start.lap_state(),
            };
            if this.session_state != SessionState::Checkered
                && this.session_state != SessionState::CoolDown
            {
                self.calc.add_lap(new_lap);
                if let Some(strat) = self.calc.strat(this.ends()) {
                    strat_to_result(&strat, result)
                }
            }
            result.fuel_last_lap = new_lap.fuel_used;
            self.lap_start = this;
        }
        // update car status info in result
        result.car.fuel = this.fuel_level;
        if this.is_on_track {
            result.race.fuel =
                (result.race.fuel - (self.last.fuel_level - this.fuel_level).max(0.0)).max(0.0)
        }
        if result.green.fuel > 0.0 {
            result.car.laps = f32::floor(this.fuel_level / result.green.fuel) as i32;
            result.car.time = Duration::from_secs_f32(
                this.fuel_level / result.green.fuel * result.green.time.as_secs_f32(),
            );
        } else {
            result.car.laps = 0;
            result.car.time = Duration::ZERO;
        }
        // update race time/laps left from source, not strat
        // TODO during parade laps show race total time/laps not the parade time numbers.
        // this also needs feeding into the strat() calls.
        let tick = this.session_time - self.last.session_time;
        let dtick = Duration::from_secs_f64(tick);
        match this.ends() {
            EndsWith::Laps(l) => {
                result.race.laps = l;
                result.race.time -= Duration::min(result.race.time, dtick);
            }
            EndsWith::Time(d) => {
                result.race.time = d;
            }
            EndsWith::LapsOrTime(l, d) => {
                result.race.laps = l;
                result.race.time = d;
            }
        }
        self.last = this;
        Ok(())
    }
}
impl Drop for SessionProgress {
    fn drop(&mut self) {
        self.calc.save_laps().unwrap(); //TODO
    }
}
impl Estimator {
    pub fn new() -> Estimator {
        Estimator {
            client: ir::Client::new(),
            state: None,
        }
    }
    pub fn update(&mut self, result: &mut Estimation) {
        unsafe {
            if self.state.is_none() {
                match self.client.session() {
                    None => {
                        *result = Estimation::default();
                        return;
                    }
                    Some(session) => match SessionProgress::new(session) {
                        Err(_) => {
                            *result = Estimation::default();
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
                Err(Error::SessionExpired) => {
                    *result = Estimation::default();
                    self.state = None;
                }
                Err(e) => {
                    panic!("programmer error {:?}", e);
                }
            }
        }
    }
}
fn strat_to_result(strat: &Strategy, result: &mut Estimation) {
    result.save = strat.fuel_to_save;
    if strat.stops.is_empty() {
        result.next_stop = None;
    } else {
        result.next_stop = Some(*strat.stops.first().unwrap());
    }
    result.stops = strat.stops.len() as i32;
    result.green = strat.green;
    result.race.laps = strat.stints.iter().map(|s| s.laps).sum();
    result.race.fuel = strat.stints.iter().map(|s| s.fuel).sum();
    result.race.time = strat.stints.iter().map(|s| s.time).sum();
    result.save_target = if strat.fuel_to_save <= 0.0 {
        0.0
    } else {
        let laps_til_last_stop: i32 = strat.stints.iter().rev().skip(1).map(|s| s.laps).sum();
        strat.fuel_to_save / (laps_til_last_stop as f32)
    }
}

#[derive(Clone, Copy, Debug)]
struct IRacingTelemetryRow {
    session_num: i32,
    session_time: f64,
    is_on_track: bool,
    player_track_surface: TrackLocation,
    session_state: SessionState,
    session_flags: Flags,
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
impl IRacingTelemetryRow {
    fn time_remaining(&self) -> Duration {
        Duration::from_secs_f64(self.session_time_remain.max(0.0))
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
            Flags::YELLOW | Flags::YELLOW_WAVING | Flags::CAUTION_WAVING | Flags::CAUTION,
        ) {
            s |= LapState::YELLOW
        }
        if self.player_track_surface == TrackLocation::ApproachingPits
            || self.player_track_surface == TrackLocation::InPitStall
        {
            s |= LapState::PITTED
        }
        if self.session_state == SessionState::ParadeLaps
            || self.session_state == SessionState::Warmup
        {
            s |= LapState::PACE_LAP
        }
        if f.intersects(Flags::ONE_TO_GREEN) && s.intersects(LapState::YELLOW) {
            s |= LapState::ONE_TO_GREEN
        }
        s
    }
}
impl fmt::Display for IRacingTelemetryRow {
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
            self.fuel_level,
            self.lap_progress,
            self.session_flags,
        )
    }
}

#[derive(Debug)]
struct TelemetryFactory {
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
impl TelemetryFactory {
    fn new(c: &ir::Session) -> TelemetryFactory {
        unsafe {
            TelemetryFactory {
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
    }
    fn read(&self, c: &ir::Session) -> Result<IRacingTelemetryRow, ir::Error> {
        unsafe {
            Ok(IRacingTelemetryRow {
                session_num: c.value(&self.session_num)?,
                session_time: c.value(&self.session_time)?,
                is_on_track: c.value(&self.is_on_track)?,
                player_track_surface: c.value(&self.player_track_surface)?,
                session_state: c.value(&self.session_state)?,
                session_flags: c.value(&self.session_flags)?,
                session_time_remain: c.value(&self.session_time_remain)?,
                session_laps_remain: c.value(&self.session_laps_remain)?,
                session_time_total: c.value(&self.session_time_total)?,
                session_laps_total: c.value(&self.session_laps_total)?,
                lap: c.value(&self.lap)?,
                lap_completed: c.value(&self.lap_completed)?,
                race_laps: c.value(&self.race_laps)?,
                fuel_level: c.value(&self.fuel_level)?,
                lap_progress: c.value(&self.lap_progress)?,
            })
        }
    }
}

#[derive(Clone, Debug)]
struct IrSessionInfo {
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

impl IrSessionInfo {
    fn parse(session_info: &str, session_num: i32) -> IrSessionInfo {
        let yamls = yaml_rust::YamlLoader::load_from_str(session_info).unwrap(); // TODO
        let si = &yamls[0];
        let di = &si["DriverInfo"];
        let wi = &si["WeekendInfo"];
        let driver = &di["Drivers"][di["DriverCarIdx"].as_i64().unwrap() as usize];
        let sessions = &si["SessionInfo"]["Sessions"];
        IrSessionInfo {
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
