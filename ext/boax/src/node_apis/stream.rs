use boa_engine::{
    Context, JsString, JsValue, Module,
    js_string,
    module::SyntheticModuleInitializer,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "Readable", "Writable", "Duplex", "Transform", "PassThrough",
    "Stream", "pipeline", "finished",
];

pub fn create_module(context: &mut Context) -> Module {
    ensure_stream_globals(context);

    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_module),
        None, None, context,
    )
}

fn ensure_stream_globals(context: &mut Context) {
    let global = context.global_object();
    let existing = global.get(js_string!("__BoaxStream"), context).unwrap_or(JsValue::undefined());
    if !existing.is_undefined() {
        return;
    }

    // Stream classes extend EventEmitter — ensure it's registered first
    super::events::ensure_event_emitter_global(context);

    let _ = context.eval(boa_engine::Source::from_bytes(STREAM_JS));
}

fn init_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let global = context.global_object();
    let stream_ns = global.get(js_string!("__BoaxStream"), context)?;

    if let Some(ns) = stream_ns.as_object() {
        module.set_export(&js_string!("default"), stream_ns.clone())?;
        for &name in &EXPORT_NAMES[1..] {
            let val = ns.get(js_string!(name), context)?;
            module.set_export(&js_string!(name), val)?;
        }
    }

    Ok(())
}

