use boa_engine::{
    Context, JsValue,
    js_string,
};

/// Patch Intl.NumberFormat and Intl.DateTimeFormat with missing functionality.
/// Called during context initialization.
pub fn register_intl_polyfills(context: &mut Context) {
    let _ = context.eval(boa_engine::Source::from_bytes(INTL_POLYFILL_JS));
}

const INTL_POLYFILL_JS: &str = r#"
(function() {
    "use strict";

    // --- NumberFormat polyfill for currency and percent styles ---

    var OriginalNumberFormat = Intl.NumberFormat;

    // Currency symbols for common currencies
    var currencySymbols = {
        USD: "$", EUR: "\u20AC", GBP: "\u00A3", JPY: "\u00A5", CNY: "\u00A5",
        KRW: "\u20A9", INR: "\u20B9", RUB: "\u20BD", BRL: "R$", CAD: "CA$",
        AUD: "A$", CHF: "CHF", SEK: "kr", NOK: "kr", DKK: "kr",
        PLN: "z\u0142", CZK: "K\u010D", HUF: "Ft", TRY: "\u20BA", MXN: "MX$",
        SGD: "S$", HKD: "HK$", TWD: "NT$", THB: "\u0E3F", ZAR: "R",
        NZD: "NZ$", ILS: "\u20AA", AED: "AED", SAR: "SAR", QAR: "QAR",
    };

    // Decimal places for currencies (most are 2, these are exceptions)
    var currencyDecimals = { JPY: 0, KRW: 0, CLP: 0, VND: 0, ISK: 0, HUF: 0 };

    function getCurrencyDecimals(currency) {
        return currencyDecimals[currency] !== undefined ? currencyDecimals[currency] : 2;
    }

    // Locale-specific formatting data
    var localeData = {
        "en-US":  { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "en-GB":  { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "en":     { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "de-DE":  { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "de":     { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "fr-FR":  { decimal: ",", group: "\u202F", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "fr":     { decimal: ",", group: "\u202F", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "ja-JP":  { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "ja":     { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "zh-CN":  { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "zh":     { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "ko-KR":  { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "ko":     { decimal: ".", group: ",", currencyPattern: "{symbol}{number}", percentPattern: "{number}%" },
        "pt-BR":  { decimal: ",", group: ".", currencyPattern: "{symbol}\u00A0{number}", percentPattern: "{number}%" },
        "pt":     { decimal: ",", group: ".", currencyPattern: "{symbol}\u00A0{number}", percentPattern: "{number}%" },
        "es-ES":  { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "es":     { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "it-IT":  { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "it":     { decimal: ",", group: ".", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "ru-RU":  { decimal: ",", group: "\u00A0", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
        "ru":     { decimal: ",", group: "\u00A0", currencyPattern: "{number}\u00A0{symbol}", percentPattern: "{number}\u00A0%" },
    };

    function getLocaleData(locale) {
        return localeData[locale] || localeData[locale.split("-")[0]] || localeData["en-US"];
    }

    function formatNumberParts(num, decimals, locData) {
        var negative = num < 0;
        num = Math.abs(num);
        var fixed = num.toFixed(decimals);
        var parts = fixed.split(".");
        var intPart = parts[0];
        var fracPart = parts[1] || "";

        // Add grouping separators
        var grouped = "";
        for (var i = intPart.length - 1, count = 0; i >= 0; i--, count++) {
            if (count > 0 && count % 3 === 0) grouped = locData.group + grouped;
            grouped = intPart[i] + grouped;
        }

        var result = grouped;
        if (fracPart) result += locData.decimal + fracPart;
        if (negative) result = "-" + result;
        return result;
    }

    function PolyfillNumberFormat(locales, options) {
        options = options || {};
        this._locale = Array.isArray(locales) ? locales[0] : (locales || "en-US");
        this._style = options.style || "decimal";
        this._currency = options.currency;
        this._currencyDisplay = options.currencyDisplay || "symbol";
        this._minimumFractionDigits = options.minimumFractionDigits;
        this._maximumFractionDigits = options.maximumFractionDigits;
        this._useGrouping = options.useGrouping !== false;
        this._localeData = getLocaleData(this._locale);

        // For decimal style, try to delegate to the original
        if (this._style === "decimal") {
            try {
                this._original = new OriginalNumberFormat(this._locale, options);
            } catch(e) {}
        }
    }

    PolyfillNumberFormat.prototype.format = function(num) {
        num = Number(num);

        if (this._style === "decimal" && this._original) {
            return this._original.format(num);
        }

        if (this._style === "currency") {
            var negative = num < 0;
            var absNum = Math.abs(num);
            var decimals = this._minimumFractionDigits !== undefined
                ? this._minimumFractionDigits
                : getCurrencyDecimals(this._currency);
            var formatted = formatNumberParts(absNum, decimals, this._localeData);
            var symbol;
            if (this._currencyDisplay === "code") {
                symbol = this._currency;
            } else if (this._currencyDisplay === "name") {
                symbol = this._currency;
            } else {
                symbol = currencySymbols[this._currency] || this._currency;
            }
            var result = this._localeData.currencyPattern
                .replace("{symbol}", symbol)
                .replace("{number}", formatted);
            return negative ? "-" + result : result;
        }

        if (this._style === "percent") {
            var pctNum = num * 100;
            var pctDecimals = this._minimumFractionDigits !== undefined
                ? this._minimumFractionDigits : 0;
            var formatted = formatNumberParts(pctNum, pctDecimals, this._localeData);
            return this._localeData.percentPattern.replace("{number}", formatted);
        }

        // Fallback: decimal
        var dec = this._maximumFractionDigits !== undefined ? this._maximumFractionDigits : 3;
        return formatNumberParts(num, dec, this._localeData);
    };

    PolyfillNumberFormat.prototype.formatToParts = function(num) {
        var str = this.format(num);
        return [{ type: "literal", value: str }];
    };

    PolyfillNumberFormat.prototype.resolvedOptions = function() {
        return {
            locale: this._locale,
            style: this._style,
            currency: this._currency,
            currencyDisplay: this._currencyDisplay,
            minimumFractionDigits: this._minimumFractionDigits || 0,
            maximumFractionDigits: this._maximumFractionDigits || 3,
            useGrouping: this._useGrouping,
        };
    };

    PolyfillNumberFormat.supportedLocalesOf = function(locales) {
        return Array.isArray(locales) ? locales : [locales];
    };

    // Replace Intl.NumberFormat
    Intl.NumberFormat = PolyfillNumberFormat;

    // --- DateTimeFormat polyfill ---

    var monthNames = {
        "en": ["January","February","March","April","May","June","July","August","September","October","November","December"],
        "de": ["Januar","Februar","M\u00E4rz","April","Mai","Juni","Juli","August","September","Oktober","November","Dezember"],
        "fr": ["janvier","f\u00E9vrier","mars","avril","mai","juin","juillet","ao\u00FBt","septembre","octobre","novembre","d\u00E9cembre"],
        "es": ["enero","febrero","marzo","abril","mayo","junio","julio","agosto","septiembre","octubre","noviembre","diciembre"],
        "ja": ["1\u6708","2\u6708","3\u6708","4\u6708","5\u6708","6\u6708","7\u6708","8\u6708","9\u6708","10\u6708","11\u6708","12\u6708"],
    };

    var shortMonthNames = {
        "en": ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"],
        "de": ["Jan.","Feb.","M\u00E4r.","Apr.","Mai","Jun.","Jul.","Aug.","Sep.","Okt.","Nov.","Dez."],
        "fr": ["janv.","f\u00E9vr.","mars","avr.","mai","juin","juil.","ao\u00FBt","sept.","oct.","nov.","d\u00E9c."],
        "es": ["ene.","feb.","mar.","abr.","may.","jun.","jul.","ago.","sept.","oct.","nov.","dic."],
    };

    var dayNames = {
        "en": ["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"],
        "de": ["Sonntag","Montag","Dienstag","Mittwoch","Donnerstag","Freitag","Samstag"],
        "fr": ["dimanche","lundi","mardi","mercredi","jeudi","vendredi","samedi"],
        "es": ["domingo","lunes","martes","mi\u00E9rcoles","jueves","viernes","s\u00E1bado"],
    };

    var shortDayNames = {
        "en": ["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],
    };

    function getLang(locale) {
        return locale.split("-")[0];
    }

    function pad2(n) { return n < 10 ? "0" + n : "" + n; }

    function PolyfillDateTimeFormat(locales, options) {
        options = options || {};
        this._locale = Array.isArray(locales) ? locales[0] : (locales || "en-US");
        this._lang = getLang(this._locale);
        this._options = options;
        this._weekday = options.weekday;
        this._year = options.year;
        this._month = options.month;
        this._day = options.day;
        this._hour = options.hour;
        this._minute = options.minute;
        this._second = options.second;
        this._hour12 = options.hour12;
        this._timeZone = options.timeZone;
        this._dateStyle = options.dateStyle;
        this._timeStyle = options.timeStyle;

        // If no options given, default to date-only
        if (!this._weekday && !this._year && !this._month && !this._day &&
            !this._hour && !this._minute && !this._second &&
            !this._dateStyle && !this._timeStyle) {
            this._year = "numeric";
            this._month = "numeric";
            this._day = "numeric";
        }
    }

    PolyfillDateTimeFormat.prototype.format = function(date) {
        if (typeof date === "number") date = new Date(date);
        if (!date) date = new Date();

        var parts = [];
        var lang = this._lang;

        // Handle dateStyle shorthand
        if (this._dateStyle) {
            return this._formatDateStyle(date);
        }

        // Weekday
        if (this._weekday) {
            var names = this._weekday === "long" ? (dayNames[lang] || dayNames["en"]) :
                (shortDayNames[lang] || shortDayNames["en"]);
            parts.push(names[date.getDay()]);
        }

        // Date parts
        var dateParts = [];
        if (this._month) {
            var m = date.getMonth();
            if (this._month === "long") {
                dateParts.push((monthNames[lang] || monthNames["en"])[m]);
            } else if (this._month === "short") {
                dateParts.push((shortMonthNames[lang] || shortMonthNames["en"] || monthNames["en"].map(function(n){return n.substring(0,3)}))[m]);
            } else if (this._month === "2-digit") {
                dateParts.push(pad2(m + 1));
            } else {
                dateParts.push("" + (m + 1));
            }
        }
        if (this._day) {
            var d = date.getDate();
            dateParts.push(this._day === "2-digit" ? pad2(d) : "" + d);
        }
        if (this._year) {
            var y = date.getFullYear();
            dateParts.push(this._year === "2-digit" ? ("" + y).slice(-2) : "" + y);
        }

        if (dateParts.length > 0) {
            // Format order depends on locale
            if (lang === "en") {
                // en-US: month/day/year
                parts.push(dateParts.join("/"));
            } else if (lang === "de" || lang === "fr" || lang === "es" || lang === "it" || lang === "pt" || lang === "ru") {
                // day.month.year or day/month/year
                if (this._month === "long" || this._month === "short") {
                    parts.push(dateParts.join(" "));
                } else {
                    parts.push([dateParts[1], dateParts[0], dateParts[2]].filter(Boolean).join("/"));
                }
            } else if (lang === "ja" || lang === "zh" || lang === "ko") {
                // year/month/day
                parts.push([dateParts[2], dateParts[0], dateParts[1]].filter(Boolean).join("/"));
            } else {
                parts.push(dateParts.join("/"));
            }
        }

        // Time parts
        if (this._hour || this._minute || this._second) {
            var timeParts = [];
            var h = date.getHours();
            var ampm = "";
            var use12 = this._hour12 !== undefined ? this._hour12 : (lang === "en");
            if (use12) {
                ampm = h >= 12 ? " PM" : " AM";
                h = h % 12 || 12;
            }
            if (this._hour) timeParts.push(this._hour === "2-digit" ? pad2(h) : "" + h);
            if (this._minute) timeParts.push(pad2(date.getMinutes()));
            if (this._second) timeParts.push(pad2(date.getSeconds()));
            parts.push(timeParts.join(":") + ampm);
        }

        return parts.join(", ");
    };

    PolyfillDateTimeFormat.prototype._formatDateStyle = function(date) {
        var lang = this._lang;
        var y = date.getFullYear();
        var m = date.getMonth();
        var d = date.getDate();

        if (this._dateStyle === "full") {
            var dayName = (dayNames[lang] || dayNames["en"])[date.getDay()];
            var monthName = (monthNames[lang] || monthNames["en"])[m];
            if (lang === "en") return dayName + ", " + monthName + " " + d + ", " + y;
            return dayName + ", " + d + " " + monthName + " " + y;
        }
        if (this._dateStyle === "long") {
            var monthName = (monthNames[lang] || monthNames["en"])[m];
            if (lang === "en") return monthName + " " + d + ", " + y;
            return d + " " + monthName + " " + y;
        }
        if (this._dateStyle === "medium") {
            var monthName = (shortMonthNames[lang] || shortMonthNames["en"] || monthNames["en"].map(function(n){return n.substring(0,3)}))[m];
            if (lang === "en") return monthName + " " + d + ", " + y;
            return d + " " + monthName + " " + y;
        }
        // short
        if (lang === "en") return (m + 1) + "/" + d + "/" + (("" + y).length > 2 ? y : y);
        return d + "/" + (m + 1) + "/" + y;
    };

    PolyfillDateTimeFormat.prototype.formatToParts = function(date) {
        var str = this.format(date);
        return [{ type: "literal", value: str }];
    };

    PolyfillDateTimeFormat.prototype.resolvedOptions = function() {
        return {
            locale: this._locale,
            weekday: this._weekday,
            year: this._year,
            month: this._month,
            day: this._day,
            hour: this._hour,
            minute: this._minute,
            second: this._second,
        };
    };

    PolyfillDateTimeFormat.supportedLocalesOf = function(locales) {
        return Array.isArray(locales) ? locales : [locales];
    };

    // Replace Intl.DateTimeFormat
    Intl.DateTimeFormat = PolyfillDateTimeFormat;
})()
"#;
