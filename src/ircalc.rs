#![allow(dead_code)]

use super::history::{Adjustments, History, RaceSession};
use super::strat::{EndsWith, Lap, LapState, Pitstop, Rate, Strategy, TimeSpan};
use chrono::{DateTime, Local};
use druid::{Data, Lens};
use ir::flags::{BroadcastMsg, PitCommand};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::{fmt, io};

use iracing_telem as ir;
use iracing_telem::flags::{Flags, SessionState, TrackLocation};
use iracing_telem::DataUpdateResult;

#[derive(Clone, Debug, Data, Lens)]
pub struct AmountLeft {
    pub fuel: f32,
    pub laps: f32,
    pub time: TimeSpan,
}
impl Default for AmountLeft {
    fn default() -> Self {
        AmountLeft {
            fuel: 0.0,
            laps: 0.0,
            time: TimeSpan::ZERO,
        }
    }
}

#[derive(Clone, Debug, Data, Lens)]
pub struct Estimation {
    pub connected: bool,            // connected to iracing
    pub car: AmountLeft,            // what's left in the car
    pub race: AmountLeft,           // what's left to go in the race
    pub race_tm_estimated: bool,    // the race time left is an estimate
    pub race_laps_estimated: bool,  // the race laps left is an estimate
    pub fuel_last_lap: f32,         // fuel used on the last lap
    pub green: Rate,                // average per lap usage (green flag only)
    pub stops: i32,                 // pitstops needed to finish race
    pub next_stop: Option<Pitstop>, // details on the next pitstop
    pub save: f32,                  // save this much fuel to skip the last pitstop
    pub save_target: f32,           // target fuel usage per lap to meet save target
    pub track_temp: f32,            // current track temp
    pub start_track_temp: f32,      // track temp at the start of the session
    #[data(same_fn = "PartialEq::eq")]
    pub now: DateTime<Local>, // current local (the simulator PC) date/time
}
impl Default for Estimation {
    fn default() -> Self {
        Estimation {
            connected: false,
            car: AmountLeft::default(),
            race: AmountLeft::default(),
            race_laps_estimated: true,
            race_tm_estimated: true,
            fuel_last_lap: 0.0,
            green: Rate::default(),
            stops: 0,
            next_stop: None,
            save: 0.0,
            save_target: 0.0,
            track_temp: 0.0,
            start_track_temp: 0.0,
            now: Local::now(),
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

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Data, Lens)]
pub struct UserSettings {
    /// 0-1 the max percentage fuel saving to consider
    pub max_fuel_save: f32,
    /// cars typically start to stutter around 0.2-0.3L of fuel left
    /// What's the minimum we should try to keep in it.
    pub min_fuel: f32,
    /// when refueling add enough fuel for this many extra laps.
    pub extra_laps: f32,
    /// when refueling add this amount of extra fuel. Will pick the larger
    /// of this or extra_laps.
    pub extra_fuel: f32,
    /// always clear tires when setting pitstop options.
    pub clear_tires: bool,
    /// always take tires when setting pitstop options.
    pub take_tires: bool,
}
impl Default for UserSettings {
    fn default() -> UserSettings {
        UserSettings {
            max_fuel_save: 0.15,
            min_fuel: 0.2,
            extra_laps: 2.0,
            extra_fuel: 1.0,
            clear_tires: false,
            take_tires: false,
        }
    }
}

#[derive(Debug)]
pub enum JsonLoadError {
    IOError(io::Error),
    JsonError(serde_json::Error),
}
impl From<io::Error> for JsonLoadError {
    fn from(e: io::Error) -> Self {
        JsonLoadError::IOError(e)
    }
}
impl From<serde_json::Error> for JsonLoadError {
    fn from(e: serde_json::Error) -> Self {
        JsonLoadError::JsonError(e)
    }
}
impl UserSettings {
    pub fn load(path: Option<PathBuf>) -> UserSettings {
        match path {
            None => Self::default(),
            Some(p) => match Self::load_impl(p) {
                Ok(s) => s,
                Err(e) => {
                    println!("Failed to load settings {:?}", e);
                    Self::default()
                }
            },
        }
    }
    fn load_impl(path: PathBuf) -> Result<UserSettings, JsonLoadError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let r: UserSettings = serde_json::from_reader(reader)?;
        Ok(r)
    }
    pub fn save(&self, path: Option<PathBuf>) -> Result<(), JsonLoadError> {
        match path {
            None => Ok(()),
            Some(p) => {
                let file = File::create(p)?;
                serde_json::to_writer_pretty(file, self)?;
                Ok(())
            }
        }
    }
}

pub fn default_laps_db() -> Option<PathBuf> {
    dirs_next::document_dir().map(|dir| dir.join("naf_calc\\laps.db"))
}
pub fn default_settings_file() -> Option<PathBuf> {
    dirs_next::document_dir().map(|dir| dir.join("naf_calc\\settings.json"))
}

