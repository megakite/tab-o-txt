use std::env;
use std::process;

use tab_o_txt::Config;
use tab_o_txt::Session;

fn main() {
    let vars: Vec<_> = env::vars().collect();
    let config = Config::build(&vars).unwrap_or_else(|err| {
        println!("Problem parsing environment variables: {}", err);
        process::exit(1);
    });

    let args: Vec<_> = env::args().collect();
    let mut session = Session::new(config, &args).unwrap_or_else(|err| {
        println!("Error when creating session: {}", err);
        process::exit(1);
    });

    session.run().unwrap_or_else(|err| {
        println!("Rumtime error: {}", err);
        process::exit(1);
    })
}
