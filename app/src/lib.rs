//! Windows sandbox entry point: CEF's `bootstrap.exe` (renamed nightfall.exe)
//! loads `nightfall.dll` and calls `RunWinMain` for every process type.

#[cfg(all(target_os = "windows", feature = "sandbox"))]
#[allow(dead_code)]
mod nf;

#[cfg(all(target_os = "windows", feature = "sandbox"))]
#[unsafe(no_mangle)]
unsafe extern "C" fn RunWinMain(
    instance: cef::sys::HINSTANCE,
    _command_line: *const u8,
    _command_show: i32,
    sandbox_info: *mut u8,
) -> i32 {
    let _library = nf::load_cef();
    let main_args = cef::MainArgs { instance };
    let args = cef::args::Args::from(main_args);
    let Some(cmd_line) = args.as_cmd_line() else {
        return 1;
    };
    nf::run_main(args.as_main_args(), &cmd_line, sandbox_info);
    0
}
