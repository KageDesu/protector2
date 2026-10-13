//! Detection and validation of a *deployed* RPG Maker project (MV or MZ).
//!
//! Key goals:
//! * Accept only a deployed/release build, never a working (editor) project —
//!   running the protector on a working project would destroy the developer's
//!   source. The decisive signal is the presence of an editor project file
//!   (`*.rmmzproject` / `*.rpgproject`), which deployment strips out.
//! * Auto-detect the engine and locate the `js/` base (project root for MZ,
//!   the `www/` subfolder for MV).
//! * Resolve a human-readable game title for display.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    MZ,
    MV,
}

impl Engine {
    pub fn as_str(&self) -> &'static str {
        match self {
            Engine::MZ => "RPG Maker MZ",
            Engine::MV => "RPG Maker MV",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RmProject {
    /// The folder the user dropped (the deployment root).
    pub root: PathBuf,
    /// Where the `js/` folder lives: `root` for MZ, `root/www` for MV.
    pub js_base: PathBuf,
    pub engine: Engine,
    pub title: String,
    /// True if an NW.js desktop runtime ships with this build (Game.exe /
    /// nw.dll / Game.app). Encrypted protection needs Node's `crypto`, which
    /// only exists there — not in web or mobile deployments.
    pub nwjs: bool,
}

impl RmProject {
    pub fn js_dir(&self) -> PathBuf {
        self.js_base.join("js")
    }
    pub fn plugins_js(&self) -> PathBuf {
        self.js_dir().join("plugins.js")
    }
    pub fn plugins_dir(&self) -> PathBuf {
        self.js_dir().join("plugins")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DetectError {
    /// Looks like a working/editor project (has a *.rmmzproject / *.rpgproject).
    WorkingProject,
    /// No js/plugins.js found in root or www — not an RPG Maker deployment.
    NotRmProject,
    /// Path does not exist or is not a directory.
    NotADirectory,
}

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectError::WorkingProject => write!(
                f,
                "This looks like a working (editor) project, not a deployed build. \
                 Deploy your game first (File > Deployment) and drop the deployed copy. \
                 Running on a working project would destroy your plugin sources."
            ),
            DetectError::NotRmProject => write!(
                f,
                "No js/plugins.js found. Drop a deployed RPG Maker MV or MZ project folder."
            ),
            DetectError::NotADirectory => write!(f, "The dropped item is not a folder."),
        }
    }
}
impl std::error::Error for DetectError {}

fn has_editor_project_file(root: &Path) -> bool {
    let Ok(rd) = fs::read_dir(root) else {
        return false;
    };
    for entry in rd.flatten() {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            let ext = ext.to_ascii_lowercase();
            // MZ: Game.rmmzproject, MV: Game.rpgproject / Game.rmmvproject
            if ext == "rmmzproject" || ext == "rpgproject" || ext == "rmmvproject" {
                return true;
            }
        }
    }
    false
}

/// Detect and validate a deployed project rooted at `dropped`.
pub fn detect(dropped: &Path) -> Result<RmProject, DetectError> {
    if !dropped.is_dir() {
        return Err(DetectError::NotADirectory);
    }

    // Guard: refuse working projects outright.
    if has_editor_project_file(dropped) {
        return Err(DetectError::WorkingProject);
    }

    // MV always nests under www/. MZ keeps js/ at the root.
    let www_plugins = dropped.join("www").join("js").join("plugins.js");
    let root_plugins = dropped.join("js").join("plugins.js");

    let (js_base, engine) = if www_plugins.is_file() {
        (dropped.join("www"), Engine::MV)
    } else if root_plugins.is_file() {
        // Distinguish MZ vs MV-at-root by core files (MV deployment normally uses
        // www, but be defensive). rmmz_core.js => MZ, rpg_core.js => MV.
        let js = dropped.join("js");
        let engine = if js.join("rpg_core.js").is_file() {
            Engine::MV
        } else {
            Engine::MZ
        };
        (dropped.to_path_buf(), engine)
    } else {
        return Err(DetectError::NotRmProject);
    };

    let title = resolve_title(dropped, &js_base);
    let nwjs = ["Game.exe", "nw.dll", "nw.exe", "Game.app", "nwjs.app"]
        .iter()
        .any(|f| dropped.join(f).exists());

    Ok(RmProject {
        root: dropped.to_path_buf(),
        js_base,
        engine,
        title,
        nwjs,
    })
}

/// Best-effort game title: package.json window.title -> index.html <title>
/// -> data/System.json gameTitle -> folder name.
fn resolve_title(root: &Path, js_base: &Path) -> String {
    if let Some(t) = title_from_package_json(&root.join("package.json")) {
        return t;
    }
    // MV deployments also carry a package.json in www sometimes.
    if let Some(t) = title_from_package_json(&js_base.join("package.json")) {
        return t;
    }
    if let Some(t) = title_from_index_html(&js_base.join("index.html")) {
        return t;
    }
    if let Some(t) = title_from_system_json(&js_base.join("data").join("System.json")) {
        return t;
    }
    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown project")
        .to_string()
}

fn title_from_package_json(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("window")
        .and_then(|w| w.get("title"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

fn title_from_index_html(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let lower = text.to_ascii_lowercase();
    let open = lower.find("<title>")?;
    let close = lower[open..].find("</title>")? + open;
    let inner = text[open + "<title>".len()..close].trim().to_string();
    if inner.is_empty() {
        None
    } else {
        Some(inner)
    }
}

fn title_from_system_json(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("gameTitle")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, content: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn detects_mz_deployment_and_title() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("js").join("plugins.js"), "var $plugins =\n[];\n");
        write(&root.join("js").join("rmmz_core.js"), "//core");
        write(
            &root.join("package.json"),
            r#"{"window":{"title":"Vortex"}}"#,
        );
        fs::write(root.join("Game.exe"), b"MZ").unwrap();

        let proj = detect(root).unwrap();
        assert_eq!(proj.engine, Engine::MZ);
        assert_eq!(proj.title, "Vortex");
        assert!(proj.nwjs, "Game.exe present => NW.js build");
        assert_eq!(proj.js_base, root.to_path_buf());
        assert!(proj.plugins_js().is_file());
    }

    #[test]
    fn detects_mv_deployment_under_www() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("www").join("js").join("plugins.js"), "var $plugins =\n[];\n");
        write(&root.join("www").join("js").join("rpg_core.js"), "//core");
        write(
            &root.join("www").join("index.html"),
            "<html><head><title>My MV Game</title></head></html>",
        );

        let proj = detect(root).unwrap();
        assert_eq!(proj.engine, Engine::MV);
        assert!(!proj.nwjs, "no runtime files => web/mobile build");
        assert_eq!(proj.title, "My MV Game");
        assert_eq!(proj.js_base, root.join("www"));
    }

    #[test]
    fn rejects_working_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("js").join("plugins.js"), "var $plugins =\n[];\n");
        fs::write(root.join("Game.rmmzproject"), b"RPGMZ 1.0.0").unwrap();

        assert!(matches!(detect(root), Err(DetectError::WorkingProject)));
    }

    #[test]
    fn rejects_non_project() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(detect(tmp.path()), Err(DetectError::NotRmProject)));
    }

    #[test]
    fn title_falls_back_to_system_json_then_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("CoolGame");
        write(&root.join("js").join("plugins.js"), "var $plugins =\n[];\n");
        write(&root.join("js").join("rmmz_core.js"), "//core");
        write(
            &root.join("data").join("System.json"),
            r#"{"gameTitle":"From System"}"#,
        );
        let proj = detect(&root).unwrap();
        assert_eq!(proj.title, "From System");
    }
}