// state needed by a running calculator
struct SessionProgress {
    ir: ir::Session,
    calc: History,
    f: TelemetryFactory,
    last: IRacingTelemetryRow,
    lap_start: IRacingTelemetryRow,
    first: IRacingTelemetryRow,
}
impl SessionProgress {
    fn new(session: ir::Session, settings: &UserSettings) -> Result<SessionProgress, ir::Error> {
        let session_info = IrSessionInfo::parse(unsafe { &session.session_info() }, 0);
        let cfg = RaceSession {
            fuel_tank_size: (session_info.driver_car_fuel_max_ltr
                * session_info.driver_car_max_fuel_pct) as f32,
            max_fuel_save: settings.max_fuel_save,
            min_fuel: settings.min_fuel,
            track_id: session_info.track_id,
            track_name: session_info.track_display_name,
            layout_name: session_info.track_config_name,
            car_id: session_info.car_id,
            car: session_info.car_name,
        };
        let calc = History::new(cfg, default_laps_db()).unwrap();
        let f = TelemetryFactory::new(&session);
        let last = f.read(&session)?;
        Ok(SessionProgress {
            ir: session,
            calc,
            f,
            last,
            lap_start: last,
            first: last,
        })
    }
    fn read(&mut self) -> Result<IRacingTelemetryRow, ir::Error> {
        self.f.read(&self.ir)
    }
    fn update(&mut self, settings: &UserSettings, result: &mut Estimation) -> Result<(), Error> {
        unsafe {
            if self.ir.get_new_data() == DataUpdateResult::SessionExpired {
                return Err(Error::SessionExpired);
            }
        };
        let adj = Adjustments {
            max_fuel_save: Some(settings.max_fuel_save),
            min_fuel: Some(settings.min_fuel),
        };
        let this = self.read()?;
        if this.session_time < self.last.session_time {
            // If the session time goes backwards then we've moved between
            // different sessions inside a single race, e.g. practice -> qualy
            self.calc.save_laps().unwrap(); // TODO
            self.last = this;
            self.lap_start = this;
            self.first = this;
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
            if let Some(x) = self.calc.strat(this.fuel_level, &adj, this.ends()) {
                strat_to_result(&x, result);
            }
        }
        if this.session_state == SessionState::ParadeLaps
            && self.last.session_state != this.session_state
        {
            // reset lap start when the parade lap starts.
            self.lap_start = this;
            // show the stratagy if there's one available
            if let Some(x) = self.calc.strat(this.fuel_level, &adj, this.ends()) {
                strat_to_result(&x, result);
            }
        }
        if this.lap_progress < 0.1 && self.last.lap_progress > 0.9 {
            let new_lap = Lap {
                fuel_left: this.fuel_level,
                fuel_used: self.lap_start.fuel_level - this.fuel_level,
                time: Self::interpolate_checkpoint_time(
                    self.last.lap_progress,
                    self.last.session_time,
                    this.lap_progress,
                    this.session_time,
                    0.0,
                ) - TimeSpan::from_secs_f64(self.lap_start.session_time),
                condition: this.lap_state() | self.lap_start.lap_state(),
            };
            if this.session_state != SessionState::Checkered
                && this.session_state != SessionState::CoolDown
            {
                if new_lap.fuel_used > 0.0 {
                    // reset to pit, towing etc can end up with have a negative fuel used
                    // so skip those, they're junk.
                    self.calc.add_lap(new_lap);
                }
                if let Some(strat) = self.calc.strat(this.fuel_level, &adj, this.ends()) {
                    strat_to_result(&strat, result)
                }
            }
            result.fuel_last_lap = new_lap.fuel_used;
            self.lap_start = this;
        }
        if this.player_track_surface == TrackLocation::ApproachingPits
            && self.last.player_track_surface != TrackLocation::ApproachingPits
        {
            if settings.clear_tires {
                unsafe {
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::ClearTires));
                }
            } else if settings.take_tires {
                unsafe {
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::LF(None)));
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::RF(None)));
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::LR(None)));
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::RR(None)));
                }
            }
            match self.calc.strat(this.fuel_level, &adj, this.ends()) {
                None => unsafe {
                    let _ = self
                        .ir
                        .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::Fuel(Some(
                            self.calc.config().fuel_tank_size.ceil() as i16,
                        ))));
                },
                Some(x) => unsafe {
                    let total: f32 = x.total_fuel();
                    let add = (total - this.fuel_level
                        + (settings.extra_fuel.max(x.green.fuel * settings.extra_laps)))
                    .ceil();
                    if add > 0.0 {
                        let _ = self
                            .ir
                            .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::Fuel(Some(
                                add as i16,
                            ))));
                    } else {
                        let _ = self
                            .ir
                            .broadcast_msg(BroadcastMsg::PitCommand(PitCommand::ClearFuel));
                    }
                },
            }
        }
        // update car status info in result
        result.car.fuel = this.fuel_level;
        if this.is_on_track {
            result.race.fuel =
                (result.race.fuel - (self.last.fuel_level - this.fuel_level).max(0.0)).max(0.0)
        }
        if result.green.fuel > 0.0 {
            result.car.laps = this.fuel_level / result.green.fuel;
            result.car.time = TimeSpan::from_secs_f32(
                this.fuel_level / result.green.fuel * result.green.time.as_secs_f32(),
            );
        } else {
            result.car.laps = 0.0;
            result.car.time = TimeSpan::ZERO;
        }
        // update race time/laps left from source, not strat
        let tick = this.session_time - self.last.session_time;
        let dtick = TimeSpan::from_secs_f64(tick);
        match this.ends() {
            EndsWith::Laps(l) => {
                result.race.laps = l as f32;
                result.race.time -= result.race.time.min(dtick);
                result.race_laps_estimated = false;
                result.race_tm_estimated = true;
            }
            EndsWith::Time(d) => {
                result.race.time = d;
                result.race_laps_estimated = true;
                result.race_tm_estimated = false;
            }
            EndsWith::LapsOrTime(l, d) => {
                result.race.laps = l as f32;
                result.race.time = d;
                result.race_laps_estimated = false;
                result.race_tm_estimated = false;
            }
        }
        // update track temp & time
        result.track_temp = this.track_temp;
        result.start_track_temp = self.first.track_temp;
        result.now = Local::now();
        self.last = this;
        Ok(())
    }
    fn interpolate_checkpoint_time(
        // pos'n and time at the end of the lap
        mut end_of_lap_pos: f32,
        end_of_lap_tm: f64,
        // pos'n and time at the start of the next lap
        start_of_lap_pos: f32,
        start_of_lap_tm: f64,
        check_pos: f32,
    ) -> TimeSpan {
        // unwrap if crossing start/finish line
        //****Note, assumes p1 is a percent from 0 to 1
        // if that is not true then unwrap the numbers before calling this function
        if end_of_lap_pos > start_of_lap_pos {
            end_of_lap_pos -= 1.0;
        }
        let pct = ((check_pos - end_of_lap_pos) / (start_of_lap_pos - end_of_lap_pos)) as f64;
        TimeSpan::from_secs_f64(end_of_lap_tm + ((start_of_lap_tm - end_of_lap_tm) * pct))
    }
}
impl Drop for SessionProgress {
    fn drop(&mut self) {
        let _ = self.calc.save_laps(); //TODO
    }
}
impl Estimator {
    pub fn new() -> Estimator {
        Estimator {
            client: ir::Client::new(),
            state: None,
        }
    }
    pub fn update(&mut self, settings: &UserSettings, result: &mut Estimation) {
        unsafe {
            if self.state.is_none() {
                match self.client.session() {
                    None => {
                        *result = Estimation::default();
                        return;
                    }
                    Some(session) => match SessionProgress::new(session, settings) {
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
            match cs.update(settings, result) {
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
    result.race.laps = strat.total_laps() as f32;
    result.race.fuel = strat.total_fuel();
    result.race.time = strat.total_time();
    result.save_target = strat.fuel_target();
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
    track_temp: f32,
}
impl IRacingTelemetryRow {
    fn ends(&self) -> EndsWith {
        let (tm, laps) = match self.session_state {
            SessionState::Warmup | SessionState::ParadeLaps => {
                (self.session_time_total, self.session_laps_total)
            }
            _ => (self.session_time_remain, self.session_laps_remain),
        };
        // TODO deal with practice better
        if tm == ir::IRSDK_UNLIMITED_TIME {
            if laps == ir::IRSDK_UNLIMITED_LAPS {
                EndsWith::Time(TimeSpan::from_secs_f64(
                    (30.0 * 60.0 - self.session_time).max(0.0),
                ))
            } else {
                EndsWith::Laps(laps)
            }
        } else if laps == ir::IRSDK_UNLIMITED_LAPS {
            EndsWith::Time(TimeSpan::from_secs_f64(tm.max(0.0)))
        } else {
            EndsWith::LapsOrTime(laps, TimeSpan::from_secs_f64(tm.max(0.0)))
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
    track_temp: ir::Var,
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
                track_temp: c.find_var("TrackTempCrew").unwrap(),
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
                track_temp: c.value(&self.track_temp)?,
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
            // TrackConfigName doesn't appear for tracks that don't have multiple configs
            track_config_name: wi["TrackConfigName"].as_str().unwrap_or("").to_string(),
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

#[cfg(test)]
mod tests {
    use super::SessionProgress;

    #[test]
    fn test_interopolate_tm() {
        let tm = SessionProgress::interpolate_checkpoint_time(0.98, 112.1, 0.02, 112.3, 0.0);
        assert!(f64::abs(tm.as_secs_f64() - 112.2) < 0.0001);

        let tm2 = SessionProgress::interpolate_checkpoint_time(0.98, 112.1, 0.02, 112.5, 0.0);
        assert!(f64::abs(tm2.as_secs_f64() - 112.3) < 0.0001);

        let tm3 = SessionProgress::interpolate_checkpoint_time(0.99, 112.1, 0.02, 112.4, 0.0);
        assert!(f64::abs(tm3.as_secs_f64() - 112.2) < 0.0001);
    }
}
