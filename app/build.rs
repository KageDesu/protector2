// Embeds the app icon and version info into the Windows .exe.
// Best-effort: if the resource compiler is unavailable, the build still
// succeeds (only the .exe icon is missing; the window icon is set at runtime).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "PKD Protector");
        res.set("FileDescription", "PKD Protector v2 — RPG Maker plugin protection");
        res.set("CompanyName", "KageDesu Workshop");
        res.set("LegalCopyright", "Pheonix KageDesu");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Icon/version resource not embedded: {e}");
        }
    }
}
