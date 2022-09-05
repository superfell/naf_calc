// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use druid::debug_state::DebugState;
use druid::piet::{Text, TextLayout, TextLayoutBuilder};
use druid::widget::{
    Align, Button, Checkbox, Flex, Label, LabelText, Painter, SizedBox, TextBox, ViewSwitcher,
};
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, EventCtx, FontDescriptor, FontFamily,
    FontWeight, Insets, Key, KeyOrValue, LayoutCtx, Lens, LifeCycle, LifeCycleCtx, PaintCtx, Point,
    Rect, RenderContext, Size, UnitPoint, UpdateCtx, Widget, WidgetExt, WidgetId, WidgetPod,
    WindowDesc,
};
use druid::{LensExt, TimerToken};
use druid_widget_nursery::DropdownSelect;
use flexi_logger::{Duplicate, FileSpec, Logger};
use history::RaceSession;
use ircalc::{AmountLeft, Estimation, UserSettings};
use log::info;
use std::fmt::Display;
use std::marker::PhantomData;
use std::mem;
use std::ops::Add;
use std::str::FromStr;
use std::time::Duration;
use strat::{EndsWith, Rate, StratRequest, TimeSpan};

mod history;
mod ircalc;
mod strat;

static TIMER_INTERVAL: Duration = Duration::from_millis(100);

// struct Events {}
// impl sapi_lite::tts::EventHandler for Events {
//     fn on_speech_finished(&self, id: u32) {
//         println!("on_rec {}", id);
//     }
// }

fn main() {
    // let events = Events {};
    // sapi_lite::initialize().unwrap();
    // let synth = sapi_lite::tts::EventfulSynthesizer::new(events).unwrap();
    // synth.speak("Pit in the next 5 laps").unwrap();
    let logger = Logger::try_with_str("info")
        .unwrap()
        .log_to_file(FileSpec::default()) // write logs to file
        .duplicate_to_stderr(Duplicate::Warn) // print warnings and errors also to the console
        .format_for_files(flexi_logger::detailed_format)
        .start()
        .unwrap();
    info!("naf_calc starting");
    log_panics::init();
    let logger_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        logger_hook(info);
        logger.flush();
        std::process::exit(-1);
    }));
    let sessions = history::Db::new(&ircalc::default_laps_db().unwrap())
        .unwrap()
        .sessions()
        .unwrap();
    // create the initial app state
    let mut initial_state = UiState {
        offline: OfflineState {
            session: sessions[0].clone(),
            green: None,
            yellow: None,
            laps: None,
            time: Some(TimeSpan::new(50 * 60, 0)),
            fuel_tank_size: None,
            max_fuel_save: None,
            strat: None,
        },
        online: ircalc::Estimation::default(),
        settings_editor: EditableSettings::default(),
        settings: UserSettings::load(ircalc::default_settings_file()),
        show_settings: false,
    };
    initial_state.offline.on_session_change();
    initial_state.offline.recalc();

    let monitors = druid::Screen::get_monitors();
    let mut m = &monitors[0];
    for cm in &monitors {
        if cm.virtual_work_rect().height() < m.virtual_work_rect().height() {
            m = cm;
        }
    }
    let mr = m.virtual_work_rect();
    // describe the main window
    let main_window = WindowDesc::new(build_root_widget())
        .title("naf calc")
        .window_size((900.0, 480.0))
        .set_position(Point::new(mr.min_x(), mr.min_y()));

    // start the application
    AppLauncher::with_window(main_window)
        .launch(initial_state)
        .expect("Failed to launch application");
}

fn build_root_widget() -> impl Widget<UiState> {
    let mut calc = ircalc::Estimator::new();
    let vs = ViewSwitcher::new(
        |v: &UiState, _env: &Env| {
            if !v.show_settings {
                if v.online.connected {
                    UiView::Online
                } else {
                    UiView::Offline
                }
            } else {
                UiView::Settings
            }
        },
        |active: &UiView, _s: &UiState, _env: &Env| match *active {
            UiView::Online => build_active_dash().boxed(),
            UiView::Offline => build_offline_widget().boxed(),
            UiView::Settings => build_settings_widget().boxed(),
        },
    );
    TimerWidget {
        on_fire: move |d: &mut UiState| calc.update(&d.settings, &mut d.online),
        timer_id: TimerToken::INVALID,
        widget: vs,
        p: PhantomData,
    }
}

const LABEL_TEXT_SIZE: f64 = 32.0;

