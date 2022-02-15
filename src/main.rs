mod ir;

fn main() {
    println!("Hello, world!");

    let mut c = ir::Client::new();
    if c.get_new_data() {
        {
            let t = c.find_var("AirTemp");
            if let Some(x) = t {
                let n = x.name();
                println!("{}: {:?}: {}", n, x.var_type(), x.float().unwrap());
            } else {
                println!("didn't find var AirTemp");
            }
        }
        let f = c.find_var("FuelLevel");
        if let Some(x) = f {
            println!("{} {}", x.name(), x.float().unwrap());
        }
        //println!("{}", c.session_info());
    }
    c.close();
}
