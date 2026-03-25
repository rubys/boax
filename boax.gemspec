# frozen_string_literal: true

require_relative "lib/boax/version"

Gem::Specification.new do |spec|
  spec.name = "boax"
  spec.version = Boax::VERSION
  spec.authors = ["Sam Ruby"]
  spec.summary = "Call JavaScript from Ruby via the Boa engine (pure Rust)"
  spec.description = "Embeds the Boa JavaScript engine to let Ruby call JS libraries with an ergonomic, proxy-object interface. No V8, no bundling, no C++ toolchain."
  spec.homepage = "https://github.com/rubys/boax"
  spec.license = "MIT OR Unlicense"
  spec.required_ruby_version = ">= 3.1"

  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,rb,lock}", "CLAUDE.md", "PLAN.md", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/boax/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