fn lbl<T: Data>(l: impl Into<LabelText<T>>, align: UnitPoint) -> impl Widget<T> {
    SizedBox::new(Align::new(
        align,
        Label::new(l)
            .with_text_size(LABEL_TEXT_SIZE)
            .with_text_color(Color::grey8(200)),
    ))
}
fn val<T: Data>(text: impl Into<LabelText<T>>, color: Option<KeyOrValue<Color>>) -> impl Widget<T> {
    let font = FontDescriptor::new(FontFamily::SYSTEM_UI)
        .with_weight(FontWeight::BOLD)
        .with_size(48.0);
    let mut lbl = Label::<T>::new(text).with_font(font);
    if let Some(c) = color {
        lbl = lbl.with_text_color(c);
    }
    Align::new(UnitPoint::TOP, lbl)
}

const COLOR_BG_KEY: Key<Color> = Key::new("color-bg-key");
const COLOR_KEY: Key<Color> = Key::new("color-key");
const COLOR_CLEAR: Color = Color::rgba8(0, 0, 0, 0);

fn colorer<T: PartialOrd + Copy + Add<Output = T>>(
    enable: bool,
    car: T,
    race: T,
    buffer: T,
) -> Color {
    if !enable {
        COLOR_CLEAR
    } else if car >= race + buffer {
        Color::GREEN
    } else if car >= race {
        Color::PURPLE
    } else {
        Color::BLACK
    }
}

const GRID: Color = Color::GRAY;
const GWIDTH: f64 = 1.0;

#[derive(Default, Debug, Clone, Copy, Data, Lens)]
struct EditableSettings {
    max_fuel_save: Option<f32>,
    min_fuel: Option<f32>,
    extra_laps: Option<f32>,
    extra_fuel: Option<f32>,
    clear_tires: bool,
    take_tires: bool,
}
impl EditableSettings {
    fn load(&mut self, s: &UserSettings) {
        self.max_fuel_save = Some(s.max_fuel_save);
        self.min_fuel = Some(s.min_fuel);
        self.extra_laps = Some(s.extra_laps);
        self.extra_fuel = Some(s.extra_fuel);
        self.clear_tires = s.clear_tires;
        self.take_tires = s.take_tires;
    }
    fn update(&self, s: &mut UserSettings) {
        if let Some(m) = self.max_fuel_save {
            s.max_fuel_save = m;
        }
        if let Some(m) = self.min_fuel {
            s.min_fuel = m;
        }
        if let Some(m) = self.extra_laps {
            s.extra_laps = m;
        }
        if let Some(m) = self.extra_fuel {
            s.extra_fuel = m;
        }
        s.clear_tires = self.clear_tires;
        s.take_tires = self.take_tires;
    }
}

fn build_settings_widget() -> impl Widget<UiState> {
    let mut w = GridWidget::new(2, 7);
    for (r, s) in [
        "Max Fuel Save",
        "Min Fuel",
        "Extra Laps",
        "Min Extra Fuel",
        "Clear Tires",
        "Take Tires",
    ]
    .into_iter()
    .enumerate()
    {
        w.set(
            0,
            r,
            lbl(s, UnitPoint::RIGHT).padding(6.0).border(GRID, GWIDTH),
        );
    }
    fn edit_box() -> impl Widget<Option<f32>> {
        Parse::new(TextBox::new().with_text_size(LABEL_TEXT_SIZE).align_left())
    }
    let mut row = 0;
    w.set(
        1,
        row,
        edit_box()
            .lens(EditableSettings::max_fuel_save)
            .lens(UiState::settings_editor)
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        1,
        row,
        edit_box()
            .lens(EditableSettings::min_fuel)
            .lens(UiState::settings_editor)
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        1,
        row,
        edit_box()
            .lens(EditableSettings::extra_laps)
            .lens(UiState::settings_editor)
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        1,
        row,
        edit_box()
            .lens(EditableSettings::extra_fuel)
            .lens(UiState::settings_editor)
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        1,
        row,
        Checkbox::new("")
            .lens(EditableSettings::clear_tires)
            .on_click(|_ctx, data, _env| {
                data.clear_tires = !data.clear_tires;
                if data.clear_tires {
                    data.take_tires = false;
                }
            })
            .lens(UiState::settings_editor)
            .align_left()
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        1,
        row,
        Checkbox::new("")
            .lens(EditableSettings::take_tires)
            .on_click(|_ctx, data, _env| {
                data.take_tires = !data.take_tires;
                if data.take_tires {
                    data.clear_tires = false;
                }
            })
            .lens(UiState::settings_editor)
            .align_left()
            .padding(6.0)
            .border(GRID, GWIDTH),
    );
    row += 1;
    w.set(
        0,
        row,
        Button::from_label(Label::new("Cancel").with_text_size(LABEL_TEXT_SIZE))
            .align_right()
            .padding(6.0)
            .on_click(|_ctx, data: &mut UiState, _env| {
                data.show_settings = false;
            }),
    );
    w.set(
        1,
        row,
        Button::from_label(Label::new("Save").with_text_size(LABEL_TEXT_SIZE))
            .align_left()
            .padding(6.0)
            .on_click(|_ctx, data: &mut UiState, _env| {
                data.settings_editor.update(&mut data.settings);
                let _ = data.settings.save(ircalc::default_settings_file());
                data.show_settings = false;
            }),
    );

    w
}

