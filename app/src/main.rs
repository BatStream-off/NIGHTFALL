#![cfg_attr(
    all(not(debug_assertions), not(feature = "sandbox"), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod nf;

#[cfg(not(all(feature = "sandbox", target_os = "windows")))]
fn main() -> Result<(), &'static str> {
    let _library = nf::load_cef();
    let args = cef::args::Args::new();
    let Some(cmd_line) = args.as_cmd_line() else {
        return Err("Failed to parse command line arguments");
    };
    nf::run_main(args.as_main_args(), &cmd_line, std::ptr::null_mut());
    Ok(())
}

#[cfg(all(feature = "sandbox", target_os = "windows"))]
fn main() -> Result<(), &'static str> {
    Err("Sur Windows avec la sandbox, lancez nightfall.exe (bootstrap) depuis le dossier de l'application.")
}
