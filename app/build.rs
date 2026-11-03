#[cfg(target_os = "windows")]
fn main() {
    let icon = "resources/win/nightfall.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::path::Path::new(icon).exists() {
        winres::WindowsResource::new()
            .set_icon_with_id(icon, "120")
            .set("FileDescription", "NIGHTFALL Browser")
            .set("ProductName", "NIGHTFALL")
            .compile()
            .expect("Windows resources");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {}
