# Boax Implementation Plan

## Phase 1: Proof of Concept — `Boax.import('Intl')` (Days 1-2)

### 1.1 Scaffold the gem
- [ ] `boax.gemspec` with metadata, `ext/boax` as extension dir
- [ ] `Gemfile` referencing gemspec
- [ ] `ext/boax/Cargo.toml` — cdylib crate depending on:
  - `magnus` (with `rb-sys`, `embed` feature)
  - `boa_engine` (with `intl` feature)
  - `boa_runtime`
  - `boa_gc`
- [ ] `ext/boax/extconf.rb` — tells rubygems to build via cargo (rb-sys-dock or cargo-rb)
- [ ] `lib/boax.rb` — Ruby entry point, requires native ext

### 1.2 BoaxRuntime
- [ ] Rust struct wrapping `boa_engine::Context` in `RefCell`
- [ ] Magnus wrap as `Boax::Runtime` Ruby class
- [ ] Singleton instance (or thread-local) accessible from `Boax` module
- [ ] Register `boa_runtime` extensions (console, timers, etc.) on context

### 1.3 BoaxObject
- [ ] Rust struct holding `JsValue` + reference to runtime
- [ ] Magnus wrap as `Boax::JsObject` (or just `BoaxObject`)
- [ ] `method_missing` — proxy to `JsObject::get()` / `call()` / `construct()`
- [ ] `to_ruby` — recursive conversion of leaf values to native Ruby types
- [ ] `to_s` / `inspect` — delegate to JS `toString()`
- [ ] `[]` / `[]=` — subscript access
- [ ] `respond_to_missing?` — check JS property existence

### 1.4 Type conversion
- [ ] `ruby_to_js(magnus::Value, &mut Context) -> JsResult<JsValue>`
- [ ] `js_to_ruby(JsValue, &Ruby) -> Result<magnus::Value, Error>`
- [ ] Handle: nil, bool, integer, float, string, symbol, array, hash

### 1.5 Boax.import for globals
- [ ] `Boax.import(name)` — look up `name` on `context.global_object()`, wrap in BoaxObject
- [ ] Works for: `Intl`, `Math`, `JSON`, `Date`, `Promise`, etc.

### 1.6 Boax.eval
- [ ] `Boax.eval(js_string)` — evaluate JS code, return BoaxObject
- [ ] Useful for expressions like `Boax.eval("Date.now()")`

### 1.7 First working demo
```ruby
require 'boax'
intl = Boax.import('Intl')
nf = intl.NumberFormat.new('en-US', { style: 'currency', currency: 'USD' })
puts nf.format(1234.56).to_ruby  # => "$1,234.56"
```

---

## Phase 2: npm Package Support — `Boax.import('lodash-es')` (Days 3-5)

### 2.1 Custom ModuleLoader
- [ ] Add `oxc_resolver` to Cargo.toml
- [ ] Implement `ModuleLoader` trait that:
  - Handles relative/absolute paths (delegate to SimpleModuleLoader behavior)
  - Resolves bare specifiers via `oxc_resolver` against a configurable `node_modules/` path
  - Caches loaded modules
- [ ] Support `package.json` `exports`, `main`, `module` fields via oxc_resolver

### 2.2 Boax.import for npm packages
- [ ] Detect whether `name` is a global or a module specifier
- [ ] For modules: create ES module source (`export * from 'package-name'`), load/link/evaluate
- [ ] Run `context.run_jobs()` to resolve async module evaluation
- [ ] Return module namespace as BoaxObject