fn build_active_dash() -> impl Widget<UiState> {
    let mut w = GridWidget::new(4, 8);
    w.set_col_width(0, 150.0);
    w.set_col_width(2, 175.0);
    w.set_row_height(0, 45.0);
    w.set_row_height(3, 15.0);
    w.set(
        0,
        0,
        Button::new("S")
            .padding(6.0)
            .on_click(|_, data: &mut UiState, _| {
                data.settings_editor.load(&data.settings);
                data.show_settings = true;
            })
            .border(GRID, GWIDTH),
    );
    for (r, s) in ["Car", "Race", "", "Last Lap", "Average"]
        .into_iter()
        .enumerate()
    {
        if !s.is_empty() {
            w.set(
                0,
                r + 1,
                lbl(s, UnitPoint::LEFT)
                    .padding(Insets::new(6.0, 0.0, 0.0, 0.0))
                    .border(GRID, GWIDTH),
            );
        } else {
            w.set(0, r + 1, SizedBox::empty().width(10.0).height(10.0));
        }
    }

    for (i, s) in ["Fuel", "Laps", "Time"].into_iter().enumerate() {
        w.set(i + 1, 0, lbl(s, UnitPoint::CENTER).border(GRID, GWIDTH));
    }
    let fmt_f32 = |f: &f32, _e: &Env| format!("{:.2}", f);
    let fmt_f32_blank_zero = |f: &f32, _e: &Env| {
        if *f > 0.0 {
            format!("{:.2}", f)
        } else {
            String::new()
        }
    };
    let fmt_lap = |f: &f32, _: &Env| format!("{:.1}", f);
    let fmt_i32 = |f: &i32, _e: &Env| format!("{:}", f);
    let fmt_ps = |f: &Option<strat::Pitstop>, _e: &Env| match f {
        None => "".to_string(),
        Some(ps) => {
            if ps.is_open() {
                format!("{}", ps.close)
            } else {
                format!("{}-{}", ps.open, ps.close)
            }
        }
    };
    let fmt_tm = |f: &AmountLeft, _e: &Env| format!("{}", f.time);
    w.set(
        1,
        1,
        val(fmt_f32, None)
            .lens(Estimation::car.then(AmountLeft::fuel))
            .border(GRID, GWIDTH)
            .background(COLOR_BG_KEY)
            .env_scope(|env, data| {
                env.set(
                    COLOR_BG_KEY,
                    colorer(data.connected, data.car.fuel, data.race.fuel, 1.0),
                )
            })
            .lens(UiState::online),
    );
    w.set(
        2,
        1,
        val(fmt_lap, None)
            .lens(Estimation::car.then(AmountLeft::laps))
            .border(GRID, GWIDTH)
            .background(COLOR_BG_KEY)
            .env_scope(|env, data| {
                env.set(
                    COLOR_BG_KEY,
                    colorer(data.connected, data.car.laps, data.race.laps, 0.0),
                )
            })
            .lens(UiState::online),
    );
    w.set(
        3,
        1,
        val(fmt_tm, None)
            .lens(Estimation::car)
            .border(GRID, GWIDTH)
            .background(COLOR_BG_KEY)
            .env_scope(|env, data| {
                env.set(
                    COLOR_BG_KEY,
                    colorer(
                        data.connected,
                        data.car.time,
                        data.race.time,
                        TimeSpan::ZERO,
                    ),
                )
            })
            .lens(UiState::online),
    );
    w.set(
        1,
        2,
        val(fmt_f32, None)
            .lens(Estimation::race.then(AmountLeft::fuel))
            .border(GRID, GWIDTH)
            .lens(UiState::online),
    );
    w.set(
        2,
        2,
        val(fmt_lap, Some(KeyOrValue::Key(COLOR_KEY)))
            .lens(Estimation::race.then(AmountLeft::laps))
            .border(GRID, GWIDTH)
            .env_scope(|env, data| {
                env.set(
                    COLOR_KEY,
                    if data.race_laps_estimated {
                        Color::grey8(150)
                    } else {
                        Color::WHITE
                    },
                )
            })
            .lens(UiState::online),
    );
    w.set(
        3,
        2,
        val(fmt_tm, Some(KeyOrValue::Key(COLOR_KEY)))
            .lens(Estimation::race)
            .border(GRID, GWIDTH)
            .env_scope(|env, data| {
                env.set(
                    COLOR_KEY,
                    if data.race_tm_estimated {
                        Color::grey8(150)
                    } else {
                        Color::WHITE
                    },
                )
            })
            .lens(UiState::online),
    );
    w.set(
        1,
        4,
        val(fmt_f32, None)
            .lens(Estimation::fuel_last_lap)
            .border(GRID, GWIDTH)
            .lens(UiState::online),
    );
    let pad_right = Insets::new(0.0, 0.0, 6.0, 0.0);
    w.set(
        2,
        4,
        lbl("Save", UnitPoint::RIGHT)
            .padding(pad_right)
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        4,
        val(fmt_f32_blank_zero, None)
            .lens(Estimation::save)
            .border(GRID, GWIDTH)
            .lens(UiState::online),
    );
    w.set(
        1,
        5,
        val(fmt_f32_blank_zero, None)
            .lens(Estimation::green.then(Rate::fuel))
            .border(GRID, GWIDTH)
            .lens(UiState::online),
    );
    w.set(
        2,
        5,
        lbl("Target", UnitPoint::RIGHT)
            .padding(pad_right)
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        5,
        val(fmt_f32_blank_zero, None)
            .lens(Estimation::save_target)
            .border(GRID, GWIDTH)
            .background(COLOR_BG_KEY)
            .env_scope(|env, data| {
                env.set(
                    COLOR_BG_KEY,
                    if data.save_target > 0.0 {
                        if data.fuel_last_lap <= data.save_target {
                            Color::GREEN
                        } else {
                            Color::BLUE
                        }
                    } else {
                        COLOR_CLEAR
                    },
                )
            })
            .lens(UiState::online),
    );
    w.set(
        0,
        6,
        lbl(
            |d: &Option<strat::Pitstop>, _: &Env| {
                match d {
                    Some(ps) => {
                        if ps.is_open() {
                            "Pits OPEN"
                        } else {
                            "Pits"
                        }
                    }
                    None => "Pits",
                }
                .to_string()
            },
            UnitPoint::LEFT,
        )
        .padding(Insets::new(0.6, 0.0, 0.0, 0.0))
        .lens(UiState::online.then(Estimation::next_stop))
        .border(GRID, GWIDTH),
    );
    w.set(
        1,
        6,
        val(fmt_ps, None)
            .border(GRID, GWIDTH)
            .background(COLOR_BG_KEY)
            .env_scope(|env, data| {
                env.set(
                    COLOR_BG_KEY,
                    match data {
                        None => COLOR_CLEAR,
                        Some(ps) => {
                            if ps.is_open() && ps.close <= 1 {
                                Color::RED
                            } else if ps.is_open() {
                                Color::GREEN
                            } else {
                                Color::BLACK
                            }
                        }
                    },
                )
            })
            .lens(UiState::online.then(Estimation::next_stop))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        6,
        lbl("Stops", UnitPoint::RIGHT)
            .padding(pad_right)
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        6,
        val(fmt_i32, None)
            .lens(UiState::online.then(Estimation::stops))
            .border(GRID, GWIDTH),
    );

    w.set(
        0,
        7,
        lbl("Trk Temp", UnitPoint::RIGHT)
            .padding(pad_right)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        7,
        val(
            |f: &Estimation, _e: &Env| {
                format!(
                    "{:0.1}  {:+0.1}",
                    f.track_temp,
                    f.track_temp - f.start_track_temp
                )
            },
            None,
        )
        .background(COLOR_BG_KEY)
        .env_scope(|env, data| {
            let delta = data.track_temp - data.start_track_temp;
            env.set(
                COLOR_BG_KEY,
                if delta < -1.0 {
                    Color::GREEN
                } else if delta > 1.0 {
                    Color::RED
                } else {
                    COLOR_CLEAR
                },
            )
        })
        .lens(UiState::online)
        .border(GRID, GWIDTH),
    );
    w.set(
        2,
        7,
        lbl("Time", UnitPoint::RIGHT)
            .padding(pad_right)
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        7,
        val(
            |f: &Estimation, _e: &Env| f.now.format("%H:%M:%S").to_string(),
            None,
        )
        .lens(UiState::online)
        .border(GRID, GWIDTH),
    );
    w
}

