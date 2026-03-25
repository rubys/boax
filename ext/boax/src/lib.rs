use std::cell::RefCell;

use boa_engine::{
    Context, JsValue, Source,
    js_string,
    object::JsObject,
    property::PropertyKey,
};
use magnus::{
    prelude::*,
    function, method,
    value::Lazy,
    Error, ExceptionClass, Ruby, TryConvert, Value, RArray, RHash,
};

// Thread-local JS context
thread_local! {
    static CONTEXT: RefCell<Option<Context>> = RefCell::new(None);
}

fn ensure_context_initialized() {
    CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            let context = Context::default();
            *ctx.borrow_mut() = Some(context);
        }
    });
}

fn with_context<F, R>(f: F) -> Result<R, Error>
where
    F: FnOnce(&mut Context) -> Result<R, Error>,
{
    ensure_context_initialized();
    CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let ctx = ctx.as_mut().unwrap();
        f(ctx)
    })
}

fn js_error_to_magnus(err: boa_engine::JsError, context: &mut Context) -> Error {
    let msg = err
        .try_native(context)
        .map(|e| e.message().to_string())
        .unwrap_or_else(|_| format!("{err}"));
    Error::new(ruby_error_class(), msg)
}

fn ruby_error_class() -> ExceptionClass {
    let ruby = Ruby::get().unwrap();
    ruby.get_inner(&BOAX_ERROR)
}

static BOAX_ERROR: Lazy<ExceptionClass> = Lazy::new(|ruby| {
    let module = ruby.define_module("Boax").unwrap();
    module
        .define_error("Error", ruby.exception_standard_error())
        .unwrap()
});

// --- Type Conversion ---

fn ruby_to_js(val: Value, context: &mut Context) -> Result<JsValue, Error> {
    let ruby = Ruby::get().unwrap();

    if val.is_nil() {
        Ok(JsValue::undefined())
    } else if val.equal(ruby.qtrue())? {
        Ok(JsValue::from(true))
    } else if val.equal(ruby.qfalse())? {
        Ok(JsValue::from(false))
    } else if val.is_kind_of(ruby.class_integer()) {
        let i: i64 = TryConvert::try_convert(val)?;
        if let Ok(i32_val) = i32::try_from(i) {
            Ok(JsValue::from(i32_val))
        } else {
            Ok(JsValue::from(i as f64))
        }
    } else if val.is_kind_of(ruby.class_float()) {
        let f: f64 = TryConvert::try_convert(val)?;
        Ok(JsValue::from(f))
    } else if val.is_kind_of(ruby.class_string()) {
        let s: String = TryConvert::try_convert(val)?;
        Ok(JsValue::from(js_string!(&*s)))
    } else if val.is_kind_of(ruby.class_symbol()) {
        let s: String = val.funcall("to_s", ())?;
        Ok(JsValue::from(js_string!(&*s)))
    } else if val.is_kind_of(ruby.class_array()) {
        let arr: RArray = TryConvert::try_convert(val)?;
        let js_arr = boa_engine::object::builtins::JsArray::new(context);
        for item in arr.into_iter() {
            let js_item = ruby_to_js(item, context)?;
            js_arr.push(js_item, context).map_err(|e| js_error_to_magnus(e, context))?;
        }
        Ok(js_arr.into())
    } else if val.is_kind_of(ruby.class_hash()) {
        let hash: RHash = TryConvert::try_convert(val)?;
        let obj = JsObject::with_object_proto(context.intrinsics());
        hash.foreach(|key: Value, value: Value| {
            let js_key: String = if key.is_kind_of(ruby.class_symbol()) {
                key.funcall("to_s", ())?
            } else {
                TryConvert::try_convert(key)?
            };
            let js_val = ruby_to_js(value, context)?;
            obj.set(
                PropertyKey::from(js_string!(&*js_key)),
                js_val,
                false,
                context,
            )
            .map_err(|e| js_error_to_magnus(e, context))?;
            Ok(magnus::r_hash::ForEach::Continue)
        })?;
        Ok(obj.into())
    } else {
        Err(Error::new(
            ruby_error_class(),
            format!("cannot convert {} to JS", unsafe { val.classname() }),
        ))
    }
}

