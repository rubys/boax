# frozen_string_literal: true

require "boax"

# These specs require node_modules/ with lodash-es installed.
# Run: npm install lodash-es
RSpec.describe "Boax module loading", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
  end

  describe "Boax.init" do
    it "raises when node_modules is missing" do
      expect { Boax.init(root: "/tmp") }.to raise_error(Boax::Error, /node_modules not found/)
    end

    it "accepts keyword root argument" do
      expect { Boax.init(root: File.join(__dir__, "..")) }.not_to raise_error
    end
  end

  describe "Boax.import with npm packages" do
    let(:lodash) { Boax.import("lodash-es") }

    it "imports lodash-es" do
      expect(lodash).to be_a(Boax::JsObject)
    end

    it "calls lodash.chunk" do
      result = lodash.chunk([1, 2, 3, 4, 5, 6], 2).to_ruby
      expect(result).to eq([[1, 2], [3, 4], [5, 6]])
    end

    it "calls lodash.uniq" do
      result = lodash.uniq([1, 1, 2, 3, 3]).to_ruby
      expect(result).to eq([1, 2, 3])
    end

    it "calls lodash.flatten" do
      result = lodash.flatten([1, [2, [3]]]).to_ruby
      expect(result).to eq([1, 2, [3]])
    end

    it "calls lodash.flattenDeep" do
      result = lodash.flattenDeep([1, [2, [3, [4]]]]).to_ruby
      expect(result).to eq([1, 2, 3, 4])
    end

    it "calls lodash.compact" do
      result = lodash.compact([0, 1, false, 2, "", 3]).to_ruby
      expect(result).to eq([1, 2, 3])
    end

    it "calls lodash string functions" do
      expect(lodash.camelCase("foo-bar").to_s).to eq("fooBar")
      expect(lodash.kebabCase("fooBar").to_s).to eq("foo-bar")
      expect(lodash.snakeCase("Foo Bar").to_s).to eq("foo_bar")
    end

    it "calls lodash object functions" do
      result = lodash.pick({ a: 1, b: 2, c: 3 }, ["a", "c"]).to_ruby
      expect(result).to eq({ "a" => 1, "c" => 3 })
    end

    it "calls lodash.merge" do
      result = lodash.merge({ a: 1 }, { b: 2 }).to_ruby
      expect(result).to eq({ "a" => 1, "b" => 2 })
    end

    it "calls lodash.range" do
      result = lodash.range(0, 10, 2).to_ruby
      expect(result).to eq([0, 2, 4, 6, 8])
    end
  end

  describe "globals still work after init" do
    it "imports Math" do
      math = Boax.import("Math")
      expect(math.sqrt(144)).to eq(12)
    end

    it "imports JSON" do
      json = Boax.import("JSON")
      expect(json.stringify({ a: 1 })).to eq('{"a":1}')
    end
  end

  describe "error handling for unknown packages" do
    it "raises for uninstalled packages" do
      expect { Boax.import("nonexistent-package-xyz") }.to raise_error(Boax::Error, /could not resolve/)
    end
  end
end