#[derive(Data, Debug, Clone, Copy, PartialEq)]
enum UiView {
    Offline,
    Online,
    Settings,
}

#[derive(Data, Lens, Debug, Clone)]
struct UiState {
    offline: OfflineState,
    online: Estimation,
    settings_editor: EditableSettings,
    settings: UserSettings,
    show_settings: bool,
}
#[derive(Data, Lens, Clone, Debug, PartialEq)]
struct OfflineState {
    session: RaceSession,
    green: Option<Rate>,
    yellow: Option<Rate>,
    laps: Option<i32>,
    time: Option<TimeSpan>,
    fuel_tank_size: Option<f32>,
    max_fuel_save: Option<f32>,
    #[data(same_fn = "PartialEq::eq")]
    strat: Option<strat::Strategy>,
}
impl OfflineState {
    fn on_session_change(&mut self) {
        self.fuel_tank_size = Some(self.session.fuel_tank_size);
        self.max_fuel_save = Some(self.session.max_fuel_save);
        let _ = history::Db::new(&ircalc::default_laps_db().unwrap()).map(|db| {
            self.green = db.db_green_laps(self.session.car_id, self.session.track_id);
            self.yellow = db.db_yellow_laps(self.session.car_id, self.session.track_id);
        });
    }
    fn recalc(&mut self) {
        if self.fuel_tank_size.is_some()
            && self.max_fuel_save.is_some()
            && (self.laps.is_some() || self.time.is_some())
            && self.green.is_some()
            && self.fuel_tank_size.unwrap() > 0.0
        {
            let r = StratRequest {
                fuel_left: self.fuel_tank_size.unwrap(),
                tank_size: self.fuel_tank_size.unwrap(),
                max_fuel_save: self.max_fuel_save.unwrap(),
                min_fuel: self.session.min_fuel,
                yellow_togo: 0,
                ends: match (self.laps, &self.time) {
                    (Some(l), None) => EndsWith::Laps(l),
                    (None, Some(t)) => EndsWith::Time(*t),
                    (Some(l), Some(t)) => EndsWith::LapsOrTime(l, *t),
                    (None, None) => unreachable!(),
                },
                green: self.green.unwrap(),
                yellow: Rate::default(),
            };
            self.strat = r.compute();
        }
    }
}

