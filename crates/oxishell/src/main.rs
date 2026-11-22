#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        if let Err(e) = ui::run() {
            eprintln!("erro ao abrir a interface: {e}");
            std::process::exit(1);
        }
    } else {
        std::process::exit(cli::run(&args));
    }
}
