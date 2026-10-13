// Usage: minify_file <path> [mangle]   (mangle = "1" to enable)
use std::fs;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let mangle = std::env::args().nth(2).as_deref() == Some("1");
    let src = fs::read_to_string(&path).expect("read");
    match protector_core::minify::minify_js_opts(&src, mangle) {
        Ok(out) => print!("{out}"),
        Err(e) => { eprintln!("ERR {e}"); std::process::exit(2); }
    }
}