### 2.3 Package management
- [ ] `Boax.init(root:)` — set the project root where `node_modules/` lives
- [ ] User runs `npm install` themselves (don't automate this initially)
- [ ] Document the workflow: `npm init && npm install lodash-es`, then `Boax.import('lodash-es')`

### 2.4 Second milestone demo
```ruby
require 'boax'
Boax.init(root: __dir__)  # expects node_modules/ here

_ = Boax.import('lodash-es')
_.chunk([1, 2, 3, 4, 5, 6], 2).to_ruby  # => [[1, 2], [3, 4], [5, 6]]
_.uniq([1, 1, 2, 3, 3]).to_ruby          # => [1, 2, 3]
```

---

## Phase 3: First Node APIs (Days 6-8)

### 3.1 Synthetic module infrastructure
- [ ] Extend ModuleLoader to intercept `node:*` and bare `path`/`fs`/etc. specifiers
- [ ] Return `Module::synthetic(...)` for known Node built-in names
- [ ] Pattern: each Node API is a separate Rust module implementing the JS exports

### 3.2 `path` module
- [ ] `join`, `resolve`, `basename`, `dirname`, `extname`
- [ ] `parse`, `format`, `normalize`, `isAbsolute`, `relative`
- [ ] `sep`, `delimiter`, `posix`, `win32`
- [ ] Test against Node.js docs edge cases

### 3.3 `util` module (partial)
- [ ] `util.format`, `util.inspect`, `util.types`
- [ ] `util.inherits` (legacy but widely used)
- [ ] `util.promisify` (if Promise integration is working)

### 3.4 `events` module
- [ ] `EventEmitter` class: `on`, `off`, `once`, `emit`, `removeListener`
- [ ] `listenerCount`, `eventNames`, `setMaxListeners`
- [ ] Implement as a native Rust `Class` registered via Boa's Class trait

---

## Phase 4: `fs` Module (Days 9-12)

### 4.1 Sync operations (core 15)
- [ ] `readFileSync`, `writeFileSync`, `appendFileSync`
- [ ] `existsSync`, `accessSync`
- [ ] `mkdirSync` (with `recursive` option), `rmdirSync`, `rmSync`
- [ ] `readdirSync` (with `withFileTypes` option)
- [ ] `statSync`, `lstatSync` — return Stats object
- [ ] `unlinkSync`, `renameSync`, `copyFileSync`
- [ ] `chmodSync`, `chownSync`

### 4.2 Stats object
- [ ] Properties: `size`, `mtime`, `atime`, `ctime`, `birthtime`, `mode`, `uid`, `gid`
- [ ] Methods: `isFile()`, `isDirectory()`, `isSymbolicLink()`
- [ ] Implement as Boa `Class`

### 4.3 Callback/Promise async variants
- [ ] `readFile(path, callback)` — spawn on job queue, invoke callback
- [ ] `fs.promises.readFile(path)` — return Promise
- [ ] Pattern: sync impl is the core, async wraps it

### 4.4 Defer to later
- `fs.createReadStream` / `createWriteStream` (needs `stream` module)
- `fs.watch` / `fs.watchFile` (needs event loop integration)

---

## Phase 5: Polish & Release (Days 13-14)

- [ ] Error handling: JS exceptions → Ruby exceptions with useful messages
- [ ] Gem packaging: precompiled gems via `rb-sys` cross-compilation (or source-only initially)
- [ ] Tests: RSpec suite covering import, eval, type conversion, Node APIs
- [ ] README with installation, usage examples, comparison to mini_racer
- [ ] Publish to RubyGems as 0.1.0

---

## Future Work (Post-MVP)

### Additional Node APIs (priority order)
1. `buffer` — Buffer class wrapping TypedArrays
2. `os` — platform info via `std::env` / `sysinfo`
3. `crypto` — hashing/HMAC via `ring` or `sha2` crates
4. `stream` — Readable/Writable/Transform streams
5. `http` / `https` — via `hyper` (significant effort)
6. `child_process` — via `std::process::Command`

### Performance
- Pre-bundling npm packages (via rolldown or simple concatenation) for faster cold start
- Module caching to disk

### Ecosystem
- Extract Node API crates into `boa_node` or contribute to `boa_runtime`
- Rails integration (`Boax::Rails.configure`, generators)
- Async support (Ruby Fibers / Ractors ↔ Boa Promises)
