# frozen_string_literal: true

require "boax"

RSpec.describe "Additional Node.js modules", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
  end

  describe "process module" do
    let(:process_mod) { Boax.import("process") }

    it "has platform" do
      expect(process_mod.platform.to_s).to match(/darwin|linux|win32/)
    end

    it "has arch" do
      expect(process_mod.arch.to_s).to match(/x64|arm64|ia32/)
    end

    it "has cwd()" do
      expect(process_mod.cwd.to_s).to start_with("/")
    end

    it "has env with HOME" do
      expect(process_mod["env"]["HOME"].to_s).not_to be_empty
    end

    it "has pid" do
      expect(process_mod.pid).to be > 0
    end

    it "has version" do
      expect(process_mod.version.to_s).to start_with("v")
    end

    it "nextTick calls callback immediately" do
      Boax.eval("globalThis.__tickCalled = false")
      process_mod.nextTick(Boax.eval("(function() { globalThis.__tickCalled = true; })"))
      expect(Boax.eval("globalThis.__tickCalled")).to be true
    end

    it "is importable as node:process" do
      expect(Boax.import("node:process").platform.to_s).to match(/darwin|linux|win32/)
    end
  end

  describe "os module" do
    let(:os) { Boax.import("os") }

    it "has platform()" do
      expect(os.platform.to_s).to match(/darwin|linux|win32/)
    end

    it "has arch()" do
      expect(os.arch.to_s).to match(/x64|arm64|ia32/)
    end

    it "has tmpdir()" do
      expect(os.tmpdir.to_s).not_to be_empty
    end

    it "has homedir()" do
      expect(os.homedir.to_s).not_to be_empty
    end

    it "has hostname()" do
      expect(os.hostname.to_s).not_to be_empty
    end

    it "has EOL" do
      expect(os["EOL"].to_s).to eq("\n")
    end

    it "has endianness()" do
      expect(os.endianness.to_s).to match(/LE|BE/)
    end

    it "has cpus() returning array" do
      cpus = os.cpus.to_ruby
      expect(cpus).to be_an(Array)
      expect(cpus.length).to be > 0
    end

    it "has userInfo()" do
      info = os.userInfo
      expect(info["username"].to_s).not_to be_empty
    end

    it "has type()" do
      expect(os.type.to_s).to match(/Darwin|Linux|Windows_NT/)
    end
  end

  describe "querystring module" do
    let(:qs) { Boax.import("querystring") }

    it "parses query strings" do
      result = qs.parse("foo=bar&baz=qux").to_ruby
      expect(result).to eq({ "foo" => "bar", "baz" => "qux" })
    end

    it "handles repeated keys as arrays" do
      result = qs.parse("a=1&a=2&a=3").to_ruby
      expect(result["a"]).to eq(["1", "2", "3"])
    end

    it "decodes percent-encoded values" do
      result = qs.parse("name=hello+world&key=a%20b").to_ruby
      expect(result["name"]).to eq("hello world")
      expect(result["key"]).to eq("a b")
    end

    it "stringifies objects" do
      result = qs.stringify({ a: 1, b: "hello" }).to_s
      expect(result).to include("a=1")
      expect(result).to include("b=hello")
    end

    it "encodes special characters" do
      result = qs.stringify({ q: "hello world" }).to_s
      expect(result).to eq("q=hello+world")
    end

    it "supports custom separators" do
      result = qs.parse("a:1;b:2", ";", ":").to_ruby
      expect(result).to eq({ "a" => "1", "b" => "2" })
    end
  end

  describe "string_decoder module" do
    it "decodes strings" do
      sd_mod = Boax.import("string_decoder")
      sd = sd_mod.StringDecoder.new("utf8")
      expect(sd.write("hello").to_s).to eq("hello")
    end

    it "is importable as node:string_decoder" do
      expect(Boax.import("node:string_decoder")).to be_a(Boax::JsObject)
    end
  end

  describe "assert module" do
    let(:assert_mod) { Boax.import("assert") }

    it "ok passes for truthy" do
      expect { assert_mod.ok(true) }.not_to raise_error
      expect { assert_mod.ok(1) }.not_to raise_error
      expect { assert_mod.ok("x") }.not_to raise_error
    end

    it "ok fails for falsy" do
      expect { assert_mod.ok(false) }.to raise_error(Boax::Error, /AssertionError/)
      expect { assert_mod.ok(0) }.to raise_error(Boax::Error, /AssertionError/)
      expect { assert_mod.ok(nil) }.to raise_error(Boax::Error, /AssertionError/)
    end

    it "strictEqual passes for identical values" do
      expect { assert_mod.strictEqual(1, 1) }.not_to raise_error
      expect { assert_mod.strictEqual("a", "a") }.not_to raise_error
    end

    it "strictEqual fails for different values" do
      expect { assert_mod.strictEqual(1, 2) }.to raise_error(Boax::Error)
    end

    it "deepEqual compares objects" do
      expect { assert_mod.deepEqual({ a: 1 }, { a: 1 }) }.not_to raise_error
      expect { assert_mod.deepEqual({ a: 1 }, { a: 2 }) }.to raise_error(Boax::Error)
    end

    it "throws checks that function throws" do
      fn = Boax.eval("(function() { throw new Error('boom'); })")
      expect { assert_mod.throws(fn) }.not_to raise_error
    end

    it "throws fails when function doesn't throw" do
      fn = Boax.eval("(function() {})")
      expect { assert_mod.throws(fn) }.to raise_error(Boax::Error)
    end

    it "fail always fails" do
      expect { assert_mod.fail("nope") }.to raise_error(Boax::Error, /nope/)
    end
  end

  describe "url module" do
    let(:url) { Boax.import("url") }

    it "parses URLs" do
      parsed = url.parse("https://example.com/path?q=1#hash")
      expect(parsed["pathname"].to_s).to include("/path")
    end

    it "fileURLToPath converts file:// URLs" do
      expect(url.fileURLToPath("file:///tmp/test.txt").to_s).to eq("/tmp/test.txt")
    end

    it "is importable as node:url" do
      expect(Boax.import("node:url")).to be_a(Boax::JsObject)
    end
  end
end