fn build_offline_widget() -> impl Widget<UiState> {
    let sessions = history::Db::new(&ircalc::default_laps_db().unwrap())
        .map(|db| db.sessions())
        .unwrap()
        .unwrap();
    let mut grid = GridWidget::new(3, 7);
    grid.set_col_width(0, 200.0);
    grid.set_col_width(2, 50.0);
    grid.set(
        2,
        0,
        Button::new("S")
            .on_click(|_ctx, data: &mut UiState, _env| {
                data.settings_editor.load(&data.settings);
                data.show_settings = true;
            })
            .padding(2.0),
    );
    let os = || UiState::offline.then(OfflineStateLens {});
    for (i, l) in [
        "Car / Track",
        "Green",
        "Yellow",
        "Laps",
        "Time",
        "Fuel Tank Size",
        "Max Save",
    ]
    .iter()
    .enumerate()
    {
        grid.set(
            0,
            i,
            Label::new(*l)
                .with_text_size(24.0)
                .align_right()
                .padding(Insets::new(0.0, 0.0, 3.0, 0.0)),
        );
    }
    grid.set(
        1,
        0,
        DropdownSelect::new(sessions.into_iter().map(|s| (s.car_track(), s)))
            .align_left()
            .lens(OfflineState::session)
            .lens(os()),
    );
    let fmt_rate = |r: &Option<strat::Rate>, _e: &Env| match r {
        Some(r) => format!("{:.2}L / {:.2}s per lap", r.fuel, r.time.as_secs_f64()),
        None => "".to_string(),
    };
    grid.set(
        1,
        1,
        lbl(fmt_rate, UnitPoint::LEFT)
            .lens(OfflineState::green)
            .lens(os()),
    );
    grid.set(
        1,
        2,
        lbl(fmt_rate, UnitPoint::LEFT)
            .lens(OfflineState::yellow)
            .lens(os()),
    );
    grid.set(
        1,
        3,
        Parse::new(TextBox::new().align_left())
            .lens(OfflineState::laps)
            .lens(os()),
    );
    grid.set(
        1,
        4,
        Parse::new(TextBox::new().align_left())
            .lens(OfflineState::time)
            .lens(os()),
    );
    grid.set(
        1,
        5,
        Parse::new(TextBox::new().align_left())
            .lens(OfflineState::fuel_tank_size)
            .lens(os()),
    );
    grid.set(
        1,
        6,
        Parse::new(TextBox::new().align_left())
            .lens(OfflineState::max_fuel_save)
            .lens(os()),
    );
    let strat = Painter::new(|ctx: &mut PaintCtx, data: &OfflineState, _env: &Env| {
        fn draw_lap_num(ctx: &mut PaintCtx, lap: i32, pos: Point) {
            let t = ctx
                .text()
                .new_text_layout(format!("{}", lap))
                .text_color(Color::WHITE)
                .build()
                .unwrap();
            let sz = t.size();
            let fixed_pos = Point::new(pos.x - (sz.width / 2.0), pos.y);
            ctx.draw_text(&t, fixed_pos);
        }
        let mut bounds = ctx.size().to_rect();
        bounds = bounds.inset(Insets::new(-50.0, -20.0, -50.0, -20.0));
        bounds.y0 = bounds.y1 + 10.0;
        ctx.fill(bounds, &Color::GREEN);
        ctx.stroke(bounds, &Color::GRAY, 1.0);
        draw_lap_num(ctx, 0, Point::new(bounds.x0, bounds.y0 - 40.0));
        if let Some(s) = &data.strat {
            let laps: i32 = s.stints.iter().map(|s| s.laps).sum();
            draw_lap_num(ctx, laps, Point::new(bounds.x1, bounds.y0 - 40.0));
            let l64 = laps as f64;
            for stop in &s.stops {
                let b = Rect::new(
                    bounds.width() / l64 * (stop.open as f64) + bounds.x0,
                    bounds.y0 - 20.0,
                    bounds.width() / l64 * (stop.close as f64) + bounds.x0,
                    bounds.y0,
                );
                ctx.fill(b, &Color::rgb8(0, 64, 0));
                ctx.stroke(bounds, &Color::grey8(220), 1.0);
                draw_lap_num(ctx, stop.open, Point::new(b.x0, b.y0 - 20.0));
                draw_lap_num(ctx, stop.close, Point::new(b.x1, b.y0 - 20.0));
            }
        }
    });
    Flex::column()
        .with_default_spacer()
        .with_flex_child(grid, 4.0)
        .with_default_spacer()
        .with_flex_child(
            Label::new(|d: &OfflineState, _: &Env| match &d.strat {
                None => "".to_string(),
                Some(s) => match s.stints.first() {
                    None => format!(
                        "{} stop{}",
                        s.stops.len(),
                        if s.stops.len() == 1 { "" } else { "s" }
                    ),
                    Some(stint) => format!(
                        "{} stop{}. Green flag stint is {} laps / {} time",
                        s.stops.len(),
                        if s.stops.len() == 1 { "" } else { "s" },
                        stint.laps,
                        stint.time
                    ),
                },
            })
            .with_text_size(24.0)
            .lens(os()),
            1.0,
        )
        .with_flex_child(strat.lens(os()), 1.0)
        .with_flex_child(
            Label::new(|d: &OfflineState, _: &Env| {
                if let Some(s) = &d.strat {
                    if s.fuel_to_save > 0.0 {
                        return format!(
                            "Save {:.2}L total to save a pit stop. Fuel lap target {:.2}L",
                            s.fuel_to_save,
                            s.fuel_target()
                        );
                    }
                }
                "".into()
            })
            .with_text_size(24.0)
            .lens(os()),
            1.0,
        )
}

