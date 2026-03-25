# Boax

Call JavaScript from Ruby. No bundling, no eval strings, no V8.

Powered by [Boa](https://github.com/boa-dev/boa), a JavaScript engine written in pure Rust.

```ruby
intl = Boax.import('Intl')
nf = intl.NumberFormat.new('en-US', { style: 'currency', currency: 'USD' })
nf.format(1234.56).to_ruby  # => "$1,234.56"
```

```ruby
_ = Boax.import('lodash-es')
_.chunk([1, 2, 3, 4, 5, 6], 2).to_ruby  # => [[1, 2], [3, 4], [5, 6]]
```

## Why not mini_racer?

| | mini_racer | boax |
|---|---|---|
| Engine | V8 (~45MB binary) | Boa (~5-10MB, pure Rust) |
| Interface | `ctx.eval("...")` | `Boax.import('lodash-es').uniq([1,1,2]).to_ruby` |
| ES modules | No | Yes |
| npm packages | Manual bundling required | `Boax.import('package-name')` |
| Node APIs | None | Incrementally added |
| Platforms | No Windows, fork-safety issues | Everywhere Rust compiles |

## Status

Under development. See [PLAN.md](PLAN.md) for the implementation roadmap.

## The bang (`!`) escape hatch

Ruby method calls on `Boax::JsObject` are proxied to JavaScript via `method_missing`. When a Ruby built-in method shadows a JS method name, append `!` to bypass Ruby and call the JS method directly.

The most common case is `Promise.then()` — Ruby's `Kernel#then` intercepts the call before `method_missing` gets it:

```ruby
promise = fs["promises"].readFile("data.txt")

# ✗ Calls Ruby's Kernel#then, not JS Promise.then()
promise.then(callback)

# ✓ Strips the !, calls JS promise.then(callback)
promise.then!(callback)
```

This works for any JS method name that conflicts with a Ruby method. The `!` is stripped before the JS property lookup, so `obj.then!(cb)` calls `obj.then(cb)` on the JS side.

## Known Issues

**Intl support is incomplete.** Boa 0.21 has partial Intl implementation — `Intl.NumberFormat` and `Intl.DateTimeFormat` throw "unimplemented" errors.

**Cache constructor references from Node API modules.** A Boa GC issue can corrupt synthetic module namespace properties after heavy object use. Grab constructors once rather than re-accessing them from the module:

```ruby
# ✓ Reliable
EventEmitter = Boax.import("events")["EventEmitter"]
ee = EventEmitter.new

# ✗ May fail after heavy use
ee = Boax.import("events").EventEmitter.new
```

This only affects the built-in Node API modules (`path`, `util`, `events`), not npm packages.

## License

This project is licensed under the [Unlicense](./LICENSE-UNLICENSE) or [MIT](./LICENSE-MIT) licenses, at your option.