fn js_to_ruby(val: &JsValue, context: &mut Context) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    if val.is_undefined() || val.is_null() {
        Ok(ruby.qnil().as_value())
    } else if let Some(b) = val.as_boolean() {
        Ok(if b {
            ruby.qtrue().as_value()
        } else {
            ruby.qfalse().as_value()
        })
    } else if val.is_number() {
        let n = val.as_number().unwrap();
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            Ok(ruby.integer_from_i64(n as i64).as_value())
        } else {
            Ok(ruby.float_from_f64(n).as_value())
        }
    } else if val.is_string() {
        let s = val.as_string().unwrap();
        let rs = s.to_std_string_escaped();
        Ok(ruby.str_new(&rs).as_value())
    } else if val.is_bigint() {
        let bi = val.as_bigint().unwrap();
        let s = bi.to_string();
        Ok(ruby.integer_from_i64(s.parse::<i64>().unwrap_or(0)).as_value())
    } else if val.is_symbol() {
        let s = val
            .to_string(context)
            .map_err(|e| js_error_to_magnus(e, context))?;
        let rs = s.to_std_string_escaped();
        Ok(ruby.to_symbol(&rs).as_value())
    } else if val.is_object() {
        Ok(BoaxObject::wrap(val.clone()))
    } else {
        Ok(ruby.qnil().as_value())
    }
}

/// Deep conversion: recursively converts JS arrays to Ruby arrays and
/// plain JS objects to Ruby hashes. Non-plain objects stay as BoaxObject.
fn js_to_ruby_deep(val: &JsValue, context: &mut Context) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    if !val.is_object() {
        return js_to_ruby(val, context);
    }

    let obj = val.as_object().unwrap();

    // Array → Ruby Array (recursive)
    if obj.is_array() {
        let length = obj
            .get(js_string!("length"), context)
            .map_err(|e| js_error_to_magnus(e, context))?
            .as_number()
            .unwrap_or(0.0) as i64;
        let arr = ruby.ary_new_capa(length as usize);
        for i in 0..length {
            let item = obj
                .get(i as u32, context)
                .map_err(|e| js_error_to_magnus(e, context))?;
            arr.push(js_to_ruby_deep(&item, context)?)?;
        }
        return Ok(arr.as_value());
    }

    // Try valueOf for wrapper objects (String("..."), Number(...))
    if let Ok(vo) = obj.get(js_string!("valueOf"), context) {
        if vo.is_callable() {
            if let Ok(prim) = vo.as_object().unwrap().call(val, &[], context) {
                if !prim.is_object() {
                    return js_to_ruby(&prim, context);
                }
            }
        }
    }

    // Plain object → Ruby Hash (recursive)
    let proto = obj.get(js_string!("constructor"), context);
    let is_plain = match proto {
        Ok(ref ctor) => {
            if let Some(ctor_obj) = ctor.as_object() {
                // Check if constructor is Object (plain object)
                let name = ctor_obj.get(js_string!("name"), context);
                matches!(name, Ok(ref n) if n.is_string() && n.as_string().unwrap().to_std_string_escaped() == "Object")
            } else {
                false
            }
        }
        Err(_) => false,
    };

    if is_plain {
        let keys = obj
            .own_property_keys(context)
            .map_err(|e| js_error_to_magnus(e, context))?;
        let hash = ruby.hash_new();
        for key in keys {
            let key_str = match &key {
                PropertyKey::String(s) => s.to_std_string_escaped(),
                PropertyKey::Index(i) => i.get().to_string(),
                PropertyKey::Symbol(_) => continue,
            };
            let prop_val = obj
                .get(key.clone(), context)
                .map_err(|e| js_error_to_magnus(e, context))?;
            hash.aset(ruby.str_new(&key_str), js_to_ruby_deep(&prop_val, context)?)?;
        }
        return Ok(hash.as_value());
    }

    // Non-plain object: keep as BoaxObject proxy
    Ok(BoaxObject::wrap(val.clone()))
}

