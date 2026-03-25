# frozen_string_literal: true

require "boax"

RSpec.describe "Node.js API modules", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
  end

  describe "path module" do
    let(:path) { Boax.import("path") }

    it "is importable as 'path'" do
      expect(path).to be_a(Boax::JsObject)
    end

    it "is importable as 'node:path'" do
      expect(Boax.import("node:path")).to be_a(Boax::JsObject)
    end

    describe "join" do
      it "joins path segments" do
        expect(path.join("/foo", "bar", "baz").to_s).to eq("/foo/bar/baz")
      end

      it "normalizes the result" do
        expect(path.join("/foo", "bar", "..", "baz").to_s).to eq("/foo/baz")
      end

      it "returns '.' for empty input" do
        expect(path.join.to_s).to eq(".")
      end
    end

    describe "basename" do
      it "returns the last portion" do
        expect(path.basename("/foo/bar/baz.html").to_s).to eq("baz.html")
      end

      it "strips the extension when provided" do
        expect(path.basename("/foo/bar/baz.html", ".html").to_s).to eq("baz")
      end
    end

    describe "dirname" do
      it "returns the directory" do
        expect(path.dirname("/foo/bar/baz").to_s).to eq("/foo/bar")
      end
    end

    describe "extname" do
      it "returns the extension" do
        expect(path.extname("index.html").to_s).to eq(".html")
      end

      it "returns empty string for no extension" do
        expect(path.extname("index").to_s).to eq("")
      end
    end

    describe "normalize" do
      it "normalizes dots" do
        expect(path.normalize("/foo/bar//baz/asdf/quux/..").to_s).to eq("/foo/bar/baz/asdf")
      end

      it "returns '.' for empty string" do
        expect(path.normalize("").to_s).to eq(".")
      end
    end

    describe "isAbsolute" do
      it "returns true for absolute paths" do
        expect(path.isAbsolute("/foo/bar")).to be true
      end

      it "returns false for relative paths" do
        expect(path.isAbsolute("foo/bar")).to be false
      end
    end

    describe "parse" do
      it "parses a path into components" do
        result = path.parse("/home/user/file.txt").to_ruby
        expect(result["root"]).to eq("/")
        expect(result["dir"]).to eq("/home/user")
        expect(result["base"]).to eq("file.txt")
        expect(result["ext"]).to eq(".txt")
        expect(result["name"]).to eq("file")
      end
    end

    describe "format" do
      it "formats a path object" do
        expect(path.format({ dir: "/home/user", base: "file.txt" }).to_s).to eq("/home/user/file.txt")
      end
    end

    describe "relative" do
      it "computes relative path" do
        result = path.relative("/data/orandea/test/aaa", "/data/orandea/impl/bbb").to_s
        expect(result).to eq("../../impl/bbb")
      end
    end

    describe "sep and delimiter" do
      it "has sep" do
        expect(path.sep.to_s).to eq("/")
      end

      it "has delimiter" do
        expect(path.delimiter.to_s).to eq(":")
      end
    end
  end

  describe "util module" do
    let(:util) { Boax.import("util") }

    it "is importable as 'util'" do
      expect(util).to be_a(Boax::JsObject)
    end

    it "is importable as 'node:util'" do
      expect(Boax.import("node:util")).to be_a(Boax::JsObject)
    end

    describe "format" do
      it "formats with %s" do
        expect(util.format("%s:%s", "foo", "bar").to_s).to eq("foo:bar")
      end

      it "formats with %d" do
        expect(util.format("%d + %d = %d", 1, 2, 3).to_s).to eq("1 + 2 = 3")
      end

      it "formats with %j" do
        expect(util.format("%j", { a: 1 }).to_s).to eq('{"a":1}')
      end

      it "joins non-string args with spaces" do
        expect(util.format(1, 2, 3).to_s).to eq("1 2 3")
      end
    end

    describe "isDeepStrictEqual" do
      it "compares equal objects" do
        expect(util.isDeepStrictEqual({ a: 1, b: 2 }, { a: 1, b: 2 })).to be true
      end

      it "compares unequal objects" do
        expect(util.isDeepStrictEqual({ a: 1 }, { a: 2 })).to be false
      end

      it "compares primitives" do
        expect(util.isDeepStrictEqual(42, 42)).to be true
        expect(util.isDeepStrictEqual(42, 43)).to be false
      end
    end

    describe "types" do
      let(:types) { util["types"] }

      it "detects Date" do
        expect(types.isDate(Boax.eval("new Date()"))).to be true
        expect(types.isDate(Boax.eval("'not a date'"))).to be false
      end

      it "detects RegExp" do
        expect(types.isRegExp(Boax.eval("/test/"))).to be true
        expect(types.isRegExp(Boax.eval("'test'"))).to be false
      end

      it "detects Map" do
        expect(types.isMap(Boax.eval("new Map()"))).to be true
      end

      it "detects Set" do
        expect(types.isSet(Boax.eval("new Set()"))).to be true
      end
    end
  end

  describe "events module" do
    # Grab EventEmitter constructor once to avoid a Boa namespace GC issue
    # where the synthetic module namespace property can get corrupted after
    # heavy object use.
    before(:all) do
      @event_emitter = Boax.import("events")["EventEmitter"]
    end

    it "is importable as 'events'" do
      expect(Boax.import("events")).to be_a(Boax::JsObject)
    end

    it "constructs EventEmitter" do
      ee = @event_emitter.new
      expect(ee).to be_a(Boax::JsObject)
    end

    it "supports on/emit" do
      ee = @event_emitter.new
      Boax.eval("globalThis.__emitResult = null; globalThis.__emitCb = function(v) { globalThis.__emitResult = v; }")
      ee.on("test", Boax.eval("globalThis.__emitCb"))
      ee.emit("test", "hello")
      expect(Boax.eval("globalThis.__emitResult")).to eq("hello")
    end

    it "supports listenerCount" do
      ee = @event_emitter.new
      expect(ee.listenerCount("test")).to eq(0)
      Boax.eval("globalThis.__noop = function() {}")
      ee.on("test", Boax.eval("globalThis.__noop"))
      expect(ee.listenerCount("test")).to eq(1)
    end

    it "supports once (listener fires only once)" do
      ee = @event_emitter.new
      Boax.eval("globalThis.__onceN = 0; globalThis.__onceFn = function() { globalThis.__onceN++; }")
      ee.once("ping", Boax.eval("globalThis.__onceFn"))
      ee.emit("ping")
      ee.emit("ping")
      expect(Boax.eval("globalThis.__onceN")).to eq(1)
    end

    it "supports eventNames" do
      ee = @event_emitter.new
      Boax.eval("globalThis.__fn1 = function() {}; globalThis.__fn2 = function() {}")
      ee.on("a", Boax.eval("globalThis.__fn1"))
      ee.on("b", Boax.eval("globalThis.__fn2"))
      names = ee.eventNames.to_ruby
      expect(names).to contain_exactly("a", "b")
    end
  end

  describe "BoaxObject as JS argument" do
    it "passes BoaxObject back to JS functions" do
      date = Boax.eval("new Date()")
      types = Boax.import("util")["types"]
      expect(types.isDate(date)).to be true
    end
  end
end
