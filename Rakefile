# frozen_string_literal: true

require "rake/extensiontask"
require "rspec/core/rake_task"

task default: [:compile, :spec]

Rake::ExtensionTask.new("boax") do |ext|
  ext.lib_dir = "lib/boax"
  ext.source_pattern = "*.{rs,toml}"
  ext.cross_compile = true
end

RSpec::Core::RakeTask.new(:spec)
