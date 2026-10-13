//! protector_core — platform-independent logic for PKD Protector v2.
//!
//! Split out from the GUI so it can be unit-tested on any host. The GUI (`app`
//! crate) is a thin eframe shell over this.

pub mod encrypt;
pub mod minify;
pub mod plugins;
pub mod project;
pub mod protect;
pub mod util;

pub use plugins::{enabled_in_order, parse_plugins_js, PluginEntry};
pub use project::{detect, DetectError, Engine, RmProject};
pub use protect::{protect, ProtectError, ProtectOptions, ProtectReport, ProtectionLevel};

/// One enabled plugin as shown in the UI list.
#[derive(Debug, Clone)]
pub struct PluginRow {
    pub file_name: String,
    pub bytes: u64,
    /// Already obfuscated (e.g. VisuMZ) — merged byte-for-byte, never minified.
    pub obfuscated: bool,
    /// Enabled in plugins.js but the file does not exist.
    pub missing: bool,
}

/// A high-level summary of a validated project, ready for the UI to display.
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub title: String,
    pub engine: Engine,
    pub nwjs: bool,
    pub enabled_plugins: usize,
    pub total_plugins: usize,
    pub plugins: Vec<PluginRow>,
}

/// Validate a dropped folder and gather display info in one call.
/// Refuses projects that are already protected.
pub fn inspect(path: &std::path::Path) -> Result<(RmProject, ProjectInfo), String> {
    let proj = detect(path).map_err(|e| e.to_string())?;
    let content = std::fs::read_to_string(proj.plugins_js())
        .map_err(|e| format!("Cannot read plugins.js: {e}"))?;
    let entries = parse_plugins_js(&content).map_err(|e| e.to_string())?;
    let enabled = enabled_in_order(&entries);

    let names: Vec<String> = enabled.iter().map(|p| p.file_name()).collect();
    if protect::is_already_protected(&proj.plugins_dir(), &names) {
        return Err(ProtectError::AlreadyProtected.to_string());
    }

    let plugins = names
        .iter()
        .map(|name| {
            let path = proj.plugins_dir().join(name);
            match std::fs::read_to_string(&path) {
                Ok(src) => PluginRow {
                    file_name: name.clone(),
                    bytes: src.len() as u64,
                    obfuscated: protect::looks_obfuscated(&src),
                    missing: false,
                },
                Err(_) => PluginRow {
                    file_name: name.clone(),
                    bytes: 0,
                    obfuscated: false,
                    missing: true,
                },
            }
        })
        .collect();

    let info = ProjectInfo {
        title: proj.title.clone(),
        engine: proj.engine,
        nwjs: proj.nwjs,
        enabled_plugins: enabled.len(),
        total_plugins: entries.len(),
        plugins,
    };
    Ok((proj, info))
}