#[derive(Debug, Clone, Copy)]
struct OfflineStateLens {}

impl Lens<OfflineState, OfflineState> for OfflineStateLens {
    /// Get non-mut access to the field.
    ///
    /// Runs the supplied closure with a reference to the data. It's
    /// structured this way, as opposed to simply returning a reference,
    /// so that the data might be synthesized on-the-fly by the lens.
    fn with<V, F: FnOnce(&OfflineState) -> V>(&self, data: &OfflineState, f: F) -> V {
        f(data)
    }

    /// Get mutable access to the field.
    ///
    /// This method is defined in terms of a closure, rather than simply
    /// yielding a mutable reference, because it is intended to be used
    /// with value-type data (also known as immutable data structures).
    /// For example, a lens for an immutable list might be implemented by
    /// cloning the list, giving the closure mutable access to the clone,
    /// then updating the reference after the closure returns.
    fn with_mut<V, F: FnOnce(&mut OfflineState) -> V>(&self, data: &mut OfflineState, f: F) -> V {
        //println!("with_mut {:?}", data);
        let start = data.clone();
        let res = f(data);
        let mut dirty = false;
        if data.session != start.session {
            data.on_session_change();
            dirty = true;
        }
        if !dirty && *data != start {
            dirty = true;
        }
        if dirty {
            data.recalc();
        }
        res
    }
}

