pub mod path;
pub mod util;
pub mod events;
pub mod fs;
pub mod process;
pub mod os;
pub mod querystring;
pub mod string_decoder;
pub mod assert;
pub mod url_module;
pub mod buffer;
pub mod crypto;

use boa_engine::{Context, Module};

/// Check if a specifier refers to a Node.js built-in module.
/// Handles both "path" and "node:path" forms.
pub fn resolve_node_builtin(specifier: &str) -> Option<&str> {
    let name = specifier.strip_prefix("node:").unwrap_or(specifier);
    match name {
        "path" | "path/posix" | "util" | "events" | "fs" |
        "process" | "os" | "querystring" | "string_decoder" |
        "assert" | "assert/strict" | "url" | "buffer" | "crypto" => Some(name),
        _ => None,
    }
}

/// Create a synthetic module for a Node.js built-in.
pub fn create_node_module(name: &str, context: &mut Context) -> Module {
    match name {
        "path" | "path/posix" => path::create_module(context),
        "util" => util::create_module(context),
        "events" => events::create_module(context),
        "fs" => fs::create_module(context),
        "process" => process::create_module(context),
        "os" => os::create_module(context),
        "querystring" => querystring::create_module(context),
        "string_decoder" => string_decoder::create_module(context),
        "assert" | "assert/strict" => assert::create_module(context),
        "url" => url_module::create_module(context),
        "buffer" => buffer::create_module(context),
        "crypto" => crypto::create_module(context),
        _ => unreachable!("unknown node builtin: {name}"),
    }
}
