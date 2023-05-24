use std::env;
use std::process;

use tab_o_txt::editor::Editor;

fn main() {
    let args: Vec<_> = env::args().collect();
    let mut session = Editor::from(&args).unwrap_or_else(|err| {
        println!("Error when starting editor: {}", err);
        process::exit(1);
    });

    session.run().unwrap_or_else(|err| {
        println!("Rumtime error: {}", err);
        process::exit(1);
    });
}
