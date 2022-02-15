mod ir;

fn main() {
    println!("Hello, world!");

    let mut c = ir::Client::new();
    let ready = c.wait_for_data(std::time::Duration::new(1, 0));
    if ready {
        let v_session_time = c.find_var("SessionTime").unwrap();
        let v_fuel_level = c.find_var("FuelLevel").unwrap();
        let v_lap_progress = c.find_var("LapDistPct").unwrap();

        let mut last_session_time = c.double(&v_session_time).unwrap();
        let mut last_fuel_level = c.float(&v_fuel_level).unwrap();
        let mut last_lap_progress = c.float(&v_lap_progress).unwrap();
        println!("starting!");
        loop {
            let ready = c.wait_for_data(std::time::Duration::new(0, 16 * 1000 * 1000));
            if ready {
                let session_time = c.double(&v_session_time).unwrap();
                if session_time < last_session_time {
                    // reset
                }
                let lap_progress = c.float(&v_lap_progress).unwrap();

                if lap_progress < 0.1 && last_lap_progress > 0.9 {
                    let fuel = c.float(&v_fuel_level).unwrap();
                    println!("fuel used {:?} left:{:?}", last_fuel_level - fuel, fuel);
                    last_fuel_level = fuel;
                }
                last_session_time = session_time;
                last_lap_progress = lap_progress;
            }
            //println!("{}", c.session_info());
        }
    }
    c.close();
}
