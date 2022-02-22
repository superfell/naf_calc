use druid::widget::{Align, Container, Flex, Label, LabelText, SizedBox};
use druid::{
    AppLauncher, Color, Data, Env, Event, FontDescriptor, FontFamily, FontWeight, Key, UnitPoint,
    Widget, WidgetExt, WindowDesc,
};
use druid::{LensExt, TimerToken};
use ircalc::{ADuration, AmountLeft, State};
use std::time::Duration;

mod calc;
mod ir;
mod ircalc;
mod strat;

static TIMER_INTERVAL: Duration = Duration::from_millis(100);

fn main() {
    // describe the main window
    let main_window = WindowDesc::new(build_root_widget)
        .title("naf calc")
        .window_size((625.0, 480.0));

    // create the initial app state
    let initial_state = ircalc::State::default();

    // start the application
    AppLauncher::with_window(main_window)
        .launch(initial_state)
        .expect("Failed to launch application");
}

fn lbl(l: &str, align: UnitPoint, w: f64, h: f64) -> Container<State> {
    Container::new(
        SizedBox::new(Align::new(align, Label::new(l).with_text_size(32.0)))
            .width(w)
            .height(h),
    )
}
fn val<T: Data>(align: UnitPoint, w: f64, h: f64, text: impl Into<LabelText<T>>) -> Container<T> {
    let font = FontDescriptor::new(FontFamily::SYSTEM_UI)
        .with_weight(FontWeight::BOLD)
        .with_size(38.0);
    Container::new(
        SizedBox::new(Align::new(align, Label::<T>::new(text).with_font(font)))
            .width(w)
            .height(h),
    )
}

const COLOR_KEY: Key<Color> = Key::new("color-key");

fn build_root_widget() -> impl Widget<State> {
    const R0_HEIGHT: f64 = 60.0;
    const R_HEIGHT: f64 = 75.0;
    const C0_WIDTH: f64 = 125.0;
    const C_WIDTH: f64 = (625.0 - C0_WIDTH) / 3.0;

    let fmt_f32 = |f: &f32, _e: &Env| format!("{:.2}", f);
    let fmt_i32 = |f: &i32, _e: &Env| format!("{:2}", f);
    let fmt_ps = |f: &Option<strat::Pitstop>, _e: &Env| match f {
        None => "".to_string(),
        Some(ps) => {
            if ps.is_open() {
                format!("OPEN {} Laps", ps.close)
            } else {
                format!("{}-{} Laps", ps.open, ps.close)
            }
        }
    };
    let fmt_tm = |f: &ADuration, _e: &Env| format!("{}", f);
    let mut calc = ircalc::IrCalc::new();

    TimerWidget {
        on_fire: move |d: &mut State| calc.update(d),
        timer_id: TimerToken::INVALID,
        widget: Flex::row()
            .must_fill_main_axis(true)
            .with_child(
                Flex::column()
                    .with_spacer(R0_HEIGHT)
                    .with_child(lbl("Car", UnitPoint::LEFT, C0_WIDTH, R_HEIGHT))
                    .with_child(lbl("Race", UnitPoint::LEFT, C0_WIDTH, R_HEIGHT))
                    .with_child(
                        lbl("Last Lap", UnitPoint::LEFT, C0_WIDTH, R_HEIGHT)
                            .background(Color::BLUE),
                    )
                    .with_child(lbl("Average", UnitPoint::LEFT, C0_WIDTH, R_HEIGHT))
                    .with_child(lbl("Pit", UnitPoint::LEFT, C0_WIDTH, R_HEIGHT)),
            )
            .with_flex_child(
                Flex::column()
                    .with_child(lbl("Fuel", UnitPoint::CENTER, C_WIDTH, R0_HEIGHT))
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32)
                            .background(COLOR_KEY)
                            .env_scope(|env, data| {
                                env.set(
                                    COLOR_KEY,
                                    if *data < 0.0 {
                                        Color::RED
                                    } else {
                                        Color::GREEN
                                    },
                                )
                            })
                            .lens(State::car.then(AmountLeft::fuel)),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32)
                            .lens(State::race.then(AmountLeft::fuel)),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32)
                            .lens(State::fuel_last_lap),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32).lens(State::fuel_avg),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_ps)
                            .background(Color::GRAY)
                            .lens(State::next_stop),
                    )
                    .expand_width(),
                1.0,
            )
            .with_flex_child(
                Flex::column()
                    .with_child(lbl("Laps", UnitPoint::CENTER, C_WIDTH, R0_HEIGHT))
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_i32)
                            .lens(State::car.then(AmountLeft::laps)),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_i32)
                            .lens(State::race.then(AmountLeft::laps)),
                    )
                    .with_child(
                        lbl("Save", UnitPoint::RIGHT, C_WIDTH, R_HEIGHT).background(Color::GRAY),
                    )
                    .with_spacer(R_HEIGHT)
                    .with_child(lbl("Stops", UnitPoint::RIGHT, C_WIDTH, R_HEIGHT)),
                1.0,
            )
            .with_flex_child(
                Flex::column()
                    .with_child(lbl("Time", UnitPoint::CENTER, C_WIDTH, R0_HEIGHT))
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_tm)
                            .lens(State::car.then(AmountLeft::time)),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_tm)
                            .lens(State::race.then(AmountLeft::time)),
                    )
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32).lens(State::save),
                    )
                    .with_spacer(R_HEIGHT)
                    .with_child(
                        val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_i32).lens(State::stops),
                    ),
                1.0,
            ),
    }
}

struct TimerWidget<T, F: FnMut(&mut T)> {
    timer_id: TimerToken,
    widget: Flex<T>,
    on_fire: F,
}

impl<T: Data, F: FnMut(&mut T)> Widget<T> for TimerWidget<T, F> {
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
