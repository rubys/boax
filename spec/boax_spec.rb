# frozen_string_literal: true

require "boax"

RSpec.describe Boax do
  describe ".eval" do
    it "evaluates integer expressions" do
      expect(Boax.eval("1 + 2")).to eq(3)
    end

    it "evaluates float expressions" do
      expect(Boax.eval("Math.PI")).to be_within(0.0001).of(3.1415)
    end

    it "evaluates string expressions" do
      expect(Boax.eval("'hello' + ' world'")).to eq("hello world")
    end

    it "returns nil for undefined" do
      expect(Boax.eval("undefined")).to be_nil
    end

    it "returns nil for null" do
      expect(Boax.eval("null")).to be_nil
    end

    it "returns booleans" do
      expect(Boax.eval("true")).to be true
      expect(Boax.eval("false")).to be false
    end

    it "returns BoaxObject for objects" do
      result = Boax.eval("({a: 1})")
      expect(result).to be_a(Boax::JsObject)
    end

    it "raises Boax::Error on JS exceptions" do
      expect { Boax.eval("throw new Error('boom')") }.to raise_error(Boax::Error, "boom")
    end

    it "raises Boax::Error on syntax errors" do
      expect { Boax.eval("if (") }.to raise_error(Boax::Error)
    end
  end

  describe ".import" do
    it "imports Math" do
      math = Boax.import("Math")
      expect(math).to be_a(Boax::JsObject)
      expect(math.PI).to be_within(0.0001).of(3.1415)
    end

    it "imports JSON" do
      json = Boax.import("JSON")
      expect(json.stringify({ a: 1 })).to eq('{"a":1}')
    end

    it "raises for unknown globals" do
      expect { Boax.import("NoSuchThing") }.to raise_error(Boax::Error)
    end
  end

  describe Boax::JsObject do
    describe "#method_missing" do
      it "calls JS methods" do
        math = Boax.import("Math")
        expect(math.sqrt(144)).to eq(12)
        expect(math.max(1, 5, 3)).to eq(5)
      end

      it "accesses JS properties" do
        math = Boax.import("Math")
        expect(math.PI).to be_a(Float)
      end

      it "raises for undefined properties" do
        math = Boax.import("Math")
        expect { math.nonexistent_method }.to raise_error(Boax::Error, /undefined JS property/)
      end
    end

    describe "#new (construct)" do
      it "constructs JS objects" do
        date_ctor = Boax.eval("Date")
        d = date_ctor.new(2024, 0, 15)
        expect(d.getFullYear).to eq(2024)
      end

      it "raises for non-constructors" do
        math = Boax.import("Math")
        expect { math.new }.to raise_error(Boax::Error, /not a JS constructor/)
      end
    end

    describe "#to_ruby" do
      it "converts arrays deeply" do
        result = Boax.eval("[1, [2, 3], 'hello']").to_ruby
        expect(result).to eq([1, [2, 3], "hello"])
      end

      it "converts plain objects to hashes" do
        result = Boax.eval("({a: 1, b: 'two'})").to_ruby
        expect(result).to eq({ "a" => 1, "b" => "two" })
      end

      it "converts nested structures" do
        result = Boax.eval("({a: [1, {b: 2}]})").to_ruby
        expect(result).to eq({ "a" => [1, { "b" => 2 }] })
      end
    end

    describe "#[] and #[]=" do
      it "reads properties by string key" do
        obj = Boax.eval("({foo: 'bar'})")
        expect(obj["foo"]).to eq("bar")
      end

      it "reads properties by integer key" do
        arr = Boax.eval("[10, 20, 30]")
        expect(arr[1]).to eq(20)
      end
    end

    describe "#to_s" do
      it "returns JS string representation" do
        math = Boax.import("Math")
        expect(math.to_s).to eq("[object Math]")
      end
    end

    describe "#inspect" do
      it "includes class name" do
        math = Boax.import("Math")
        expect(math.inspect).to include("Boax::JsObject")
      end
    end

    describe "#respond_to?" do
      it "returns true for existing JS properties" do
        math = Boax.import("Math")
        expect(math.respond_to?(:sqrt)).to be true
        expect(math.respond_to?(:PI)).to be true
      end

      it "returns false for missing JS properties" do
        math = Boax.import("Math")
        expect(math.respond_to?(:nonexistent)).to be false
      end
    end

    describe "type conversions (Ruby → JS → Ruby)" do
      let(:json) { Boax.import("JSON") }

      it "handles nil" do
        expect(Boax.eval("undefined")).to be_nil
      end

      it "handles integers" do
        expect(Boax.eval("42")).to eq(42)
        expect(Boax.eval("42")).to be_a(Integer)
      end

      it "handles floats" do
        expect(Boax.eval("3.14")).to be_within(0.001).of(3.14)
        expect(Boax.eval("3.14")).to be_a(Float)
      end

      it "handles strings" do
        expect(Boax.eval("'hello'")).to eq("hello")
      end

      it "round-trips hashes through JSON" do
        input = { name: "test", count: 42 }
        parsed = json.parse(json.stringify(input))
        expect(parsed.to_ruby).to eq({ "name" => "test", "count" => 42 })
      end
    end
  end
end
