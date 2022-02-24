use druid::widget::{Align, Label, LabelText, SizedBox};
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, FontDescriptor, FontFamily, FontWeight,
    Insets, Key, Point, Size, UnitPoint, Widget, WidgetExt, WidgetPod, WindowDesc,
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

fn lbl(l: &str, align: UnitPoint) -> impl Widget<Estimation> {
    SizedBox::new(Align::new(
        align,
        Label::new(l)
            .with_text_size(32.0)
            .with_text_color(Color::grey8(200)),
    ))
}
fn val<T: Data>(text: impl Into<LabelText<T>>) -> impl Widget<T> {
    let font = FontDescriptor::new(FontFamily::SYSTEM_UI)
        .with_weight(FontWeight::BOLD)
        .with_size(56.0);
    Align::new(UnitPoint::CENTER, Label::<T>::new(text).with_font(font))
}

const COLOR_KEY: Key<Color> = Key::new("color-key");

fn build_root_widget() -> impl Widget<Estimation> {
    const GRID: Color = Color::GRAY;
    const GWIDTH: f64 = 1.0;
    let mut w = GridWidget::new(4, 7);
    w.set_col_width(0, 150.0);
    w.set_row_height(0, 45.0);
    w.set_row_height(3, 15.0);
    for (r, s) in ["", "Car", "Race", "", "Last Lap", "Average", "Pit"]
        .iter()
        .enumerate()
    {
        if !s.is_empty() {
            w.set(
                0,
                r,
                lbl(s, UnitPoint::LEFT)
                    .padding(Insets::new(6.0, 0.0, 0.0, 0.0))
                    .border(GRID, GWIDTH),
            );
        } else {
            w.set(0, r, SizedBox::empty().width(10.0).height(10.0));
        }
    }

    for (i, s) in ["Fuel", "Laps", "Time"].iter().enumerate() {
        w.set(i + 1, 0, lbl(s, UnitPoint::CENTER).border(GRID, GWIDTH));
    }
    let fmt_f32 = |f: &f32, _e: &Env| format!("{:.2}", f);
    let fmt_i32 = |f: &i32, _e: &Env| format!("{:}", f);
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
        val(fmt_f32)
            .lens(Estimation::car.then(AmountLeft::fuel))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        1,
        val(fmt_i32)
            .lens(Estimation::car.then(AmountLeft::laps))
            .border(GRID, GWIDTH),
    );
    w.set(3, 1, val(fmt_tm).lens(Estimation::car).border(GRID, GWIDTH));
    w.set(
        1,
        2,
        val(fmt_f32)
            .lens(Estimation::race.then(AmountLeft::fuel))
            .border(GRID, GWIDTH),
    );
    w.set(
        2,
        2,
        val(fmt_i32)
            .lens(Estimation::race.then(AmountLeft::laps))
            .border(GRID, GWIDTH),
    );
    w.set(
        3,
        2,
        val(fmt_tm).lens(Estimation::race).border(GRID, GWIDTH),
    );
    w.set(
        1,
        4,
        val(fmt_f32)
            .lens(Estimation::fuel_last_lap)
            .border(GRID, GWIDTH),
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
        val(fmt_f32).lens(Estimation::save).border(GRID, GWIDTH),
    );
    w.set(
        1,
        5,
        val(fmt_f32)
            .lens(Estimation::green.then(Rate::fuel))
            .border(GRID, GWIDTH),
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
        val(fmt_f32)
            .lens(Estimation::save_target)
            .border(GRID, GWIDTH),
    );
    w.set(
        1,
        6,
        val(fmt_ps).lens(Estimation::next_stop).border(GRID, GWIDTH),
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
        val(fmt_i32).lens(Estimation::stops).border(GRID, GWIDTH),
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

struct GridWidget<T: Data> {
    cells: Vec<Option<WidgetPod<T, Box<dyn Widget<T>>>>>,
    cols: usize,
    rows: usize,
    col_widths: Vec<Option<f64>>,
    row_heights: Vec<Option<f64>>,
}
impl<T: Data> GridWidget<T> {
    fn new(cols: usize, rows: usize) -> GridWidget<T> {
        let mut w = GridWidget {
            cols: cols,
            rows: rows,
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
