use druid::widget::{Align, Container, Label, LabelText, SizedBox};
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, FontDescriptor, FontFamily, FontWeight,
    Key, Point, UnitPoint, Widget, WidgetExt, WidgetPod, WindowDesc,
};
use druid::{LensExt, TimerToken};
use ircalc::{AmountLeft, Estimation};
use std::marker::PhantomData;
use std::time::Duration;
use strat::Rate;

mod calc;
mod ir;
mod ircalc;
mod strat;

static TIMER_INTERVAL: Duration = Duration::from_millis(100);

fn main() {
    // describe the main window
    let main_window = WindowDesc::new(build_root_widget)
        .title("naf calc")
        .window_size((900.0, 480.0));

    // create the initial app state
    let initial_state = ircalc::Estimation::default();

    // start the application
    AppLauncher::with_window(main_window)
        .launch(initial_state)
        .expect("Failed to launch application");
}

fn lbl(l: &str, align: UnitPoint, w: f64, h: f64) -> Container<Estimation> {
    Container::new(
        SizedBox::new(Align::new(align, Label::new(l).with_text_size(32.0)))
            .width(w)
            .height(h),
    )
}
fn val<T: Data>(align: UnitPoint, w: f64, h: f64, text: impl Into<LabelText<T>>) -> Container<T> {
    let font = FontDescriptor::new(FontFamily::SYSTEM_UI)
        .with_weight(FontWeight::BOLD)
        .with_size(48.0);
    Container::new(
        SizedBox::new(Align::new(align, Label::<T>::new(text).with_font(font)))
            .width(w)
            .height(h),
    )
}

const COLOR_KEY: Key<Color> = Key::new("color-key");

fn build_root_widget() -> impl Widget<Estimation> {
    const GRID: Color = Color::GRAY;
    const GWIDTH: f64 = 1.0;
    let mut w = GridWidget::new(4, 6);
    let mut r = 0;
    for s in ["", "Car", "Race", "Last Lap", "Average", "Pit"] {
        w.set(
            0,
            r,
            lbl(s, UnitPoint::LEFT, 120.0, 40.0).border(GRID, GWIDTH),
        );
        r += 1;
    }

    for (i, s) in ["Fuel", "Laps", "Time"].iter().enumerate() {
        w.set(
            i + 1,
            0,
            lbl(s, UnitPoint::CENTER, 40.0, 40.0).border(GRID, GWIDTH),
        );
    }
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
    let fmt_tm = |f: &AmountLeft, _e: &Env| {
        format!("{:02}:{:02}", f.time.as_secs() / 60, f.time.as_secs() % 60)
    };
    w.set(
        1,
        1,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::car.then(AmountLeft::fuel))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        1,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_i32)
            .lens(Estimation::car.then(AmountLeft::laps))
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        1,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_tm)
            .lens(Estimation::car)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        2,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::race.then(AmountLeft::fuel))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        2,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_i32)
            .lens(Estimation::race.then(AmountLeft::laps))
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        2,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_tm)
            .lens(Estimation::race)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        3,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::fuel_last_lap)
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        3,
        lbl("Save", UnitPoint::RIGHT, 40.0, 40.0).border(GRID, GWIDTH),
    );
    w.set(
        3,
        3,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::save)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        4,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::green.then(Rate::fuel))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        4,
        lbl("Target", UnitPoint::RIGHT, 40.0, 40.0).border(GRID, GWIDTH),
    );
    w.set(
        3,
        4,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_f32)
            .lens(Estimation::save_target)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        5,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_ps)
            .lens(Estimation::next_stop)
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        5,
        lbl("Stops", UnitPoint::RIGHT, 40.0, 40.0).border(GRID, GWIDTH),
    );
    w.set(
        3,
        5,
        val(UnitPoint::CENTER, 40.0, 40.0, fmt_i32)
            .lens(Estimation::stops)
            .border(GRID, GWIDTH),
    );

    let mut calc = ircalc::Estimator::new();
    TimerWidget {
        on_fire: move |d| calc.update(d),
        timer_id: TimerToken::INVALID,
        widget: w,
        p: PhantomData,
    }
}

//     val(UnitPoint::CENTER, C_WIDTH, R_HEIGHT, fmt_f32)
//         .background(COLOR_KEY)
//         .env_scope(|env, data| {
//             env.set(
//                 COLOR_KEY,
//                 if *data < 1.0 {
//                     Color::RED
//                 } else {
//                     Color::GREEN
//                 },
//             )
//         })
//         .lens(Estimation::car.then(AmountLeft::fuel)),
// )

struct GridWidget<T: Data, CW: Widget<T>> {
    cells: Vec<Option<WidgetPod<T, CW>>>,
    cols: usize,
    rows: usize,
    p: PhantomData<T>,
}
impl<T: Data, CW: Widget<T>> GridWidget<T, CW> {
    fn new(cols: usize, rows: usize) -> GridWidget<T, CW> {
        let mut w = GridWidget {
            cols: cols,
            rows: rows,
            cells: Vec::with_capacity(cols * rows),
            p: PhantomData,
        };
        for _i in 0..(cols * rows) {
            w.cells.push(None);
        }
        w
    }
    fn set(&mut self, col: usize, row: usize, cell: CW) {
        let idx = self.cell_idx(col, row);
        self.cells[idx] = Some(WidgetPod::new(cell));
    }
    fn cell_idx(&self, col: usize, row: usize) -> usize {
        // across, then down
        row * self.cols + col
    }
}

impl<T: Data, CW: Widget<T>> Widget<T> for GridWidget<T, CW> {
    fn event(&mut self, ctx: &mut druid::EventCtx, event: &Event, data: &mut T, env: &Env) {
        for c in &mut self.cells {
            if let Some(cell) = c {
                cell.event(ctx, event, data, env);
            }
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut druid::LifeCycleCtx,
        event: &druid::LifeCycle,
        data: &T,
        env: &Env,
    ) {
        for c in &mut self.cells {
            if let Some(cell) = c {
                cell.lifecycle(ctx, event, data, env);
            }
        }
    }

    fn update(&mut self, ctx: &mut druid::UpdateCtx, _old_data: &T, data: &T, env: &Env) {
        for c in &mut self.cells {
            if let Some(cell) = c {
                cell.update(ctx, data, env);
            }
        }
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &T,
        env: &Env,
    ) -> druid::Size {
        let cell_min = druid::Size::new(
            bc.min().width / self.cols as f64,
            bc.min().height / self.rows as f64,
        );
        let cell_max = druid::Size::new(
            bc.max().width / self.cols as f64,
            bc.max().height / self.rows as f64,
        );
        let cell_bc = BoxConstraints::new(cell_min, cell_max);
        let mut y = 0f64;
        for r in 0..self.rows {
            let mut max_height = 0f64;
            let mut x = 0f64;
            for c in 0..self.cols {
                let idx = self.cell_idx(c, r);
                if let Some(w) = &mut self.cells[idx] {
                    let cs = w.layout(ctx, &cell_bc, data, env);
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
        for c in &mut self.cells {
            if let Some(cell) = c {
                cell.paint(ctx, data, env);
            }
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