// --- BoaxObject: proxy wrapper around JsValue ---

// JsValue contains GC-traced pointers that aren't Send. This is safe because
// Ruby's GIL ensures single-threaded access, and our thread-local Context
// guarantees the JsValue is only accessed from the thread that created it.
struct BoaxObjectInner {
    value: JsValue,
}
unsafe impl Send for BoaxObjectInner {}

#[magnus::wrap(class = "Boax::JsObject", free_immediately)]
struct BoaxObject {
    inner: BoaxObjectInner,
}

impl BoaxObject {
    fn new(value: JsValue) -> Self {
        BoaxObject {
            inner: BoaxObjectInner { value },
        }
    }

    fn value(&self) -> &JsValue {
        &self.inner.value
    }

    fn wrap(value: JsValue) -> Value {
        let ruby = Ruby::get().unwrap();
        ruby.obj_wrap(BoaxObject::new(value)).as_value()
    }

    fn to_ruby(&self) -> Result<Value, Error> {
        with_context(|ctx| js_to_ruby_deep(self.value(), ctx))
    }

    fn to_s(&self) -> Result<String, Error> {
        with_context(|ctx| {
            let s = self
                .value()
                .to_string(ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;
            Ok(s.to_std_string_escaped())
        })
    }

    fn inspect(&self) -> Result<String, Error> {
        with_context(|ctx| {
            let s = self
                .value()
                .to_string(ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;
            Ok(format!("#<Boax::JsObject {}>", s.to_std_string_escaped()))
        })
    }

    fn js_typeof(&self) -> String {
        self.value().type_of().to_string()
    }

    fn subscript_get(&self, key: Value) -> Result<Value, Error> {
        with_context(|ctx| {
            let obj = self
                .value()
                .as_object()
                .ok_or_else(|| Error::new(ruby_error_class(), "not a JS object"))?;

            let pk = value_to_property_key(key)?;
            let result = obj.get(pk, ctx).map_err(|e| js_error_to_magnus(e, ctx))?;
            js_to_ruby(&result, ctx)
        })
    }

    fn subscript_set(&self, key: Value, val: Value) -> Result<Value, Error> {
        with_context(|ctx| {
            let obj = self
                .value()
                .as_object()
                .ok_or_else(|| Error::new(ruby_error_class(), "not a JS object"))?;

            let pk = value_to_property_key(key)?;
            let js_val = ruby_to_js(val, ctx)?;
            obj.set(pk, js_val, false, ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;
            Ok(val)
        })
    }

    fn method_missing(&self, args: &[Value]) -> Result<Value, Error> {
        if args.is_empty() {
            return Err(Error::new(ruby_error_class(), "no method name given"));
        }

        let method_name: String = args[0].funcall("to_s", ())?;
        let ruby_args = &args[1..];

        // Handle `.new(...)` → JS construct
        if method_name == "new" {
            return self.js_construct(ruby_args);
        }

        with_context(|ctx| {
            let obj = self
                .value()
                .as_object()
                .ok_or_else(|| Error::new(ruby_error_class(), "not a JS object"))?;

            let prop_key = PropertyKey::from(js_string!(&*method_name));
            let prop = obj
                .get(prop_key, ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;

            if prop.is_undefined() {
                return Err(Error::new(
                    ruby_error_class(),
                    format!("undefined JS property: {method_name}"),
                ));
            }

            if prop.is_callable() {
                let js_args: Vec<JsValue> = ruby_args
                    .iter()
                    .map(|a| ruby_to_js(*a, ctx))
                    .collect::<Result<_, _>>()?;
                let result = prop
                    .as_object()
                    .unwrap()
                    .call(self.value(), &js_args, ctx)
                    .map_err(|e| js_error_to_magnus(e, ctx))?;
                js_to_ruby(&result, ctx)
            } else {
                // Property access (getter)
                js_to_ruby(&prop, ctx)
            }
        })
    }

    fn respond_to_missing(&self, args: &[Value]) -> Result<bool, Error> {
        if args.is_empty() {
            return Ok(false);
        }

        let method_name: String = args[0].funcall("to_s", ())?;

        if method_name == "new" {
            return with_context(|_ctx| {
                let obj = self.value().as_object();
                Ok(obj.map_or(false, |o| o.is_constructor()))
            });
        }

        with_context(|ctx| {
            let obj = match self.value().as_object() {
                Some(o) => o,
                None => return Ok(false),
            };
            let prop_key = PropertyKey::from(js_string!(&*method_name));
            let prop = obj.get(prop_key, ctx).map_err(|e| js_error_to_magnus(e, ctx))?;
            Ok(!prop.is_undefined())
        })
    }

    fn js_construct(&self, ruby_args: &[Value]) -> Result<Value, Error> {
        with_context(|ctx| {
            let obj = self
                .value()
                .as_object()
                .ok_or_else(|| Error::new(ruby_error_class(), "not a JS constructor"))?;

            if !obj.is_constructor() {
                return Err(Error::new(ruby_error_class(), "not a JS constructor"));
            }

            let js_args: Vec<JsValue> = ruby_args
                .iter()
                .map(|a| ruby_to_js(*a, ctx))
                .collect::<Result<_, _>>()?;
            let result = obj
                .construct(&js_args, None, ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;
            Ok(BoaxObject::wrap(result.into()))
        })
    }
}

fn value_to_property_key(val: Value) -> Result<PropertyKey, Error> {
    let ruby = Ruby::get().unwrap();
    if val.is_kind_of(ruby.class_integer()) {
        let i: u32 = TryConvert::try_convert(val)?;
        Ok(PropertyKey::from(i))
    } else {
        let s: String = TryConvert::try_convert(val)?;
        Ok(PropertyKey::from(js_string!(&*s)))
    }
}

// --- Module-level functions ---

fn boax_eval(code: String) -> Result<Value, Error> {
    with_context(|ctx| {
        let result = ctx
            .eval(Source::from_bytes(&code))
            .map_err(|e| js_error_to_magnus(e, ctx))?;
        js_to_ruby(&result, ctx)
    })
}

fn boax_import(name: String) -> Result<Value, Error> {
    with_context(|ctx| {
        let global = ctx.global_object();
        let prop = global
            .get(js_string!(&*name), ctx)
            .map_err(|e| js_error_to_magnus(e, ctx))?;

        if prop.is_undefined() {
            return Err(Error::new(
                ruby_error_class(),
                format!("JS global '{name}' not found"),
            ));
        }

        Ok(BoaxObject::wrap(prop))
    })
}

// --- Magnus class storage ---

static BOAX_JSOBJECT_CLASS: Lazy<magnus::RClass> = Lazy::new(|ruby| {
    let module = ruby.define_module("Boax").unwrap();
    let class = module
        .define_class("JsObject", ruby.class_object())
        .unwrap();

    class
        .define_method("to_ruby", method!(BoaxObject::to_ruby, 0))
        .unwrap();
    class
        .define_method("to_s", method!(BoaxObject::to_s, 0))
        .unwrap();
    class
        .define_method("inspect", method!(BoaxObject::inspect, 0))
        .unwrap();
    class
        .define_method("typeof", method!(BoaxObject::js_typeof, 0))
        .unwrap();
    class
        .define_method("[]", method!(BoaxObject::subscript_get, 1))
        .unwrap();
    class
        .define_method("[]=", method!(BoaxObject::subscript_set, 2))
        .unwrap();
    class
        .define_method("method_missing", method!(BoaxObject::method_missing, -1))
        .unwrap();
    class
        .define_method(
            "respond_to_missing?",
            method!(BoaxObject::respond_to_missing, -1),
        )
        .unwrap();

    class
});

// --- Init ---

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    Lazy::force(&BOAX_ERROR, ruby);
    Lazy::force(&BOAX_JSOBJECT_CLASS, ruby);

    let module = ruby.define_module("Boax")?;
    module.define_module_function("eval", function!(boax_eval, 1))?;
    module.define_module_function("import", function!(boax_import, 1))?;

    Ok(())
}
