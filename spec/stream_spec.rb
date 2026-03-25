# frozen_string_literal: true

require "boax"

RSpec.describe "stream module", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
  end

  let(:stream) { Boax.import("stream") }

  describe "Readable" do
    it "emits data events on push" do
      Boax.eval("globalThis.__rChunks = []")
      r = stream.Readable.new
      r.on("data", Boax.eval("(function(c) { globalThis.__rChunks.push(c); })"))
      r.push("hello")
      r.push("world")
      expect(Boax.eval("globalThis.__rChunks").to_ruby).to eq(["hello", "world"])
    end

    it "emits end on push(nil)" do
      Boax.eval("globalThis.__rEnded = false")
      r = stream.Readable.new
      r.on("end", Boax.eval("(function() { globalThis.__rEnded = true; })"))
      r.push(nil)
      expect(Boax.eval("globalThis.__rEnded")).to be true
    end

    it "does not emit nil as data" do
      Boax.eval("globalThis.__rData = []")
      r = stream.Readable.new
      r.on("data", Boax.eval("(function(c) { globalThis.__rData.push(c); })"))
      r.push("a")
      r.push(nil)
      expect(Boax.eval("globalThis.__rData").to_ruby).to eq(["a"])
    end
  end

  describe "Writable" do
    it "calls _write for each chunk" do
      Boax.eval("globalThis.__wData = []")
      w = stream.Writable.new({
        write: Boax.eval("(function(chunk, enc, cb) { globalThis.__wData.push(chunk); cb(); })")
      })
      w.write("hello")
      w.write("world")
      expect(Boax.eval("globalThis.__wData").to_ruby).to eq(["hello", "world"])
    end

    it "emits finish on end" do
      Boax.eval("globalThis.__wFinished = false")
      w = stream.Writable.new
      w.on("finish", Boax.eval("(function() { globalThis.__wFinished = true; })"))
      w.end
      expect(Boax.eval("globalThis.__wFinished")).to be true
    end
  end

  describe "Transform" do
    it "transforms chunks" do
      Boax.eval("globalThis.__tData = []")
      t = stream.Transform.new({
        transform: Boax.eval("(function(chunk, enc, cb) { cb(null, chunk.toUpperCase()); })")
      })
      t.on("data", Boax.eval("(function(d) { globalThis.__tData.push(d); })"))
      t.write("hello")
      t.write("world")
      expect(Boax.eval("globalThis.__tData").to_ruby).to eq(["HELLO", "WORLD"])
    end
  end

  describe "PassThrough" do
    it "passes data through unchanged" do
      Boax.eval("globalThis.__ptData = []")
      pt = stream.PassThrough.new
      pt.on("data", Boax.eval("(function(d) { globalThis.__ptData.push(d); })"))
      pt.write("pass")
      pt.write("through")
      expect(Boax.eval("globalThis.__ptData").to_ruby).to eq(["pass", "through"])
    end
  end

  describe "pipe" do
    it "pipes readable to writable" do
      Boax.eval("globalThis.__pipeData = []")
      src = stream.Readable.new
      dest = stream.Writable.new({
        write: Boax.eval("(function(c, e, cb) { globalThis.__pipeData.push(c); cb(); })")
      })
      src.pipe(dest)
      src.push("piped")
      src.push(nil)
      expect(Boax.eval("globalThis.__pipeData").to_ruby).to eq(["piped"])
    end

    it "calls end on destination when source ends" do
      Boax.eval("globalThis.__pipeFinished = false")
      src = stream.Readable.new
      dest = stream.Writable.new
      dest.on("finish", Boax.eval("(function() { globalThis.__pipeFinished = true; })"))
      src.pipe(dest)
      src.push(nil)
      expect(Boax.eval("globalThis.__pipeFinished")).to be true
    end
  end

  describe "pipeline" do
    it "chains streams together" do
      Boax.eval("globalThis.__plResult = []")
      src = stream.PassThrough.new
      xform = stream.Transform.new({
        transform: Boax.eval("(function(c, e, cb) { cb(null, c + '!'); })")
      })
      dest = stream.Writable.new({
        write: Boax.eval("(function(c, e, cb) { globalThis.__plResult.push(c); cb(); })")
      })
      stream.pipeline(src, xform, dest, Boax.eval("(function(err) {})"))
      src.write("hello")
      src.end
      expect(Boax.eval("globalThis.__plResult").to_ruby).to eq(["hello!"])
    end
  end

  describe "finished" do
    it "calls callback when stream ends" do
      Boax.eval("globalThis.__finDone = false")
      r = stream.Readable.new
      stream.finished(r, Boax.eval("(function(err) { globalThis.__finDone = true; })"))
      r.push(nil)
      expect(Boax.eval("globalThis.__finDone")).to be true
    end
  end

  describe "importable" do
    it "works as 'stream'" do
      expect(Boax.import("stream")).to be_a(Boax::JsObject)
    end

    it "works as 'node:stream'" do
      expect(Boax.import("node:stream")).to be_a(Boax::JsObject)
    end
  end
end
