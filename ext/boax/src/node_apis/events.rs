use boa_engine::{
    Context, JsString, JsValue, Module,
    js_string,
    module::SyntheticModuleInitializer,
};

const EXPORT_NAMES: &[&str] = &["default", "EventEmitter"];

pub fn create_module(context: &mut Context) -> Module {
    // Register EventEmitter on globalThis first, then re-export.
    // This avoids a Boa GC issue where synthetic module namespace
    // properties get corrupted after heavy object use.
    ensure_event_emitter_global(context);

    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_events_module),
        None, None, context,
    )
}

fn ensure_event_emitter_global(context: &mut Context) {
    let global = context.global_object();
    let existing = global.get(js_string!("EventEmitter"), context).unwrap_or(JsValue::undefined());
    if !existing.is_undefined() {
        return;
    }

    let _ = context.eval(boa_engine::Source::from_bytes(EMITTER_JS));
}

fn init_events_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let global = context.global_object();
    let emitter_ctor = global.get(js_string!("EventEmitter"), context)?;

    module.set_export(&js_string!("default"), emitter_ctor.clone())?;
    module.set_export(&js_string!("EventEmitter"), emitter_ctor)?;

    Ok(())
}

const EMITTER_JS: &str = r#"
(function() {
    function EventEmitter() {
        this._events = {};
        this._maxListeners = 10;
    }

    EventEmitter.prototype.on = function(event, listener) {
        if (!this._events[event]) this._events[event] = [];
        this._events[event].push({ fn: listener, once: false });
        return this;
    };

    EventEmitter.prototype.addListener = EventEmitter.prototype.on;

    EventEmitter.prototype.once = function(event, listener) {
        if (!this._events[event]) this._events[event] = [];
        this._events[event].push({ fn: listener, once: true });
        return this;
    };

    EventEmitter.prototype.off = function(event, listener) {
        if (!this._events[event]) return this;
        this._events[event] = this._events[event].filter(function(entry) {
            return entry.fn !== listener;
        });
        return this;
    };

    EventEmitter.prototype.removeListener = EventEmitter.prototype.off;

    EventEmitter.prototype.removeAllListeners = function(event) {
        if (event === undefined) {
            this._events = {};
        } else {
            delete this._events[event];
        }
        return this;
    };

    EventEmitter.prototype.emit = function(event) {
        if (!this._events[event]) return false;
        var args = Array.prototype.slice.call(arguments, 1);
        var listeners = this._events[event].slice();
        var removed = [];
        for (var i = 0; i < listeners.length; i++) {
            listeners[i].fn.apply(this, args);
            if (listeners[i].once) removed.push(i);
        }
        for (var j = removed.length - 1; j >= 0; j--) {
            this._events[event].splice(removed[j], 1);
        }
        return listeners.length > 0;
    };

    EventEmitter.prototype.listenerCount = function(event) {
        return this._events[event] ? this._events[event].length : 0;
    };

    EventEmitter.prototype.eventNames = function() {
        return Object.keys(this._events).filter(function(key) {
            return this._events[key].length > 0;
        }.bind(this));
    };

    EventEmitter.prototype.listeners = function(event) {
        if (!this._events[event]) return [];
        return this._events[event].map(function(entry) { return entry.fn; });
    };

    EventEmitter.prototype.setMaxListeners = function(n) {
        this._maxListeners = n;
        return this;
    };

    EventEmitter.prototype.getMaxListeners = function() {
        return this._maxListeners;
    };

    EventEmitter.defaultMaxListeners = 10;

    globalThis.EventEmitter = EventEmitter;
    return EventEmitter;
})()
"#;
