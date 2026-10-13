//! The protection operation itself.
//!
//! Level 1 (this module's `merge`): concatenate the source of every enabled
//! plugin, in load order, into the first enabled plugin file (the "master"),
//! then blank the rest. `plugins.js` is never touched, so plugin *parameters*
//! (which the engine reads from `$plugins` there) keep working.
//!
//! What this fixes vs the v1 tool:
//! * Annotation blocks (`/*: ... */`, `/*~struct~ ... */`) are stripped
//!   correctly even when a license `/* ... */` comment precedes them — v1's
//!   first-`*/` scan removed the wrong region (or nothing).
//! * A `;` separator is inserted between concatenated files, so a file ending
//!   in `})()` without a semicolon can't accidentally fuse with the next file's
//!   opening `(` into a single call expression.
//! * Plugins that read `document.currentScript` are detected and reported,
//!   because after merging their script name becomes the master file's name.

use std::fs;
use std::path::{Path, PathBuf};

use crate::plugins::{enabled_in_order, parse_plugins_js};
use crate::project::RmProject;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionLevel {
    /// Merge all enabled plugins into the master file and blank the rest.
    Merge,
    /// Merge, then ship the bundle as an AES-256-GCM encrypted blob + loader.
    Encrypt,
}

#[derive(Debug, Clone)]
pub struct ProtectOptions {
    pub level: ProtectionLevel,
    /// Strip comments/whitespace from each readable plugin before merging.
    /// Already-obfuscated plugins are always kept as-is.
    pub minify: bool,
    pub backup: bool,
}

impl Default for ProtectOptions {
    fn default() -> Self {
        Self {
            level: ProtectionLevel::Merge,
            minify: true,
            backup: true,
        }
    }
}

/// First line written into every protected master file. Used to detect a
/// project that was already protected (re-running would destroy the code,
/// because the merged-away plugin files are already empty).
pub const PROTECTED_MARKER: &str = "/* Protected by PKD Protector */";

#[derive(Debug, Default, Clone)]
pub struct ProtectReport {
    pub master_file: String,
    pub merged_plugins: Vec<String>,
    pub blanked_files: usize,
    pub backup_path: Option<PathBuf>,
    pub warnings: Vec<String>,
    pub original_bytes: usize,
    pub result_bytes: usize,
}

#[derive(Debug)]
pub enum ProtectError {
    Io(String),
    NoEnabledPlugins,
    Parse(String),
    AlreadyProtected,
}

impl std::fmt::Display for ProtectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtectError::Io(e) => write!(f, "File error: {e}"),
            ProtectError::NoEnabledPlugins => {
                write!(f, "No enabled plugins found to protect.")
            }
            ProtectError::Parse(e) => write!(f, "{e}"),
            ProtectError::AlreadyProtected => write!(
                f,
                "This project is already protected. Restore the original plugins \
                 (e.g. from the backup) before protecting again."
            ),
        }
    }
}
impl std::error::Error for ProtectError {}

impl From<std::io::Error> for ProtectError {
    fn from(e: std::io::Error) -> Self {
        ProtectError::Io(e.to_string())
    }
}

