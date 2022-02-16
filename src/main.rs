mod calc;
mod ir;
mod strat;

#[derive(Clone, Copy, Debug)]
struct CarState {
    session_time: f64,
    fuel_level: f32,
    lap_progress: f32,
}
struct CarStateFactory {
    session_time: ir::Var,
    fuel_level: ir::Var,
    lap_progress: ir::Var,
}
impl CarStateFactory {
    fn new(c: &ir::Client) -> CarStateFactory {
        CarStateFactory {
            session_time: c.find_var("SessionTime").unwrap(),
            fuel_level: c.find_var("FuelLevel").unwrap(),
            lap_progress: c.find_var("LapDistPct").unwrap(),
        }
    }
    fn read(&self, c: &ir::Client) -> CarState {
        CarState {
            session_time: c.double(&self.session_time).unwrap(),
            fuel_level: c.float(&self.fuel_level).unwrap(),
            lap_progress: c.float(&self.lap_progress).unwrap(),
        }
    }
}

fn main() {
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
        'session: loop {
            println!("new session");
            let f = CarStateFactory::new(&c);
            let mut last = f.read(&c);
            let mut lap_start = last;
            let calc_interval = std::time::Duration::new(0, 16 * 1000 * 1000);
            loop {
                let ready = c.wait_for_data(calc_interval);
                if ready {
                    let this = f.read(&c);
                    if this.session_time < last.session_time {
                        // reset
                        continue 'session;
                    }
                    if this.lap_progress < 0.1 && last.lap_progress > 0.9 {
                        let fuel_used = lap_start.fuel_level - this.fuel_level;
                        println!("fuel used {:?} left:{:?}", fuel_used, this.fuel_level);
                        lap_start = this;
                    }
                    last = this;
                } else if !c.connected() {
                    println!("no longer connected to iracing");
                    continue 'main;
                }
            }
        }
    }
}
