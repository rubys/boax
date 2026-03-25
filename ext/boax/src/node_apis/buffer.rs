use boa_engine::{
    Context, JsString, JsValue, Module,
    js_string,
    module::SyntheticModuleInitializer,
};

const EXPORT_NAMES: &[&str] = &["default", "Buffer"];

pub fn create_module(context: &mut Context) -> Module {
    // Register Buffer on the global object first, then the synthetic module
    // just re-exports it. This avoids a Boa VM panic when eval'ing large
    // JS inside synthetic module initialization.
    ensure_buffer_global(context);

    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_module),
        None, None, context,
    )
}

fn ensure_buffer_global(context: &mut Context) {
    let global = context.global_object();
    let existing = global.get(js_string!("Buffer"), context).unwrap_or(JsValue::undefined());
    if !existing.is_undefined() {
        return; // Already registered
    }

    let _ = context.eval(boa_engine::Source::from_bytes(BUFFER_JS));
}

fn init_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let global = context.global_object();
    let buffer_ctor = global.get(js_string!("Buffer"), context)?;

    module.set_export(&js_string!("default"), buffer_ctor.clone())?;
    module.set_export(&js_string!("Buffer"), buffer_ctor)?;
    Ok(())
}

const BUFFER_JS: &str = r#"
(function() {
    "use strict";

    var _encoder = new TextEncoder();
    var _decoder = new TextDecoder();

    // --- Encoding helpers ---

    function hexEncode(bytes) {
        var hex = '';
        for (var i = 0; i < bytes.length; i++) {
            hex += (bytes[i] < 16 ? '0' : '') + bytes[i].toString(16);
        }
        return hex;
    }

    function hexDecode(str) {
        str = str.replace(/\s/g, '');
        if (str.length % 2 !== 0) str = '0' + str;
        var bytes = new Uint8Array(str.length / 2);
        for (var i = 0; i < bytes.length; i++) {
            bytes[i] = parseInt(str.substring(i * 2, i * 2 + 2), 16);
        }
        return bytes;
    }

    var b64chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    var b64lookup = new Uint8Array(256);
    for (var i = 0; i < b64chars.length; i++) b64lookup[b64chars.charCodeAt(i)] = i;

    function base64Encode(bytes) {
        var result = '';
        var len = bytes.length;
        for (var i = 0; i < len; i += 3) {
            var b0 = bytes[i];
            var b1 = i + 1 < len ? bytes[i + 1] : 0;
            var b2 = i + 2 < len ? bytes[i + 2] : 0;
            result += b64chars[b0 >> 2];
            result += b64chars[((b0 & 3) << 4) | (b1 >> 4)];
            result += (i + 1 < len) ? b64chars[((b1 & 15) << 2) | (b2 >> 6)] : '=';
            result += (i + 2 < len) ? b64chars[b2 & 63] : '=';
        }
        return result;
    }

    function base64Decode(str) {
        str = str.replace(/[=\s]/g, '');
        var len = str.length;
        var bytes = new Uint8Array(Math.floor(len * 3 / 4));
        var j = 0;
        for (var i = 0; i < len; i += 4) {
            var a = b64lookup[str.charCodeAt(i)];
            var b = b64lookup[str.charCodeAt(i + 1)];
            var c = i + 2 < len ? b64lookup[str.charCodeAt(i + 2)] : 0;
            var d = i + 3 < len ? b64lookup[str.charCodeAt(i + 3)] : 0;
            bytes[j++] = (a << 2) | (b >> 4);
            if (i + 2 < len) bytes[j++] = ((b & 15) << 4) | (c >> 2);
            if (i + 3 < len) bytes[j++] = ((c & 3) << 6) | d;
        }
        return bytes.slice(0, j);
    }

    function base64urlEncode(bytes) {
        return base64Encode(bytes).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
    }

    function base64urlDecode(str) {
        str = str.replace(/-/g, '+').replace(/_/g, '/');
        while (str.length % 4) str += '=';
        return base64Decode(str);
    }

    function latin1Encode(str) {
        var bytes = new Uint8Array(str.length);
        for (var i = 0; i < str.length; i++) bytes[i] = str.charCodeAt(i) & 0xFF;
        return bytes;
    }

    function latin1Decode(bytes) {
        var str = '';
        for (var i = 0; i < bytes.length; i++) str += String.fromCharCode(bytes[i]);
        return str;
    }

    function stringToBytes(str, encoding) {
        encoding = (encoding || 'utf8').toLowerCase();
        switch (encoding) {
            case 'utf8': case 'utf-8': return _encoder.encode(str);
            case 'hex': return hexDecode(str);
            case 'base64': return base64Decode(str);
            case 'base64url': return base64urlDecode(str);
            case 'latin1': case 'binary': return latin1Encode(str);
            case 'ascii':
                var b = new Uint8Array(str.length);
                for (var i = 0; i < str.length; i++) b[i] = str.charCodeAt(i) & 0x7F;
                return b;
            default: return _encoder.encode(str);
        }
    }

    function bytesToString(bytes, encoding) {
        encoding = (encoding || 'utf8').toLowerCase();
        switch (encoding) {
            case 'utf8': case 'utf-8': return _decoder.decode(bytes);
            case 'hex': return hexEncode(bytes);
            case 'base64': return base64Encode(bytes);
            case 'base64url': return base64urlEncode(bytes);
            case 'latin1': case 'binary': return latin1Decode(bytes);
            case 'ascii':
                var s = '';
                for (var i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i] & 0x7F);
                return s;
            default: return _decoder.decode(bytes);
        }
    }

    function Buffer(arg, encodingOrOffset, length) {
        if (typeof arg === 'number') {
            this._bytes = new Uint8Array(arg);
        } else if (typeof arg === 'string') {
            this._bytes = stringToBytes(arg, encodingOrOffset);
        } else if (arg instanceof Uint8Array) {
            this._bytes = new Uint8Array(arg);
        } else if (arg instanceof ArrayBuffer) {
            var offset = encodingOrOffset || 0;
            var len = length !== undefined ? length : arg.byteLength - offset;
            this._bytes = new Uint8Array(arg, offset, len);
        } else if (arg && arg._bytes) {
            this._bytes = new Uint8Array(arg._bytes);
        } else if (Array.isArray(arg)) {
            this._bytes = new Uint8Array(arg);
        } else {
            this._bytes = new Uint8Array(0);
        }
        this.length = this._bytes.length;
    }

    Buffer.from = function(arg, encodingOrOffset, length) {
        if (typeof arg === 'string') return new Buffer(arg, encodingOrOffset);
        if (arg instanceof ArrayBuffer) return new Buffer(arg, encodingOrOffset, length);
        if (arg && arg._bytes) return new Buffer(arg);
        if (Array.isArray(arg) || arg instanceof Uint8Array) return new Buffer(arg);
        if (arg && typeof arg[Symbol.iterator] === 'function') return new Buffer(Array.from(arg));
        throw new TypeError('The first argument must be a string, Buffer, ArrayBuffer, Array, or array-like object');
    };

    Buffer.alloc = function(size, fill, encoding) {
        var buf = new Buffer(size);
        if (fill !== undefined) {
            if (typeof fill === 'number') {
                buf._bytes.fill(fill);
            } else if (typeof fill === 'string') {
                var fillBytes = stringToBytes(fill, encoding);
                for (var i = 0; i < size; i++) buf._bytes[i] = fillBytes[i % fillBytes.length];
            }
        }
        return buf;
    };

    Buffer.allocUnsafe = function(size) { return new Buffer(size); };
    Buffer.allocUnsafeSlow = Buffer.allocUnsafe;

    Buffer.concat = function(list, totalLength) {
        if (!Array.isArray(list) || list.length === 0) return Buffer.alloc(0);
        if (totalLength === undefined) {
            totalLength = 0;
            for (var i = 0; i < list.length; i++) totalLength += list[i].length;
        }
        var result = Buffer.alloc(totalLength);
        var offset = 0;
        for (var i = 0; i < list.length; i++) {
            var bytes = list[i]._bytes || list[i];
            result._bytes.set(bytes, offset);
            offset += bytes.length;
            if (offset >= totalLength) break;
        }
        return result;
    };

    Buffer.isBuffer = function(obj) { return obj instanceof Buffer; };

    Buffer.isEncoding = function(encoding) {
        switch ((encoding || '').toLowerCase()) {
            case 'utf8': case 'utf-8': case 'hex': case 'base64':
            case 'base64url': case 'latin1': case 'binary': case 'ascii':
                return true;
            default: return false;
        }
    };

    Buffer.byteLength = function(string, encoding) {
        if (typeof string !== 'string') return string.length || 0;
        return stringToBytes(string, encoding).length;
    };

    Buffer.compare = function(a, b) {
        var ab = a._bytes || a, bb = b._bytes || b;
        var len = Math.min(ab.length, bb.length);
        for (var i = 0; i < len; i++) {
            if (ab[i] < bb[i]) return -1;
            if (ab[i] > bb[i]) return 1;
        }
        return ab.length < bb.length ? -1 : ab.length > bb.length ? 1 : 0;
    };

    Buffer.prototype.toString = function(encoding, start, end) {
        start = start || 0;
        end = end !== undefined ? end : this.length;
        return bytesToString(this._bytes.slice(start, end), encoding);
    };

    Buffer.prototype.toJSON = function() {
        return { type: 'Buffer', data: Array.from(this._bytes) };
    };

    Buffer.prototype.equals = function(other) { return Buffer.compare(this, other) === 0; };

    Buffer.prototype.compare = function(target) { return Buffer.compare(this, target); };

    Buffer.prototype.copy = function(target, targetStart, sourceStart, sourceEnd) {
        targetStart = targetStart || 0;
        sourceStart = sourceStart || 0;
        sourceEnd = sourceEnd !== undefined ? sourceEnd : this.length;
        var bytes = this._bytes.slice(sourceStart, sourceEnd);
        var tgt = target._bytes || target;
        for (var i = 0; i < bytes.length && (targetStart + i) < tgt.length; i++) tgt[targetStart + i] = bytes[i];
        return bytes.length;
    };

    Buffer.prototype.slice = function(start, end) {
        start = start || 0;
        end = end !== undefined ? end : this.length;
        if (start < 0) start = Math.max(this.length + start, 0);
        if (end < 0) end = Math.max(this.length + end, 0);
        return new Buffer(this._bytes.slice(start, end));
    };
    Buffer.prototype.subarray = Buffer.prototype.slice;

    Buffer.prototype.fill = function(value, offset, end, encoding) {
        offset = offset || 0;
        end = end !== undefined ? end : this.length;
        if (typeof value === 'number') {
            this._bytes.fill(value, offset, end);
        } else if (typeof value === 'string') {
            var bytes = stringToBytes(value, encoding);
            for (var i = offset; i < end; i++) this._bytes[i] = bytes[(i - offset) % bytes.length];
        }
        return this;
    };

    Buffer.prototype.write = function(string, offset, length, encoding) {
        offset = offset || 0;
        if (typeof length === 'string') { encoding = length; length = undefined; }
        var bytes = stringToBytes(string, encoding);
        var maxLen = length !== undefined ? Math.min(length, bytes.length) : bytes.length;
        maxLen = Math.min(maxLen, this.length - offset);
        for (var i = 0; i < maxLen; i++) this._bytes[offset + i] = bytes[i];
        return maxLen;
    };

    Buffer.prototype.indexOf = function(value, byteOffset, encoding) {
        byteOffset = byteOffset || 0;
        if (typeof value === 'number') {
            for (var i = byteOffset; i < this.length; i++) if (this._bytes[i] === (value & 0xFF)) return i;
            return -1;
        }
        if (typeof value === 'string') value = stringToBytes(value, encoding);
        var needle = value._bytes || value;
        for (var i = byteOffset; i <= this.length - needle.length; i++) {
            var found = true;
            for (var j = 0; j < needle.length; j++) {
                if (this._bytes[i + j] !== needle[j]) { found = false; break; }
            }
            if (found) return i;
        }
        return -1;
    };

    Buffer.prototype.includes = function(value, byteOffset, encoding) {
        return this.indexOf(value, byteOffset, encoding) !== -1;
    };

    Buffer.prototype.readUInt8 = function(offset) { return this._bytes[offset || 0]; };
    Buffer.prototype.readInt8 = function(offset) {
        var v = this._bytes[offset || 0]; return v > 127 ? v - 256 : v;
    };
    Buffer.prototype.readUInt16BE = function(o) { o = o || 0; return (this._bytes[o] << 8) | this._bytes[o + 1]; };
    Buffer.prototype.readUInt16LE = function(o) { o = o || 0; return this._bytes[o] | (this._bytes[o + 1] << 8); };
    Buffer.prototype.readUInt32BE = function(o) {
        o = o || 0;
        return ((this._bytes[o] * 0x1000000) + (this._bytes[o+1] << 16) + (this._bytes[o+2] << 8) + this._bytes[o+3]) >>> 0;
    };
    Buffer.prototype.readUInt32LE = function(o) {
        o = o || 0;
        return (this._bytes[o] + (this._bytes[o+1] << 8) + (this._bytes[o+2] << 16) + (this._bytes[o+3] * 0x1000000)) >>> 0;
    };

    Buffer.prototype.writeUInt8 = function(v, o) { this._bytes[o || 0] = v & 0xFF; return (o || 0) + 1; };
    Buffer.prototype.writeUInt16BE = function(v, o) { o = o||0; this._bytes[o] = (v>>8)&0xFF; this._bytes[o+1] = v&0xFF; return o+2; };
    Buffer.prototype.writeUInt16LE = function(v, o) { o = o||0; this._bytes[o] = v&0xFF; this._bytes[o+1] = (v>>8)&0xFF; return o+2; };
    Buffer.prototype.writeUInt32BE = function(v, o) { o = o||0; this._bytes[o]=(v>>>24)&0xFF; this._bytes[o+1]=(v>>16)&0xFF; this._bytes[o+2]=(v>>8)&0xFF; this._bytes[o+3]=v&0xFF; return o+4; };
    Buffer.prototype.writeUInt32LE = function(v, o) { o = o||0; this._bytes[o]=v&0xFF; this._bytes[o+1]=(v>>8)&0xFF; this._bytes[o+2]=(v>>16)&0xFF; this._bytes[o+3]=(v>>>24)&0xFF; return o+4; };

    globalThis.Buffer = Buffer;
    return Buffer;
})()
"#;
