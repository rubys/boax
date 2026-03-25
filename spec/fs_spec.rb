# frozen_string_literal: true

require "boax"
require "tmpdir"
require "fileutils"

RSpec.describe "fs module", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
    @fs = Boax.import("fs")
    @dir = Dir.mktmpdir("boax-fs-spec")
  end

  after(:all) do
    FileUtils.rm_rf(@dir) if @dir
  end

  let(:fs) { @fs }
  let(:dir) { @dir }

  describe "readFileSync / writeFileSync" do
    it "writes and reads a file" do
      fs.writeFileSync("#{dir}/rw.txt", "hello")
      expect(fs.readFileSync("#{dir}/rw.txt").to_s).to eq("hello")
    end

    it "accepts encoding option" do
      fs.writeFileSync("#{dir}/enc.txt", "utf8 test")
      expect(fs.readFileSync("#{dir}/enc.txt", "utf8").to_s).to eq("utf8 test")
    end

    it "raises ENOENT for missing files" do
      expect { fs.readFileSync("#{dir}/missing.txt") }.to raise_error(Boax::Error, /ENOENT/)
    end
  end

  describe "appendFileSync" do
    it "appends to a file" do
      fs.writeFileSync("#{dir}/append.txt", "line1")
      fs.appendFileSync("#{dir}/append.txt", "\nline2")
      expect(fs.readFileSync("#{dir}/append.txt").to_s).to eq("line1\nline2")
    end

    it "creates the file if it does not exist" do
      fs.appendFileSync("#{dir}/new-append.txt", "created")
      expect(fs.readFileSync("#{dir}/new-append.txt").to_s).to eq("created")
    end
  end

  describe "existsSync" do
    it "returns true for existing files" do
      fs.writeFileSync("#{dir}/exists.txt", "x")
      expect(fs.existsSync("#{dir}/exists.txt")).to be true
    end

    it "returns false for missing files" do
      expect(fs.existsSync("#{dir}/nope.txt")).to be false
    end
  end

  describe "accessSync" do
    it "does not raise for accessible files" do
      fs.writeFileSync("#{dir}/access.txt", "x")
      expect { fs.accessSync("#{dir}/access.txt") }.not_to raise_error
    end

    it "raises for missing files" do
      expect { fs.accessSync("#{dir}/no-access.txt") }.to raise_error(Boax::Error, /ENOENT/)
    end
  end

  describe "mkdirSync" do
    it "creates a directory" do
      fs.mkdirSync("#{dir}/newdir")
      expect(File.directory?("#{dir}/newdir")).to be true
    end

    it "creates directories recursively" do
      fs.mkdirSync("#{dir}/a/b/c", { recursive: true })
      expect(File.directory?("#{dir}/a/b/c")).to be true
    end
  end

  describe "rmdirSync" do
    it "removes an empty directory" do
      fs.mkdirSync("#{dir}/rmdir-test")
      fs.rmdirSync("#{dir}/rmdir-test")
      expect(File.exist?("#{dir}/rmdir-test")).to be false
    end
  end

  describe "rmSync" do
    it "removes a file" do
      fs.writeFileSync("#{dir}/rm-file.txt", "x")
      fs.rmSync("#{dir}/rm-file.txt")
      expect(File.exist?("#{dir}/rm-file.txt")).to be false
    end

    it "removes directories recursively" do
      fs.mkdirSync("#{dir}/rm-tree/child", { recursive: true })
      fs.writeFileSync("#{dir}/rm-tree/child/f.txt", "x")
      fs.rmSync("#{dir}/rm-tree", { recursive: true })
      expect(File.exist?("#{dir}/rm-tree")).to be false
    end

    it "does not raise with force on missing path" do
      expect { fs.rmSync("#{dir}/force-missing", { force: true }) }.not_to raise_error
    end

    it "raises without force on missing path" do
      expect { fs.rmSync("#{dir}/force-missing") }.to raise_error(Boax::Error, /ENOENT/)
    end
  end

  describe "readdirSync" do
    before do
      fs.mkdirSync("#{dir}/readdir-test") rescue nil
      fs.writeFileSync("#{dir}/readdir-test/a.txt", "a")
      fs.writeFileSync("#{dir}/readdir-test/b.txt", "b")
      fs.mkdirSync("#{dir}/readdir-test/sub") rescue nil
    end

    it "lists directory entries as strings" do
      entries = fs.readdirSync("#{dir}/readdir-test").to_ruby
      expect(entries.sort).to eq(["a.txt", "b.txt", "sub"])
    end

    it "lists with file types when withFileTypes is true" do
      entries = fs.readdirSync("#{dir}/readdir-test", { withFileTypes: true })
      # Access as BoaxObjects to check methods
      found_file = false
      found_dir = false
      entries.to_ruby.each do |e|
        name = e["name"] rescue e.name.to_s
        if name == "a.txt"
          found_file = true
        elsif name == "sub"
          found_dir = true
        end
      end
      expect(found_file).to be true
      expect(found_dir).to be true
    end
  end

  describe "statSync" do
    before do
      fs.writeFileSync("#{dir}/stat-test.txt", "hello world")
    end

    it "returns size" do
      stat = fs.statSync("#{dir}/stat-test.txt")
      expect(stat.size).to eq(11)
    end

    it "returns isFile true for files" do
      stat = fs.statSync("#{dir}/stat-test.txt")
      expect(stat.isFile).to be true
      expect(stat.isDirectory).to be false
    end

    it "returns isDirectory true for directories" do
      stat = fs.statSync(dir)
      expect(stat.isDirectory).to be true
      expect(stat.isFile).to be false
    end

    it "has timestamp properties" do
      stat = fs.statSync("#{dir}/stat-test.txt")
      # mtimeMs should be a positive number (ms since epoch)
      expect(stat.mtimeMs.to_i).to be > 0
    end

    it "raises ENOENT for missing files" do
      expect { fs.statSync("#{dir}/no-stat.txt") }.to raise_error(Boax::Error, /ENOENT/)
    end
  end

  describe "lstatSync" do
    it "returns metadata without following symlinks" do
      fs.writeFileSync("#{dir}/lstat-file.txt", "test")
      stat = fs.lstatSync("#{dir}/lstat-file.txt")
      expect(stat.isFile).to be true
    end
  end

  describe "unlinkSync" do
    it "deletes a file" do
      fs.writeFileSync("#{dir}/unlink.txt", "x")
      fs.unlinkSync("#{dir}/unlink.txt")
      expect(File.exist?("#{dir}/unlink.txt")).to be false
    end
  end

  describe "renameSync" do
    it "renames a file" do
      fs.writeFileSync("#{dir}/old-name.txt", "data")
      fs.renameSync("#{dir}/old-name.txt", "#{dir}/new-name.txt")
      expect(File.exist?("#{dir}/old-name.txt")).to be false
      expect(File.read("#{dir}/new-name.txt")).to eq("data")
    end
  end

  describe "copyFileSync" do
    it "copies a file" do
      fs.writeFileSync("#{dir}/src-copy.txt", "copy me")
      fs.copyFileSync("#{dir}/src-copy.txt", "#{dir}/dst-copy.txt")
      expect(File.read("#{dir}/dst-copy.txt")).to eq("copy me")
    end
  end

  describe "chmodSync" do
    it "changes file permissions" do
      fs.writeFileSync("#{dir}/chmod.txt", "x")
      fs.chmodSync("#{dir}/chmod.txt", 0o644)
      mode = File.stat("#{dir}/chmod.txt").mode & 0o777
      expect(mode).to eq(0o644)
    end
  end

  describe "realpathSync" do
    it "resolves to absolute path" do
      fs.writeFileSync("#{dir}/real.txt", "x")
      real = fs.realpathSync("#{dir}/real.txt").to_s
      expect(real).to start_with("/")
      expect(real).to include("real.txt")
    end
  end

  describe "importable as node:fs" do
    it "works with node: prefix" do
      fs2 = Boax.import("node:fs")
      expect(fs2).to be_a(Boax::JsObject)
    end
  end

  describe "callback variants" do
    it "readFile calls callback with (null, data)" do
      fs.writeFileSync("#{dir}/cb-read.txt", "callback data")
      Boax.eval("globalThis.__cbData = null; globalThis.__cbErr = null")
      cb = Boax.eval("(function(err, data) { globalThis.__cbErr = err; globalThis.__cbData = data; })")
      fs.readFile("#{dir}/cb-read.txt", cb)
      expect(Boax.eval("globalThis.__cbData")).to eq("callback data")
      expect(Boax.eval("globalThis.__cbErr")).to be_nil
    end

    it "readFile calls callback with error for missing file" do
      Boax.eval("globalThis.__cbErr2 = null")
      cb = Boax.eval("(function(err) { globalThis.__cbErr2 = err ? err.message || String(err) : null; })")
      fs.readFile("#{dir}/missing-cb.txt", cb)
      expect(Boax.eval("globalThis.__cbErr2").to_s).to include("ENOENT")
    end

    it "writeFile calls callback on success" do
      Boax.eval("globalThis.__wCalled = false")
      cb = Boax.eval("(function(err) { globalThis.__wCalled = !err; })")
      fs.writeFile("#{dir}/cb-write.txt", "via callback", cb)
      expect(Boax.eval("globalThis.__wCalled")).to be true
      expect(File.read("#{dir}/cb-write.txt")).to eq("via callback")
    end

    it "stat calls callback with (null, stats)" do
      fs.writeFileSync("#{dir}/cb-stat.txt", "x")
      Boax.eval("globalThis.__statSize = null")
      cb = Boax.eval("(function(err, stats) { globalThis.__statSize = stats ? stats.size : null; })")
      fs.stat("#{dir}/cb-stat.txt", cb)
      expect(Boax.eval("globalThis.__statSize")).to eq(1)
    end

    it "mkdir calls callback" do
      Boax.eval("globalThis.__mkdirOk = false")
      cb = Boax.eval("(function(err) { globalThis.__mkdirOk = !err; })")
      fs.mkdir("#{dir}/cb-mkdir", cb)
      expect(Boax.eval("globalThis.__mkdirOk")).to be true
      expect(File.directory?("#{dir}/cb-mkdir")).to be true
    end

    it "unlink calls callback" do
      fs.writeFileSync("#{dir}/cb-unlink.txt", "x")
      Boax.eval("globalThis.__unlinkOk = false")
      cb = Boax.eval("(function(err) { globalThis.__unlinkOk = !err; })")
      fs.unlink("#{dir}/cb-unlink.txt", cb)
      expect(Boax.eval("globalThis.__unlinkOk")).to be true
      expect(File.exist?("#{dir}/cb-unlink.txt")).to be false
    end
  end

  describe "fs.promises" do
    let(:promises) { fs["promises"] }

    # Use then! (bang) to call JS Promise.then() since Ruby's
    # Kernel#then would otherwise intercept the call.

    it "readFile returns a promise" do
      fs.writeFileSync("#{dir}/p-read.txt", "promise data")
      p = promises.readFile("#{dir}/p-read.txt")
      expect(p.typeof).to eq("object")
    end

    it "readFile resolves with file contents" do
      fs.writeFileSync("#{dir}/p-read2.txt", "promise data")
      Boax.eval("globalThis.__pResult = null")
      cb = Boax.eval("(function(d) { globalThis.__pResult = d; })")
      promises.readFile("#{dir}/p-read2.txt").then!(cb)
      expect(Boax.eval("globalThis.__pResult")).to eq("promise data")
    end

    it "readFile rejects for missing file" do
      Boax.eval("globalThis.__pErr = null")
      err_cb = Boax.eval("(function(e) { globalThis.__pErr = String(e); })")
      promises.readFile("#{dir}/p-missing.txt").then!(Boax.eval("null"), err_cb)
      expect(Boax.eval("globalThis.__pErr").to_s).to include("ENOENT")
    end

    it "writeFile resolves on success" do
      Boax.eval("globalThis.__pDone = false")
      cb = Boax.eval("(function() { globalThis.__pDone = true; })")
      promises.writeFile("#{dir}/p-write.txt", "promise write").then!(cb)
      expect(Boax.eval("globalThis.__pDone")).to be true
      expect(File.read("#{dir}/p-write.txt")).to eq("promise write")
    end

    it "stat resolves with stats object" do
      fs.writeFileSync("#{dir}/p-stat.txt", "hello")
      Boax.eval("globalThis.__pSize = null")
      cb = Boax.eval("(function(s) { globalThis.__pSize = s.size; })")
      promises.stat("#{dir}/p-stat.txt").then!(cb)
      expect(Boax.eval("globalThis.__pSize")).to eq(5)
    end

    it "mkdir resolves on success" do
      Boax.eval("globalThis.__pMkdir = false")
      cb = Boax.eval("(function() { globalThis.__pMkdir = true; })")
      promises.mkdir("#{dir}/p-mkdir").then!(cb)
      expect(Boax.eval("globalThis.__pMkdir")).to be true
      expect(File.directory?("#{dir}/p-mkdir")).to be true
    end
  end
end
