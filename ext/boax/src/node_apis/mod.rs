pub mod path;
pub mod util;
pub mod events;

use boa_engine::{Context, Module};

/// Check if a specifier refers to a Node.js built-in module.
/// Handles both "path" and "node:path" forms.
pub fn resolve_node_builtin(specifier: &str) -> Option<&str> {
    let name = specifier.strip_prefix("node:").unwrap_or(specifier);
    match name {
        "path" | "path/posix" | "util" | "events" => Some(name),
        _ => None,
    }
}

/// Create a synthetic module for a Node.js built-in.
pub fn create_node_module(name: &str, context: &mut Context) -> Module {
    match name {
        "path" | "path/posix" => path::create_module(context),
        "util" => util::create_module(context),
        "events" => events::create_module(context),
        _ => unreachable!("unknown node builtin: {name}"),
    }
}
