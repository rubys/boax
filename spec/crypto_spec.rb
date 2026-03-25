# frozen_string_literal: true

require "boax"

RSpec.describe "crypto module", skip: !File.directory?(File.join(__dir__, "..", "node_modules")) do
  before(:all) do
    Boax.init(root: File.join(__dir__, ".."))
    Boax.import("buffer") # ensure Buffer is available for randomBytes
    @crypto = Boax.import("crypto")
  end

  let(:crypto) { @crypto }

  describe "createHash" do
    it "hashes with sha256" do
      result = crypto.createHash("sha256").update("hello").digest("hex").to_s
      expect(result).to eq("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
    end

    it "hashes with sha1" do
      result = crypto.createHash("sha1").update("hello").digest("hex").to_s
      expect(result).to eq("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
    end

    it "hashes with md5" do
      result = crypto.createHash("md5").update("hello").digest("hex").to_s
      expect(result).to eq("5d41402abc4b2a76b9719d911017c592")
    end

    it "supports base64 encoding" do
      result = crypto.createHash("sha256").update("hello").digest("base64").to_s
      expect(result).to eq("LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=")
    end

    it "supports chained updates" do
      h = crypto.createHash("sha256")
      h.update("hello")
      h.update(" world")
      result = h.digest("hex").to_s
      expect(result).to eq("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
    end

    it "raises for unsupported algorithm" do
      expect { crypto.createHash("blowfish").digest("hex") }.to raise_error(Boax::Error, /unsupported/)
    end
  end

  describe "createHmac" do
    it "computes HMAC-SHA256" do
      result = crypto.createHmac("sha256", "secret").update("hello").digest("hex").to_s
      expect(result).to eq("88aab3ede8d3adf94d26ab90d3bafd4a2083070c3bcce9c014ee04a443847c0b")
    end

    it "supports chained updates" do
      h = crypto.createHmac("sha256", "key")
      h.update("hello")
      h.update(" world")
      result = h.digest("hex").to_s
      # Just verify it produces a 64-char hex string (SHA-256)
      expect(result.length).to eq(64)
      expect(result).to match(/\A[0-9a-f]+\z/)
    end
  end

  describe "randomBytes" do
    it "returns a Buffer of the specified size" do
      buf = crypto.randomBytes(32)
      expect(buf.length).to eq(32)
    end

    it "returns different values each time" do
      a = crypto.randomBytes(16).toString("hex").to_s
      b = crypto.randomBytes(16).toString("hex").to_s
      expect(a).not_to eq(b)
    end
  end

  describe "randomUUID" do
    it "returns a v4 UUID string" do
      uuid = crypto.randomUUID.to_s
      expect(uuid.length).to eq(36)
      expect(uuid).to match(/\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z/)
    end

    it "returns different values each time" do
      a = crypto.randomUUID.to_s
      b = crypto.randomUUID.to_s
      expect(a).not_to eq(b)
    end
  end

  describe "getHashes" do
    it "returns supported algorithms" do
      hashes = crypto.getHashes.to_ruby
      expect(hashes).to include("sha256", "sha1", "md5")
    end
  end

  describe "importable as node:crypto" do
    it "works with node: prefix" do
      expect(Boax.import("node:crypto")).to be_a(Boax::JsObject)
    end
  end
end