type Options<T> = Vec<Option<T>>;

struct GridWidget<T: Data> {
    cells: Options<WidgetPod<T, Box<dyn Widget<T>>>>,
    cols: usize,
    rows: usize,
    col_widths: Vec<Option<f64>>,
    row_heights: Vec<Option<f64>>,
}
impl<T: Data> GridWidget<T> {
    fn new(cols: usize, rows: usize) -> GridWidget<T> {
        let mut w = GridWidget {
            cols,
            rows,
            cells: Vec::with_capacity(cols * rows),
            col_widths: Vec::with_capacity(cols),
            row_heights: Vec::with_capacity(rows),
        };
        w.cells.resize_with(cols * rows, || None);
        w.col_widths.resize(cols, None);
        w.row_heights.resize(rows, None);
        w
    }
    fn set(&mut self, col: usize, row: usize, cell: impl Widget<T> + 'static) {
        let idx = self.cell_idx(col, row);
        self.cells[idx] = Some(WidgetPod::new(cell).boxed());
    }
    fn set_row_height(&mut self, row: usize, height: f64) {
        self.row_heights[row] = Some(height);
    }
    fn set_col_width(&mut self, col: usize, width: f64) {
        self.col_widths[col] = Some(width);
    }
    fn cell_idx(&self, col: usize, row: usize) -> usize {
        // across, then down
        row * self.cols + col
    }
}

impl<T: Data> Widget<T> for GridWidget<T> {
    fn event(&mut self, ctx: &mut druid::EventCtx, event: &Event, data: &mut T, env: &Env) {
        for cell in self.cells.iter_mut().flatten() {
            cell.event(ctx, event, data, env);
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut druid::LifeCycleCtx,
        event: &druid::LifeCycle,
        data: &T,
        env: &Env,
    ) {
        for cell in self.cells.iter_mut().flatten() {
            cell.lifecycle(ctx, event, data, env);
        }
    }

    fn update(&mut self, ctx: &mut druid::UpdateCtx, _old_data: &T, data: &T, env: &Env) {
        for cell in self.cells.iter_mut().flatten() {
            cell.update(ctx, data, env);
        }
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &T,
        env: &Env,
    ) -> druid::Size {
        let fixed_w: f64 = self.col_widths.iter().flatten().sum();
        let fixed_wc = self.col_widths.iter().flatten().count();
        let fixed_h: f64 = self.row_heights.iter().flatten().sum();
        let fixed_hc = self.row_heights.iter().flatten().count();
        let cell_min = Size::new(
            (bc.min().width - fixed_w) / (self.cols - fixed_wc) as f64,
            (bc.min().height - fixed_h) / (self.rows - fixed_hc) as f64,
        );
        let cell_max = Size::new(
            (bc.max().width - fixed_w) / (self.cols - fixed_wc) as f64,
            (bc.max().height - fixed_h) / (self.rows - fixed_hc) as f64,
        );
        let mut y = 0f64;
        for r in 0..self.rows {
            let mut cell_bc = BoxConstraints::new(cell_min, cell_max);
            if let Some(h) = self.row_heights[r] {
                cell_bc =
                    BoxConstraints::new(Size::new(cell_min.width, h), Size::new(cell_max.width, h));
            }
            let mut max_height = 0f64;
            let mut x = 0f64;
            for c in 0..self.cols {
                let idx = self.cell_idx(c, r);
                let this_bc = match self.col_widths[c] {
                    None => cell_bc,
                    Some(w) => BoxConstraints::new(
                        Size::new(w, cell_bc.min().height),
                        Size::new(w, cell_bc.max().height),
                    ),
                };
                if let Some(w) = &mut self.cells[idx] {
                    let cs = w.layout(ctx, &this_bc, data, env);
                    max_height = f64::max(max_height, cs.height);
                    w.set_origin(ctx, data, env, Point::new(x, y));
                    x += cs.width;
                }
            }
            y += max_height;
        }
        bc.max()
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &T, env: &Env) {
        for cell in self.cells.iter_mut().flatten() {
            cell.paint(ctx, data, env);
        }
    }
}

struct TimerWidget<T: Data, W: Widget<T>, F: FnMut(&mut T)> {
    timer_id: TimerToken,
    widget: W,
    on_fire: F,
    p: PhantomData<T>,
}

impl<T: Data, W: Widget<T>, F: FnMut(&mut T)> Widget<T> for TimerWidget<T, W, F> {
    fn event(&mut self, ctx: &mut druid::EventCtx, event: &druid::Event, data: &mut T, env: &Env) {
        match event {
            Event::WindowConnected => {
                // Start the timer when the application launches
                self.timer_id = ctx.request_timer(TIMER_INTERVAL);
            }
            Event::Timer(id) => {
                if *id == self.timer_id {
                    (self.on_fire)(data);
                    self.timer_id = ctx.request_timer(TIMER_INTERVAL);
                }
            }
            _ => (),
        }
        self.widget.event(ctx, event, data, env);
    }

