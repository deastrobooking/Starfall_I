use bevy::app::AppExit;
use starfall_i::render_lab::{run, LabConfig, USAGE};

fn main() -> AppExit {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return AppExit::Success;
    }
    match LabConfig::parse(args) {
        Ok(config) => run(config),
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            AppExit::error()
        }
    }
}
