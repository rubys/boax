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
end