/// Run the protection on `project`.
pub fn protect(project: &RmProject, opts: &ProtectOptions) -> Result<ProtectReport, ProtectError> {
    let plugins_js = project.plugins_js();
    let plugins_dir = project.plugins_dir();

    let content = fs::read_to_string(&plugins_js)?;
    let entries = parse_plugins_js(&content).map_err(|e| ProtectError::Parse(e.to_string()))?;
    let enabled = enabled_in_order(&entries);
    if enabled.is_empty() {
        return Err(ProtectError::NoEnabledPlugins);
    }
    if is_already_protected(&plugins_dir, &enabled.iter().map(|p| p.file_name()).collect::<Vec<_>>()) {
        return Err(ProtectError::AlreadyProtected);
    }

    let mut report = ProtectReport::default();

    // Read every enabled plugin that actually exists on disk, in order.
    struct Loaded {
        name: String,
        file: PathBuf,
        src: String,
    }
    let mut loaded: Vec<Loaded> = Vec::new();
    for p in &enabled {
        let file = plugins_dir.join(p.file_name());
        match fs::read_to_string(&file) {
            Ok(src) => {
                report.original_bytes += src.len();
                if uses_current_script(&src) {
                    report.warnings.push(format!(
                        "{} uses document.currentScript — may misbehave after merging.",
                        p.file_name()
                    ));
                }
                loaded.push(Loaded {
                    name: p.file_name(),
                    file,
                    src,
                });
            }
            Err(_) => {
                report.warnings.push(format!(
                    "{} is enabled but missing on disk — skipped.",
                    p.file_name()
                ));
            }
        }
    }

    if loaded.is_empty() {
        return Err(ProtectError::NoEnabledPlugins);
    }

    // Build the merged master in load order, stripping annotation blocks and
    // separating files with a newline + semicolon + newline.
    // Level 2+ minifies each plugin individually (not the merged bundle):
    //  * Per-plugin keeps each script's scope isolated, exactly as separate
    //    files behave.
    //  * We DO NOT mangle identifiers — re-mangling broke real projects, and
    //    the unreadability we want comes from comment/whitespace stripping plus
    //    level-3 encryption.
    //  * Already-obfuscated plugins (VisuMZ's `_0x....` scheme, with fragile
    //    self-decoding string tables) are left byte-for-byte intact.
    let do_minify = opts.minify;

    let mut master = String::with_capacity(report.original_bytes + 64);
    master.push_str(PROTECTED_MARKER);
    master.push('\n');
    for item in &loaded {
        // Strip annotation blocks, then neutralize a leading file-level
        // "use strict" directive. The latter is essential: concatenating
        // separate plugin scripts into one file would otherwise let the first
        // plugin's top-level "use strict" put the WHOLE bundle into strict
        // mode, so plugins relying on sloppy-mode behavior (e.g. VisuMZ's
        // `getSelfSwitchValue = function(){}` implicit global) would throw.
        // Each plugin's own inner-IIFE "use strict" is kept, and plugins are
        // never wrapped in an IIFE, so top-level/implicit globals still leak
        // exactly as with separate scripts.
        let base = strip_leading_use_strict(strip_annotations(&item.src).trim());

        let code = if do_minify && !looks_obfuscated(&base) {
            match crate::minify::minify_js(&base) {
                Ok(min) => min,
                Err(e) => {
                    report
                        .warnings
                        .push(format!("{}: minify skipped, kept as-is ({e})", item.name));
                    base
                }
            }
        } else {
            base
        };

        master.push_str("\n/* ");
        master.push_str(&item.name);
        master.push_str(" */\n");
        master.push_str(code.trim());
        master.push_str("\n;\n");
    }

    // Backup the whole plugins folder first (before any writes).
    if opts.backup {
        let dest = backup_dest(&project.root);
        copy_dir(&plugins_dir, &dest)?;
        report.backup_path = Some(dest);
    }

    // Blank every loaded plugin file, then write the master into the first one.
    for item in &loaded {
        fs::write(&item.file, "")?;
    }
    report.blanked_files = loaded.len();

    // Level 3: encrypt the working bundle and replace it with a runtime loader.
    if matches!(opts.level, ProtectionLevel::Encrypt) {
        let enc = crate::encrypt::encrypt(master.as_bytes()).map_err(ProtectError::Io)?;
        master = crate::encrypt::build_loader(&enc);
    }

    let master_item = &loaded[0];
    fs::write(&master_item.file, &master)?;
    report.master_file = master_item.name.clone();
    report.result_bytes = master.len();
    report.merged_plugins = loaded.iter().map(|l| l.name.clone()).collect();

    Ok(report)
}

