//! JavaScript minification + identifier mangling via oxc (pure Rust).
//!
//! Safety-first configuration for RPG Maker plugins:
//! * Source type is **script** (not module), so top-level `var`/`function`
//!   declarations keep their global-object semantics.
//! * `top_level` mangling is **off** — global names (`Imported`, `VisuMZ`,
//!   plugin namespaces, functions reached via `window.X` or event script
//!   calls) are never renamed. Only locals inside functions get mangled.
//! * Property mangling is **off** — renaming `.prototype.foo` would break
//!   everything.
//! * Aggressive compression is **off** by default: a tree-shaker can drop code
//!   it believes is unused but which is actually invoked dynamically. We only
//!   strip comments/whitespace and mangle locals, which is safe and already
//!   makes the output unreadable.
//!
//! If parsing fails for any reason, the original source is returned unchanged
//! (with the error surfaced to the caller), so protection never corrupts code.

use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
use oxc_mangler::MangleOptions;
use oxc_minifier::{Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Minify (whitespace + comment strip) and, when `mangle` is true, also mangle
/// local identifiers. Returns the transformed code, or an error string if the
/// input could not be parsed (caller should then keep the original).
pub fn minify_js_opts(src: &str, mangle: bool) -> Result<String, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::script();

    let parsed = Parser::new(&allocator, src, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        let first = parsed
            .diagnostics
            .iter()
            .next()
            .map(|d| d.to_string())
            .unwrap_or_default();
        return Err(format!("parse error: {first}"));
    }

    let mut program = parsed.program;

    let options = MinifierOptions {
        mangle: mangle.then(|| MangleOptions {
            top_level: Some(false), // never rename global/top-level bindings
            ..MangleOptions::default()
        }),
        mangle_properties: None,
        compress: None, // conservative: no dead-code elimination / folding
    };

    let ret = Minifier::new(options).minify(&allocator, &mut program);

    let out = Codegen::new()
        .with_options(CodegenOptions {
            minify: true,
            comments: CommentOptions::disabled(),
            ..CodegenOptions::default()
        })
        .with_scoping(ret.scoping)
        .build(&program);

    Ok(out.code)
}

/// Default minify: strip comments/whitespace only. Mangling is opt-in because
/// re-mangling already-obfuscated third-party code (e.g. VisuMZ) can break it.
pub fn minify_js(src: &str) -> Result<String, String> {
    minify_js_opts(src, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangles_locals_keeps_globals_and_removes_comments() {
        let src = r#"
            /* a comment */
            var Imported = Imported || {};
            Imported.MyPlugin = true;
            (function () {
                var averyLongLocalName = 123;
                function helperInside() { return averyLongLocalName + 1; }
                window.publicApi = helperInside;
            })();
        "#;
        let out = minify_js_opts(src, true).unwrap();
        // Comment gone, whitespace collapsed.
        assert!(!out.contains("a comment"));
        assert!(!out.contains("\n\n"));
        // Global names preserved.
        assert!(out.contains("Imported"));
        assert!(out.contains("window.publicApi"));
        // Long local name mangled away.
        assert!(!out.contains("averyLongLocalName"));
    }

    #[test]
    fn preserves_implicit_global_assignment() {
        // The VisuMZ pattern that broke under strict mode must survive minify.
        let src = "getSelfSwitchValue = function(a,b,c){ return a+b+c; };";
        let out = minify_js(src).unwrap();
        assert!(out.contains("getSelfSwitchValue"));
    }

    #[test]
    fn invalid_js_reports_error() {
        assert!(minify_js("function (){ this is not js ++").is_err());
    }
}
