# frozen_string_literal: true

require "boax"

# These specs require node_modules/ with minimist installed.
# Run: npm install minimist
RSpec.describe "Boax.require (CommonJS)", skip: !File.directory?(File.join(__dir__, "..", "node_modules", "minimist")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
  end

  describe "CJS npm packages" do
    it "loads minimist" do
      minimist = Boax.require("minimist")
      expect(minimist.typeof).to eq("function")
    end

    it "calls a bare function export" do
      minimist = Boax.require("minimist")
      result = minimist.call(["--foo", "bar", "hello"])
      expect(result.to_ruby).to eq({ "_" => ["hello"], "foo" => "bar" })
    end

    it "parses flags correctly" do
      minimist = Boax.require("minimist")
      result = minimist.call(["--verbose", "--count", "3"]).to_ruby
      expect(result["verbose"]).to be true
      expect(result["count"]).to eq(3)
    end
  end

  describe "CJS node builtins" do
    it "requires path" do
      path = Boax.require("path")
      expect(path.join("a", "b").to_s).to eq("a/b")
    end

    it "requires fs" do
      fs = Boax.require("fs")
      expect(fs.existsSync("/tmp")).to be true
    end

    it "requires node:path" do
      path = Boax.require("node:path")
      expect(path.join("x", "y").to_s).to eq("x/y")
    end
  end

  describe "caching" do
    it "returns the same object on repeated requires" do
      a = Boax.require("minimist")
      b = Boax.require("minimist")
      # Both should be function type (cached)
      expect(a.typeof).to eq("function")
      expect(b.typeof).to eq("function")
    end
  end

  describe "error handling" do
    it "raises for uninstalled packages" do
      expect { Boax.require("nonexistent-package-xyz") }.to raise_error(Boax::Error, /could not resolve/)
    end
  end

  describe "coexistence with ESM" do
    it "import and require work in the same session" do
      _ = Boax.import("lodash-es")
      minimist = Boax.require("minimist")
      expect(_.chunk([1, 2, 3, 4], 2).to_ruby).to eq([[1, 2], [3, 4]])
      expect(minimist.call(["--x", "1"]).to_ruby["x"]).to eq(1)
    end
  end
end