/// Strip RPG Maker annotation block comments (`/*:` and `/*~struct~`), leaving
/// ordinary comments (including the license `/* ... */`) intact. A JS block
/// comment cannot nest, so each block ends at the first `*/` after its start —
/// exactly how the engine's own parser treats it.
pub fn strip_annotations(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 2 < bytes.len() && bytes[i + 1] == b'*' {
            let marker = bytes[i + 2];
            // Annotation blocks start with `/*:` (optionally `/*:ja` etc.) or `/*~`.
            if marker == b':' || marker == b'~' {
                if let Some(rel) = src[i + 2..].find("*/") {
                    i = i + 2 + rel + 2; // skip past the closing */
                    continue;
                } else {
                    // Unterminated — drop the rest.
                    break;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Skip ASCII whitespace and JS comments (`//...` and `/* ... */`) starting at `i`.
fn skip_ws_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'/' {
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else {
            return i;
        }
    }
}

/// If a string literal starts at `i` (quote char `"` or `'`), return
/// (unquoted content, index just past the closing quote). Minimal escape
/// handling (a backslash skips the next byte).
fn read_string(b: &[u8], i: usize) -> Option<(String, usize)> {
    let quote = b[i];
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut j = i + 1;
    let mut content = String::new();
    while j < b.len() {
        let c = b[j];
        if c == b'\\' {
            j += 2;
            continue;
        }
        if c == quote {
            return Some((content, j + 1));
        }
        content.push(c as char);
        j += 1;
    }
    None
}

/// Remove a leading file-level `"use strict"` directive from a plugin's source,
/// if present in the directive prologue (possibly preceded by comments or other
/// string directives). Everything else is left untouched.
pub fn strip_leading_use_strict(code: &str) -> String {
    let b = code.as_bytes();
    let mut i = skip_ws_comments(b, 0);
    loop {
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            return code.to_string();
        }
        let Some((content, end)) = read_string(b, i) else {
            return code.to_string();
        };
        if content == "use strict" {
            let mut j = skip_ws_comments(b, end);
            if j < b.len() && b[j] == b';' {
                j += 1;
            }
            let mut out = String::with_capacity(code.len());
            out.push_str(&code[..i]);
            out.push_str(&code[j..]);
            return out;
        }
        // A different directive string: skip it (and optional `;`) and continue.
        i = skip_ws_comments(b, end);
        if i < b.len() && b[i] == b';' {
            i += 1;
        }
        i = skip_ws_comments(b, i);
    }
}

fn uses_current_script(src: &str) -> bool {
    src.contains("document.currentScript")
}

/// True if the project looks already protected: an enabled plugin file carries
/// our marker, or (for safety against v1-protected projects) every enabled
/// plugin except the first is an empty file.
pub fn is_already_protected(plugins_dir: &Path, enabled_files: &[String]) -> bool {
    let mut empty_after_first = 0usize;
    let mut existing_after_first = 0usize;
    for (i, name) in enabled_files.iter().enumerate() {
        let path = plugins_dir.join(name);
        let Ok(meta) = fs::metadata(&path) else { continue };
        if i > 0 {
            existing_after_first += 1;
            if meta.len() == 0 {
                empty_after_first += 1;
            }
        }
        // Only the head of the file is needed to find the marker.
        if let Ok(mut f) = fs::File::open(&path) {
            use std::io::Read;
            let mut head = [0u8; 256];
            let n = f.read(&mut head).unwrap_or(0);
            if String::from_utf8_lossy(&head[..n]).contains(PROTECTED_MARKER) {
                return true;
            }
        }
    }
    existing_after_first >= 2 && empty_after_first == existing_after_first
}

/// Heuristic: already-obfuscated code (e.g. VisuMZ's `_0x....` identifier
/// scheme with self-decoding string tables). Such code must not be re-emitted —
/// its decoder is fragile — and it is already unreadable, so there is nothing
/// to gain from minifying it.
pub fn looks_obfuscated(src: &str) -> bool {
    src.matches("_0x").count() > 50
}

fn backup_dest(root: &Path) -> PathBuf {
    root.join(format!(
        "_PKD_Protector_Backup_{}",
        crate::util::timestamp()
    ))
    .join("plugins")
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::detect;
    use std::fs;
    use std::path::Path;

    fn write(p: &Path, c: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, c).unwrap();
    }

    #[test]
    fn strips_annotation_but_keeps_license() {
        let src = "/* Copyright (c) 2022 KageDesu */\n\
                   /*:\n * @plugindesc test\n * @help bla */\n\
                   /*~struct~Item:\n * @param x */\n\
                   var A = 1;\n";
        let out = strip_annotations(src);
        assert!(out.contains("Copyright"), "license must be kept");
        assert!(!out.contains("@plugindesc"));
        assert!(!out.contains("@param"));
        assert!(out.contains("var A = 1;"));
    }

    #[test]
    fn strips_annotation_even_after_leading_license_block() {
        // This is exactly the v1 bug: a `/* */` block before `/*: */`.
        let src = "/* lic */ /*: @help */ code();";
        let out = strip_annotations(src);
        assert!(out.contains("/* lic */"));
        assert!(!out.contains("@help"));
        assert!(out.contains("code();"));
    }

    fn make_project(root: &Path) {
        // plugins.js with 3 enabled + 1 disabled, in a specific order.
        let plugins_js = r#"var $plugins =
[
{"name":"First","status":true,"description":"","parameters":{}},
{"name":"Second","status":true,"description":"","parameters":{}},
{"name":"Off","status":false,"description":"","parameters":{}},
{"name":"Third","status":true,"description":"","parameters":{}}
];
"#;
        write(&root.join("js").join("plugins.js"), plugins_js);
        write(&root.join("js").join("rmmz_core.js"), "//core");
        write(
            &root.join("js").join("plugins").join("First.js"),
            "/*:\n@help a */\n(function(){ window.A = 1; })()",
        );
        write(
            &root.join("js").join("plugins").join("Second.js"),
            "/*:\n@help b */\n(function(){ window.B = 2; })()",
        );
        write(
            &root.join("js").join("plugins").join("Off.js"),
            "/*:\n@help off */\nshould_not_be_touched();",
        );
        write(
            &root.join("js").join("plugins").join("Third.js"),
            "/*:\n@help c */\n(function(){ window.C = 3; })()",
        );
    }

    #[test]
    fn merge_concatenates_in_order_and_blanks_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_project(root);
        let project = detect(root).unwrap();

        let report = protect(
            &project,
            &ProtectOptions {
                level: ProtectionLevel::Merge,
                minify: false,
                backup: true,
            },
        )
        .unwrap();

        assert_eq!(report.master_file, "First.js");
        assert_eq!(report.merged_plugins, vec!["First.js", "Second.js", "Third.js"]);
        assert_eq!(report.blanked_files, 3);

        let pd = root.join("js").join("plugins");
        let master = fs::read_to_string(pd.join("First.js")).unwrap();
        // All three bodies present, in order, headers stripped.
        let a = master.find("window.A").unwrap();
        let b = master.find("window.B").unwrap();
        let c = master.find("window.C").unwrap();
        assert!(a < b && b < c, "load order must be preserved");
        assert!(!master.contains("@help"));
        assert!(master.starts_with(PROTECTED_MARKER));
        // Safe separator inserted between IIFEs.
        assert!(master.contains(")()\n;\n") || master.contains(")();\n"));

        // Other enabled files blanked.
        assert_eq!(fs::read_to_string(pd.join("Second.js")).unwrap(), "");
        assert_eq!(fs::read_to_string(pd.join("Third.js")).unwrap(), "");
        // Disabled plugin left completely alone.
        assert!(fs::read_to_string(pd.join("Off.js"))
            .unwrap()
            .contains("should_not_be_touched"));

        // plugins.js untouched.
        assert!(fs::read_to_string(root.join("js").join("plugins.js"))
            .unwrap()
            .contains("\"name\":\"Off\""));

        // Backup exists and holds the originals.
        let backup = report.backup_path.unwrap();
        assert!(backup.join("First.js").is_file());
        assert!(fs::read_to_string(backup.join("First.js")).unwrap().contains("window.A"));
    }

    #[test]
    fn strips_leading_use_strict_only() {
        assert_eq!(strip_leading_use_strict("\"use strict\";\nvar a=1;"), "\nvar a=1;");
        assert_eq!(strip_leading_use_strict("'use strict'\nx=1"), "x=1");
        // Preceded by a comment — still a directive.
        let r = strip_leading_use_strict("// c\n\"use strict\";\ncode();");
        assert!(!r.contains("use strict"));
        assert!(r.contains("code();"));
        // A "use strict" that is NOT in the prologue must be kept.
        let keep = "var a = \"use strict\";";
        assert_eq!(strip_leading_use_strict(keep), keep);
        // Inner-IIFE strict (after real code) is untouched.
        let inner = "(function(){ \"use strict\"; })();";
        assert_eq!(strip_leading_use_strict(inner), inner);
    }

    #[test]
    fn refuses_to_protect_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_project(root);
        let project = detect(root).unwrap();
        let opts = ProtectOptions { level: ProtectionLevel::Merge, minify: true, backup: false };
        protect(&project, &opts).unwrap();
        assert!(matches!(protect(&project, &opts), Err(ProtectError::AlreadyProtected)));
    }

    #[test]
    fn detects_v1_style_protection() {
        // v1 output: no marker, but every enabled plugin after the first is empty.
        let tmp = tempfile::tempdir().unwrap();
        let pd = tmp.path();
        fs::write(pd.join("A.js"), "merged code").unwrap();
        fs::write(pd.join("B.js"), "").unwrap();
        fs::write(pd.join("C.js"), "").unwrap();
        let names: Vec<String> = ["A.js", "B.js", "C.js"].iter().map(|s| s.to_string()).collect();
        assert!(is_already_protected(pd, &names));
        fs::write(pd.join("C.js"), "real code").unwrap();
        assert!(!is_already_protected(pd, &names));
    }

    #[test]
    fn warns_on_current_script() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("js").join("plugins.js"),
            "var $plugins =\n[\n{\"name\":\"X\",\"status\":true,\"description\":\"\",\"parameters\":{}}\n];\n");
        write(&root.join("js").join("rmmz_core.js"), "//core");
        write(&root.join("js").join("plugins").join("X.js"),
            "/*: */\nconst n = document.currentScript.src;");
        let project = detect(root).unwrap();
        let report = protect(&project, &ProtectOptions { level: ProtectionLevel::Merge, minify: false, backup: false }).unwrap();
        assert!(report.warnings.iter().any(|w| w.contains("currentScript")));
    }
}
