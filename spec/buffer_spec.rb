# frozen_string_literal: true

require "boax"

RSpec.describe "buffer module", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
    @buffer = Boax.import("buffer")["Buffer"]
  end

  let(:b) { @buffer }

  describe "Buffer.from" do
    it "creates from string" do
      expect(b.from("hello").toString.to_s).to eq("hello")
    end

    it "creates from string with encoding" do
      expect(b.from("68656c6c6f", "hex").toString.to_s).to eq("hello")
      expect(b.from("aGVsbG8=", "base64").toString.to_s).to eq("hello")
    end

    it "creates from array" do
      buf = b.from([104, 101, 108, 108, 111])
      expect(buf.toString.to_s).to eq("hello")
    end
  end

  describe "Buffer.alloc" do
    it "allocates zeroed buffer" do
      buf = b.alloc(4)
      expect(buf.length).to eq(4)
      expect(buf.toString("hex").to_s).to eq("00000000")
    end

    it "fills with a byte value" do
      buf = b.alloc(3, 0xFF)
      expect(buf.toString("hex").to_s).to eq("ffffff")
    end
  end

  describe "Buffer.concat" do
    it "concatenates buffers" do
      result = b.concat([b.from("hello "), b.from("world")])
      expect(result.toString.to_s).to eq("hello world")
    end
  end

  describe "Buffer.isBuffer" do
    it "returns true for buffers" do
      expect(b.isBuffer(b.from("x"))).to be true
    end

    it "returns false for non-buffers" do
      expect(b.isBuffer(Boax.eval("'string'"))).to be false
    end
  end

  describe "Buffer.compare" do
    it "returns 0 for equal buffers" do
      expect(b.compare(b.from("abc"), b.from("abc"))).to eq(0)
    end

    it "returns -1 when first is less" do
      expect(b.compare(b.from("abc"), b.from("abd"))).to eq(-1)
    end

    it "returns 1 when first is greater" do
      expect(b.compare(b.from("abd"), b.from("abc"))).to eq(1)
    end
  end

  describe "Buffer.byteLength" do
    it "returns byte length of string" do
      expect(b.byteLength("hello")).to eq(5)
    end
  end

  describe "encoding roundtrips" do
    it "roundtrips through hex" do
      buf = b.from("hello world")
      hex = buf.toString("hex").to_s
      expect(b.from(hex, "hex").toString.to_s).to eq("hello world")
    end

    it "roundtrips through base64" do
      buf = b.from("hello world")
      b64 = buf.toString("base64").to_s
      expect(b.from(b64, "base64").toString.to_s).to eq("hello world")
    end
  end

  describe "instance methods" do
    it "slice returns a sub-buffer" do
      buf = b.from("hello world")
      expect(buf.slice(0, 5).toString.to_s).to eq("hello")
      expect(buf.slice(6).toString.to_s).to eq("world")
    end

    it "indexOf finds content" do
      buf = b.from("hello world")
      expect(buf.indexOf("world")).to eq(6)
      expect(buf.indexOf("xyz")).to eq(-1)
    end

    it "includes checks presence" do
      buf = b.from("hello world")
      expect(buf.includes("world")).to be true
      expect(buf.includes("xyz")).to be false
    end

    it "copy copies bytes" do
      src = b.from("hello")
      dst = b.alloc(5)
      src.copy(dst)
      expect(dst.toString.to_s).to eq("hello")
    end

    it "equals compares content" do
      expect(b.from("abc").equals(b.from("abc"))).to be true
      expect(b.from("abc").equals(b.from("abd"))).to be false
    end

    it "toJSON returns type and data" do
      json = b.from("hi").toJSON.to_ruby
      expect(json["type"]).to eq("Buffer")
      expect(json["data"]).to eq([104, 105])
    end
  end

  describe "integer read/write" do
    it "reads and writes UInt8" do
      buf = b.alloc(1)
      buf.writeUInt8(42, 0)
      expect(buf.readUInt8(0)).to eq(42)
    end

    it "reads and writes UInt16BE" do
      buf = b.alloc(2)
      buf.writeUInt16BE(0x0102, 0)
      expect(buf.readUInt16BE(0)).to eq(0x0102)
    end

    it "reads and writes UInt32BE" do
      buf = b.alloc(4)
      buf.writeUInt32BE(0x01020304, 0)
      expect(buf.readUInt32BE(0)).to eq(0x01020304)
    end

    it "reads and writes UInt16LE" do
      buf = b.alloc(2)
      buf.writeUInt16LE(0x0102, 0)
      expect(buf.readUInt16LE(0)).to eq(0x0102)
    end
  end

  describe "importable as node:buffer" do
    it "works with node: prefix" do
      expect(Boax.import("node:buffer")).to be_a(Boax::JsObject)
    end
  end
end
