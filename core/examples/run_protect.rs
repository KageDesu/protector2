use std::path::Path;
use protector_core::{detect, protect, ProtectOptions, ProtectionLevel};
fn main() {
    let root = std::env::args().nth(1).expect("path");
    let level = match std::env::args().nth(2).as_deref() {
        
        Some("encrypt") => ProtectionLevel::Encrypt,
        _ => ProtectionLevel::Merge,
    };
    let project = detect(Path::new(&root)).expect("detect");
    let report = protect(&project, &ProtectOptions { level, minify: std::env::args().nth(3).as_deref() != Some("nomin"), backup: true }).expect("protect");
    println!("master: {}", report.master_file);
    println!("merged: {} plugins", report.merged_plugins.len());
    println!("bytes: {} -> {}", report.original_bytes, report.result_bytes);
    for w in &report.warnings { println!("  WARN {w}"); }
}
