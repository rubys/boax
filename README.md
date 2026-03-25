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

## License

This project is licensed under the [Unlicense](./LICENSE-UNLICENSE) or [MIT](./LICENSE-MIT) licenses, at your option.
