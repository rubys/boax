# Boax

Call JavaScript from Ruby. No bundling, no eval strings, no V8.

Powered by [Boa](https://github.com/boa-dev/boa), a JavaScript engine written in pure Rust.

```ruby
require 'boax'

math = Boax.import('Math')
math.sqrt(144)  # => 12

Boax.init(root: __dir__)
_ = Boax.import('lodash-es')
_.chunk([1, 2, 3, 4, 5, 6], 2).to_ruby  # => [[1, 2], [3, 4], [5, 6]]
_.uniq([1, 1, 2, 3, 3]).to_ruby          # => [1, 2, 3]
```

## Status

**Proof of concept.** This is an early exploration of embedding the Boa JS engine in Ruby via Rust. It works, but the API may change, performance hasn't been tuned, and Boa itself is pre-1.0.

### What works

- **`Boax.eval(code)`** — evaluate JS expressions, returns native Ruby types
- **`Boax.import(name)`** — import JS globals (`Math`, `JSON`, `Date`) or npm packages (`lodash-es`)
- **Proxy objects** — `method_missing` forwards Ruby calls to JS: `math.sqrt(144)`, `_.chunk([1,2,3], 2)`
- **Constructors** — `Date.new(2024, 0, 15)` calls `new Date(2024, 0, 15)` in JS
- **Type conversion** — nil, bool, integer, float, string, symbol, array, hash (both directions)
- **`to_ruby`** — deep conversion of JS arrays/objects to Ruby arrays/hashes
- **npm packages** — ES module resolution via [oxc_resolver](https://github.com/nicolo-ribaudo/oxc-resolver) against `node_modules/`
- **Node API modules** — `path`, `util`, `events`, `fs`, `process`, `os`, `querystring`, `string_decoder`, `assert`, `url`, `buffer`, `crypto`, `stream`
- **Web API globals** — `URL`, `URLSearchParams`, `console`, `setTimeout`/`setInterval`, `TextEncoder`/`TextDecoder`, `structuredClone`

### What doesn't work (yet)

- **Intl** — Boa 0.21 has partial Intl support; `NumberFormat` and `DateTimeFormat` throw "unimplemented"
- **Performance** — Boa has no JIT; compute-heavy JS will be slower than V8
- **CommonJS** — only ES modules are supported; CJS packages need a bundler
- **Streams** — MVP without backpressure; no real async I/O
- **HTTP, child_process** — not yet implemented

## Why not mini_racer?

| | mini_racer | boax |
|---|---|---|
| Engine | V8 (~45MB binary) | Boa (~5-10MB, pure Rust) |
| Interface | `ctx.eval("...")` | `Boax.import('lodash-es').uniq([1,1,2]).to_ruby` |
| ES modules | No | Yes |
| npm packages | Manual bundling required | `Boax.import('package-name')` |
| Node APIs | None | 13 built-in modules |
| Platforms | No Windows, fork-safety issues | Everywhere Rust compiles |

## Getting started

Requirements: Ruby 3.1+, Rust 1.70+, npm (for package imports).

```sh
git clone https://github.com/rubys/boax
cd boax
bundle install
bundle exec rake compile

# Run the tests
npm install       # installs lodash-es for integration tests
bundle exec rspec
```

### Usage

```ruby
require 'boax'

# Evaluate JS
Boax.eval("1 + 2")            # => 3
Boax.eval("'hello'.repeat(3)") # => "hellohellohello"

# Import JS globals
json = Boax.import('JSON')
json.stringify({ a: 1, b: [2, 3] })  # => '{"a":1,"b":[2,3]}'

# Import npm packages (requires Boax.init and npm install)
Boax.init(root: __dir__)
_ = Boax.import('lodash-es')
_.camelCase('foo-bar')  # => "fooBar"

# Node built-in modules
path = Boax.import('path')
path.join('/foo', 'bar', 'baz')  # => "/foo/bar/baz"

fs = Boax.import('fs')
fs.writeFileSync('/tmp/test.txt', 'hello')
fs.readFileSync('/tmp/test.txt')  # => "hello"

# Deep conversion to Ruby types
Boax.eval("({a: [1, {b: 2}]})").to_ruby
# => {"a" => [1, {"b" => 2}]}
```

## The bang (`!`) escape hatch

Ruby method calls on `Boax::JsObject` are proxied to JavaScript via `method_missing`. When a Ruby built-in method shadows a JS method name, append `!` to bypass Ruby and call the JS method directly.

The most common case is `Promise.then()` — Ruby's `Kernel#then` intercepts the call before `method_missing` gets it:

```ruby
promise = fs["promises"].readFile("data.txt")

# Calls Ruby's Kernel#then, not JS Promise.then()
promise.then(callback)

# Strips the !, calls JS promise.then(callback)
promise.then!(callback)
```

This works for any JS method name that conflicts with a Ruby method. The `!` is stripped before the JS property lookup, so `obj.then!(cb)` calls `obj.then(cb)` on the JS side.

## License

This project is licensed under the [Unlicense](./LICENSE-UNLICENSE) or [MIT](./LICENSE-MIT) licenses, at your option.
