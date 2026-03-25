# Boax

A Ruby gem that embeds the [Boa](https://github.com/boa-dev/boa) JavaScript engine (pure Rust) to let Ruby call JavaScript libraries with an ergonomic, Rubyx-inspired interface.

## Vision

```ruby
intl = Boax.import('Intl')
fmt = intl.DateTimeFormat.new('en-US', { weekday: 'long' })
fmt.format(Boax.eval("Date.now()")).to_ruby  # => "Tuesday"

lodash = Boax.import('lodash-es')
lodash.chunk([1, 2, 3, 4], 2).to_ruby  # => [[1, 2], [3, 4]]
```

## Architecture

```
Ruby VM
  │  magnus crate (high-level Ruby FFI, like Rubyx uses)
  │  rb-sys crate (low-level Ruby C API linkage)
  ▼
boax native extension (cdylib)
  │  boa_engine — the JS engine, linked as a Rust dependency
  │  boa_runtime — WebAPI extensions (console, fetch, timers, URL, etc.)
  │  oxc_resolver — Node.js-compatible module resolution
  ▼
Boa JS engine (in-process, same address space)
```

No dynamic library loading, no GIL, no C++ toolchain. Pure Rust.

## Key Design Decisions

### Proxy objects via method_missing
`BoaxObject` wraps a `JsValue`. Ruby method calls are proxied to JS property
access / function calls via `method_missing`, following the Rubyx pattern. Complex
JS objects stay as opaque `BoaxObject` wrappers; leaf values convert via `to_ruby`.

### Constructor calls
When a JS value is a constructor (e.g., `Intl.DateTimeFormat`), calling `.new(...)`
from Ruby should use `JsObject::construct()` rather than `call()`. This is more
Ruby-idiomatic than Rubyx's approach of overloading the call operator.

### Module loading strategy
- Built-in globals (`Intl`, `Math`, `JSON`): `context.global_object().get(name)`
- npm packages: Custom `ModuleLoader` using `oxc_resolver` against `node_modules/`
- Node APIs (`path`, `fs`, etc.): Synthetic modules registered in the loader

### GC coordination
`BoaxObject` must prevent the `BoaxRuntime` (and its `Context`) from being collected
while any `BoaxObject` is alive. Use `Rc` or magnus mark protocol.

### Type conversion (ruby_to_js / js_to_ruby)
| Ruby | JsValue |
|---|---|
| `nil` | `JsValue::undefined()` |
| `true`/`false` | `JsValue::Boolean` |
| `Integer` | `JsValue::Integer` |
| `Float` | `JsValue::Rational` |
| `String` | `JsString` |
| `Symbol` | `JsString` |
| `Array` | `JsArray` |
| `Hash` | `JsObject` (plain object) |
| Complex JS objects | Opaque `BoaxObject` wrapper |

## Reference Projects

- **Rubyx** (https://github.com/yinho999/rubyx) — Ruby↔Python bridge via Rust.
  Uses magnus + rb-sys for Ruby FFI, libloading for Python embedding.
  Boax follows the same gem structure and proxy-object pattern.

- **mini_racer** (https://github.com/rubyjs/mini_racer) — Ruby gem embedding V8.
  Eval-based interface, no module support, ~45MB binary from V8.
  Boax differentiates on: ergonomic import API, ES module support, npm packages,
  smaller binary (~5-10MB), pure Rust (no C++ toolchain), portability.

- **Boa engine** (https://github.com/boa-dev/boa) — Pure Rust JS engine.
  Key crates: boa_engine, boa_runtime, boa_gc, boa_parser.
  Key traits: ModuleLoader, RuntimeExtension, Class, TryFromJs, TryIntoJs.
  Key types: Context, JsValue, JsObject, Module, NativeFunction.
  Has full ES module support, synthetic modules, custom module loaders.
  See boa's examples/src/bin/ for embedding patterns.

## Boa Source

A clone of boa is available at /Users/rubys/git/boa for reference. Key locations:
- `core/engine/src/module/loader/mod.rs` — ModuleLoader trait
- `core/engine/src/module/synthetic.rs` — Synthetic modules
- `core/engine/src/context/mod.rs` — Context and ContextBuilder
- `core/runtime/src/extensions.rs` — RuntimeExtension trait
- `examples/src/bin/modules.rs` — Module loading example
- `examples/src/bin/modulehandler.rs` — Custom require() example
- `examples/src/bin/classes.rs` — Native class registration
- `examples/src/bin/closures.rs` — Rust closures as JS functions

## Competitive Position

- vs mini_racer: Smaller, portable, ergonomic import API, module support. Slower (no JIT yet).
- Boa's roadmap includes JIT work. When that lands, the speed gap narrows and boax's
  structural advantages (size, portability, ergonomics) remain.
- boax creates a real-world use case for Boa, which motivates engine improvements,
  which makes boax more capable — a virtuous cycle.

## Long-term Strategy

Node API implementations (`path`, `fs`, `events`, etc.) are Ruby-independent Rust code.
As they stabilize, extract them into shared crates (e.g., `boa_node`) or contribute
upstream to `boa_runtime`. This benefits the entire Boa ecosystem.
