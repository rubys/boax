# frozen_string_literal: true

require "boax"

RSpec.describe "Intl polyfills" do
  let(:intl) { Boax.import("Intl") }

  describe "NumberFormat" do
    describe "currency style" do
      it "formats USD" do
        nf = intl.NumberFormat.new("en-US", { style: "currency", currency: "USD" })
        expect(nf.format(1234.56).to_s).to eq("$1,234.56")
      end

      it "formats EUR in de-DE" do
        nf = intl.NumberFormat.new("de-DE", { style: "currency", currency: "EUR" })
        result = nf.format(1234.56).to_s
        expect(result).to include("1.234,56")
        expect(result).to include("\u20AC") # €
      end

      it "formats JPY (no decimals)" do
        nf = intl.NumberFormat.new("ja-JP", { style: "currency", currency: "JPY" })
        expect(nf.format(1234).to_s).to eq("\u00A51,234")
      end

      it "formats negative amounts" do
        nf = intl.NumberFormat.new("en-US", { style: "currency", currency: "USD" })
        expect(nf.format(-42.50).to_s).to eq("-$42.50")
      end

      it "formats GBP" do
        nf = intl.NumberFormat.new("en-GB", { style: "currency", currency: "GBP" })
        expect(nf.format(99.99).to_s).to eq("\u00A399.99")
      end
    end

    describe "percent style" do
      it "formats percentages" do
        nf = intl.NumberFormat.new("en-US", { style: "percent" })
        expect(nf.format(0.42).to_s).to eq("42%")
      end

      it "formats 100%" do
        nf = intl.NumberFormat.new("en-US", { style: "percent" })
        expect(nf.format(1.0).to_s).to eq("100%")
      end
    end

    describe "decimal style" do
      it "delegates to Boa for basic formatting" do
        nf = intl.NumberFormat.new("en-US")
        expect(nf.format(1234567.89).to_s).to eq("1,234,567.89")
      end
    end

    describe "resolvedOptions" do
      it "returns options" do
        nf = intl.NumberFormat.new("en-US", { style: "currency", currency: "USD" })
        opts = nf.resolvedOptions
        expect(opts["style"].to_s).to eq("currency")
        expect(opts["currency"].to_s).to eq("USD")
      end
    end
  end

  describe "DateTimeFormat" do
    # Use a fixed date for deterministic tests
    let(:date) { Boax.eval("new Date(2024, 0, 15, 14, 30, 45)") }

    describe "default (numeric date)" do
      it "formats en-US date" do
        dtf = intl.DateTimeFormat.new("en-US")
        result = dtf.format(date).to_s
        expect(result).to include("1")  # month
        expect(result).to include("15") # day
        expect(result).to include("2024") # year
      end
    end

    describe "dateStyle" do
      it "formats full date" do
        dtf = intl.DateTimeFormat.new("en-US", { dateStyle: "full" })
        result = dtf.format(date).to_s
        expect(result).to include("Monday")
        expect(result).to include("January")
        expect(result).to include("15")
        expect(result).to include("2024")
      end

      it "formats long date" do
        dtf = intl.DateTimeFormat.new("en-US", { dateStyle: "long" })
        result = dtf.format(date).to_s
        expect(result).to include("January")
        expect(result).to include("15")
        expect(result).to include("2024")
      end

      it "formats short date" do
        dtf = intl.DateTimeFormat.new("en-US", { dateStyle: "short" })
        result = dtf.format(date).to_s
        expect(result).to include("1/15/2024")
      end
    end

    describe "time formatting" do
      it "formats time with hour12" do
        dtf = intl.DateTimeFormat.new("en-US", {
          hour: "numeric", minute: "2-digit", hour12: true
        })
        result = dtf.format(date).to_s
        expect(result).to include("2")
        expect(result).to include("30")
        expect(result).to include("PM")
      end
    end

    describe "weekday" do
      it "formats with long weekday" do
        dtf = intl.DateTimeFormat.new("en-US", { weekday: "long" })
        expect(dtf.format(date).to_s).to eq("Monday")
      end
    end

    describe "locales" do
      it "formats German dates" do
        dtf = intl.DateTimeFormat.new("de-DE", { dateStyle: "long" })
        result = dtf.format(date).to_s
        expect(result).to include("Januar")
        expect(result).to include("15")
      end

      it "formats French dates" do
        dtf = intl.DateTimeFormat.new("fr-FR", { dateStyle: "long" })
        result = dtf.format(date).to_s
        expect(result).to include("janvier")
      end
    end
  end
end