const STREAM_JS: &str = r#"
(function() {
    "use strict";

    // Stream MVP: Readable, Writable, Transform, PassThrough, Duplex
    // Built on top of EventEmitter (must be registered on globalThis first).
    // No real backpressure — write() always returns true.

    // --- Stream base class ---

    function Stream() {
        EventEmitter.call(this);
        this.destroyed = false;
    }
    Stream.prototype = Object.create(EventEmitter.prototype);
    Stream.prototype.constructor = Stream;

    Stream.prototype.destroy = function(err) {
        if (this.destroyed) return this;
        this.destroyed = true;
        if (err) this.emit('error', err);
        this.emit('close');
        return this;
    };

    Stream.prototype.pipe = function(dest) {
        var src = this;
        src.on('data', function(chunk) {
            dest.write(chunk);
        });
        src.on('end', function() {
            dest.end();
        });
        src.on('error', function(err) {
            dest.destroy(err);
        });
        dest.emit('pipe', src);
        return dest;
    };

    // --- Readable ---

    function Readable(options) {
        Stream.call(this);
        options = options || {};
        this.readableEncoding = options.encoding || null;
        this.readableHighWaterMark = options.highWaterMark || 16384;
        this.readableFlowing = null;
        this.readableEnded = false;
        this.readableObjectMode = options.objectMode || false;
        this._readableBuffer = [];
        this._readableState = 'idle';

        // If subclass provides _read, use it
        if (typeof this._read !== 'function') {
            this._read = function() {};
        }
    }
    Readable.prototype = Object.create(Stream.prototype);
    Readable.prototype.constructor = Readable;

    Readable.prototype.read = function(size) {
        if (this._readableBuffer.length === 0) {
            return null;
        }
        if (size === undefined || size >= this._readableBuffer.length) {
            var all = this._readableBuffer;
            this._readableBuffer = [];
            // Concatenate if buffer items
            if (all.length === 1) return all[0];
            if (typeof all[0] === 'string') return all.join('');
            return all;
        }
        return this._readableBuffer.splice(0, size);
    };

    Readable.prototype.push = function(chunk) {
        if (chunk === null || chunk === undefined) {
            this.readableEnded = true;
            this.emit('end');
            return false;
        }
        this._readableBuffer.push(chunk);
        this.emit('data', chunk);
        return true;
    };

    Readable.prototype.unshift = function(chunk) {
        this._readableBuffer.unshift(chunk);
    };

    Readable.prototype.resume = function() {
        this.readableFlowing = true;
        return this;
    };

    Readable.prototype.pause = function() {
        this.readableFlowing = false;
        return this;
    };

    Readable.prototype.setEncoding = function(enc) {
        this.readableEncoding = enc;
        return this;
    };

    Readable.prototype.isPaused = function() {
        return this.readableFlowing === false;
    };

    Readable.from = function(iterable, options) {
        var readable = new Readable(options);
        if (Array.isArray(iterable)) {
            var i = 0;
            var items = iterable;
            readable._read = function() {
                if (i < items.length) {
                    readable.push(items[i++]);
                } else {
                    readable.push(null);
                }
            };
            // Push all items immediately
            for (var j = 0; j < items.length; j++) {
                readable.push(items[j]);
            }
            readable.push(null);
        }
        return readable;
    };

    // --- Writable ---

    function Writable(options) {
        Stream.call(this);
        options = options || {};
        this.writableHighWaterMark = options.highWaterMark || 16384;
        this.writableFinished = false;
        this.writableEnded = false;
        this.writableObjectMode = options.objectMode || false;
        this._writableBuffer = [];
        this._writableState = 'idle';

        if (typeof options.write === 'function') {
            this._write = options.write;
        }
    }
    Writable.prototype = Object.create(Stream.prototype);
    Writable.prototype.constructor = Writable;

    Writable.prototype.write = function(chunk, encoding, callback) {
        if (typeof encoding === 'function') { callback = encoding; encoding = undefined; }
        if (this.writableEnded) {
            var err = new Error('write after end');
            if (callback) callback(err);
            this.emit('error', err);
            return false;
        }

        if (typeof this._write === 'function') {
            var self = this;
            this._write(chunk, encoding || 'utf8', function(err) {
                if (err) {
                    self.emit('error', err);
                    if (callback) callback(err);
                } else {
                    if (callback) callback();
                }
            });
        } else {
            this._writableBuffer.push(chunk);
            if (callback) callback();
        }
        return true; // No backpressure
    };

    Writable.prototype.end = function(chunk, encoding, callback) {
        if (typeof chunk === 'function') { callback = chunk; chunk = undefined; }
        if (typeof encoding === 'function') { callback = encoding; encoding = undefined; }
        if (chunk !== undefined && chunk !== null) {
            this.write(chunk, encoding);
        }
        this.writableEnded = true;
        this.writableFinished = true;
        this.emit('finish');
        if (callback) callback();
        return this;
    };

    Writable.prototype.cork = function() {};
    Writable.prototype.uncork = function() {};
    Writable.prototype.setDefaultEncoding = function(enc) { return this; };

    // --- Duplex ---

    function Duplex(options) {
        Readable.call(this, options);
        Writable.call(this, options);
        this.allowHalfOpen = (options && options.allowHalfOpen !== undefined) ? options.allowHalfOpen : true;
    }
    Duplex.prototype = Object.create(Readable.prototype);
    // Mixin Writable methods
    var writableMethods = ['write', 'end', 'cork', 'uncork', 'setDefaultEncoding'];
    for (var i = 0; i < writableMethods.length; i++) {
        Duplex.prototype[writableMethods[i]] = Writable.prototype[writableMethods[i]];
    }
    // Copy writable properties from constructor
    var origDuplexInit = Duplex;
    Duplex = function(options) {
        origDuplexInit.call(this, options);
    };
    Duplex.prototype = origDuplexInit.prototype;
    Duplex.prototype.constructor = Duplex;

    // --- Transform ---

    function Transform(options) {
        Duplex.call(this, options);
        this._transformState = { afterTransform: null };

        if (typeof options === 'object' && typeof options.transform === 'function') {
            this._transform = options.transform;
        }
    }
    Transform.prototype = Object.create(Duplex.prototype);
    Transform.prototype.constructor = Transform;

    Transform.prototype._transform = function(chunk, encoding, callback) {
        callback(null, chunk);
    };

    Transform.prototype.write = function(chunk, encoding, callback) {
        if (typeof encoding === 'function') { callback = encoding; encoding = undefined; }
        var self = this;
        this._transform(chunk, encoding || 'utf8', function(err, data) {
            if (err) {
                self.emit('error', err);
                if (callback) callback(err);
            } else {
                if (data !== null && data !== undefined) {
                    self.push(data);
                }
                if (callback) callback();
            }
        });
        return true;
    };

    Transform.prototype._flush = function(callback) { callback(); };

    Transform.prototype.end = function(chunk, encoding, callback) {
        if (typeof chunk === 'function') { callback = chunk; chunk = undefined; }
        if (typeof encoding === 'function') { callback = encoding; encoding = undefined; }
        if (chunk !== undefined && chunk !== null) {
            this.write(chunk, encoding);
        }
        var self = this;
        this._flush(function(err, data) {
            if (data !== null && data !== undefined) {
                self.push(data);
            }
            self.push(null);
            self.writableEnded = true;
            self.writableFinished = true;
            self.emit('finish');
            if (callback) callback(err);
        });
        return this;
    };

    // --- PassThrough ---

    function PassThrough(options) {
        Transform.call(this, options);
    }
    PassThrough.prototype = Object.create(Transform.prototype);
    PassThrough.prototype.constructor = PassThrough;
    PassThrough.prototype._transform = function(chunk, encoding, callback) {
        callback(null, chunk);
    };

    // --- pipeline ---

    function pipeline() {
        var streams = Array.prototype.slice.call(arguments);
        var callback = typeof streams[streams.length - 1] === 'function' ? streams.pop() : null;

        for (var i = 0; i < streams.length - 1; i++) {
            streams[i].pipe(streams[i + 1]);
        }

        var last = streams[streams.length - 1];
        if (callback) {
            last.on('finish', function() { callback(null); });
            last.on('error', function(err) { callback(err); });
            // Also listen for errors on all streams
            for (var j = 0; j < streams.length - 1; j++) {
                (function(s) {
                    s.on('error', function(err) { callback(err); });
                })(streams[j]);
            }
        }

        return last;
    }

    // --- finished ---

    function finished(stream, callback) {
        var ended = false;
        function done(err) {
            if (!ended) {
                ended = true;
                callback(err || null);
            }
        }
        stream.on('end', function() { done(); });
        stream.on('finish', function() { done(); });
        stream.on('error', function(err) { done(err); });
        stream.on('close', function() { done(); });
        return function() { ended = true; }; // abort function
    }

    // --- Export ---

    var ns = {
        Stream: Stream,
        Readable: Readable,
        Writable: Writable,
        Duplex: Duplex,
        Transform: Transform,
        PassThrough: PassThrough,
        pipeline: pipeline,
        finished: finished
    };

    globalThis.__BoaxStream = ns;
    return ns;
})()
"#;