    fn lifecycle(
        &mut self,
        ctx: &mut druid::LifeCycleCtx,
        event: &druid::LifeCycle,
        data: &T,
        env: &Env,
    ) {
        self.widget.lifecycle(ctx, event, data, env);
    }

    fn update(&mut self, ctx: &mut druid::UpdateCtx, old_data: &T, data: &T, env: &Env) {
        self.widget.update(ctx, old_data, data, env);
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &T,
        env: &Env,
    ) -> druid::Size {
        self.widget.layout(ctx, bc, data, env)
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &T, env: &Env) {
        self.widget.paint(ctx, data, env);
    }
}

/// Converts a `Widget<String>` to a `Widget<Option<T>>`, mapping parse errors to None
/// This a modified version of the druid supplied Parse widget, which has issues when
/// the parse/to_string() can loose characters e.g. for f32 "1.0" -> "1"
struct Parse<T> {
    widget: T,
    state: String,
}

impl<T> Parse<T> {
    /// Create a new `Parse` widget.
    pub fn new(widget: T) -> Self {
        Self {
            widget,
            state: String::new(),
        }
    }
}

impl<T: FromStr + Display + Data, W: Widget<String>> Widget<Option<T>> for Parse<W> {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut Option<T>, env: &Env) {
        self.widget.event(ctx, event, &mut self.state, env);
        *data = self.state.parse().ok();
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &Option<T>,
        env: &Env,
    ) {
        if let LifeCycle::WidgetAdded = event {
            if let Some(data) = data {
                self.state = data.to_string();
            }
        }
        self.widget.lifecycle(ctx, event, &self.state, env)
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &Option<T>, data: &Option<T>, env: &Env) {
        if match (_old_data, data) {
            (None, None) => true,
            (Some(_), None) => false,
            (None, Some(_)) => false,
            (Some(x), Some(y)) => Data::same(x, y),
        } {
            return;
        }
        let old = match *data {
            None => return, // Don't clobber the input
            Some(ref x) => {
                // Its possible that the current self.state already represents the data value
                // in that case we shouldn't clobber the self.state. This helps deal
                // with types where parse()/to_string() round trips can loose information
                // e.g. with floating point numbers, text of "1.0" becomes "1" in the
                // round trip, and this makes it impossible to type in the . otherwise
                match self.state.parse() {
                    Err(_) => Some(mem::replace(&mut self.state, x.to_string())),
                    Ok(v) => {
                        if !Data::same(&v, x) {
                            Some(mem::replace(&mut self.state, x.to_string()))
                        } else {
                            None
                        }
                    }
                }
            }
        };
        // if old is None here, that means that self.state hasn't changed
        let old_data = old.as_ref().unwrap_or(&self.state);
        self.widget.update(ctx, old_data, &self.state, env)
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        _data: &Option<T>,
        env: &Env,
    ) -> Size {
        self.widget.layout(ctx, bc, &self.state, env)
    }

    fn paint(&mut self, ctx: &mut PaintCtx, _data: &Option<T>, env: &Env) {
        self.widget.paint(ctx, &self.state, env)
    }

    fn id(&self) -> Option<WidgetId> {
        self.widget.id()
    }

    fn debug_state(&self, _data: &Option<T>) -> DebugState {
        DebugState {
            display_name: "Parse".to_string(),
            main_value: self.state.clone(),
            ..Default::default()
        }
    }
}
