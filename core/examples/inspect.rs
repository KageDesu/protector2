use std::path::Path;
fn main() {
    for arg in std::env::args().skip(1) {
        println!("== {arg}");
        match protector_core::inspect(Path::new(&arg)) {
            Ok((proj, info)) => println!(
                "  OK  engine={}  title={:?}  enabled={}/{}  js_base={}",
                info.engine.as_str(), info.title, info.enabled_plugins, info.total_plugins,
                proj.js_base.display()
            ),
            Err(e) => println!("  ERR {e}"),
        }
    }
}
