use std::env;
use std::process;

use tab_o_txt::editor::{Config, Editor};

fn main() {
    let config = Config::new();

    let args: Vec<_> = env::args().collect();
    let mut session = Editor::from(config, &args).unwrap_or_else(|err| {
        println!("Error when starting editor: {}", err);
        process::exit(1);
    });

    session.run().unwrap_or_else(|err| {
        println!("Rumtime error: {}", err);
        process::exit(1);
    });
}
