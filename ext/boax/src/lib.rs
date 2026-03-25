mod node_apis;

use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsString, JsValue, Module, NativeFunction, Source,
    builtins::promise::PromiseState,
    js_string,
    module::{ModuleLoader, Referrer, resolve_module_specifier},
    object::JsObject,
    property::PropertyKey,
};
use boa_gc::GcRefCell;
use magnus::{
    prelude::*,
    function, method,
    value::Lazy,
    Error, ExceptionClass, Ruby, TryConvert, Value, RArray, RHash,
};
use oxc_resolver::{ResolveOptions, Resolver};
use rustc_hash::FxHashMap;

// --- NpmModuleLoader ---

struct NpmModuleLoader {
    root: PathBuf,
    resolver: Resolver,
    module_map: GcRefCell<FxHashMap<PathBuf, Module>>,
}

impl std::fmt::Debug for NpmModuleLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NpmModuleLoader")
            .field("root", &self.root)
            .finish()
    }
}

impl NpmModuleLoader {
    fn new(root: PathBuf) -> JsResult<Self> {
        let canonical_root = root.canonicalize().map_err(|e| {
            JsNativeError::typ()
                .with_message(format!("could not resolve root path `{}`", root.display()))
                .with_cause(JsError::from_opaque(js_string!(e.to_string()).into()))
        })?;

        let options = ResolveOptions {
            condition_names: vec!["import".into(), "require".into(), "module".into(), "default".into()],
            extensions: vec![".js".into(), ".mjs".into(), ".cjs".into(), ".json".into()],
            main_fields: vec!["module".into(), "main".into()],
            ..ResolveOptions::default()
        };

        Ok(Self {
            root: canonical_root,
            resolver: Resolver::new(options),
            module_map: GcRefCell::default(),
        })
    }

    fn insert(&self, path: PathBuf, module: Module) {
        self.module_map.borrow_mut().insert(path, module);
    }

    fn get(&self, path: &Path) -> Option<Module> {
        self.module_map.borrow().get(path).cloned()
    }

    /// Resolve a specifier to an absolute file path.
    fn resolve_path(
        &self,
        specifier: &str,
        referrer_path: Option<&Path>,
        context: &mut Context,
    ) -> JsResult<PathBuf> {
        let is_relative = specifier.starts_with("./") || specifier.starts_with("../");

        if is_relative {
            // Use Boa's built-in path resolution for relative imports
            resolve_module_specifier(
                Some(&self.root),
                &js_string!(specifier),
                referrer_path,
                context,
            )
        } else {
            // Bare specifier → resolve via oxc_resolver from the referrer's directory
            // or the project root
            let resolve_dir = referrer_path
                .and_then(|p| p.parent())
                .unwrap_or(&self.root);

            self.resolver
                .resolve(resolve_dir, specifier)
                .map(|resolution| resolution.into_path_buf())
                .map_err(|err| {
                    JsNativeError::typ()
                        .with_message(format!("could not resolve module '{specifier}': {err}"))
                        .into()
                })
        }
    }
}

impl ModuleLoader for NpmModuleLoader {
    fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> impl Future<Output = JsResult<Module>> {
        let result = (|| {
            let spec_str = specifier.to_std_string_escaped();
            let mut ctx = context.borrow_mut();

            // Check for Node.js built-in modules (e.g., "path", "node:path")
            if let Some(builtin_name) = node_apis::resolve_node_builtin(&spec_str) {
                let cache_key = PathBuf::from(format!("node:{builtin_name}"));
                if let Some(module) = self.get(&cache_key) {
                    return Ok(module);
                }
                let module = node_apis::create_node_module(builtin_name, &mut ctx);
                self.insert(cache_key, module.clone());
                return Ok(module);
            }

            let path = self.resolve_path(&spec_str, referrer.path(), &mut ctx)?;

            // Check cache
            if let Some(module) = self.get(&path) {
                return Ok(module);
            }

            // Load from filesystem
            let source = Source::from_filepath(&path).map_err(|err| {
                JsNativeError::typ()
                    .with_message(format!("could not open `{}`", path.display()))
                    .with_cause(JsError::from_opaque(js_string!(err.to_string()).into()))
            })?;

            // Check if JSON
            let module = if path.extension().is_some_and(|ext| ext == "json") {
                let json_str = std::fs::read_to_string(&path).map_err(|err| {
                    JsNativeError::typ()
                        .with_message(format!("could not read `{}`", path.display()))
                        .with_cause(JsError::from_opaque(js_string!(err.to_string()).into()))
                })?;
                Module::parse_json(js_string!(&*json_str), &mut ctx)
                    .map_err(|err| {
                        JsNativeError::syntax()
                            .with_message(format!("could not parse JSON module `{spec_str}`"))
                            .with_cause(err)
                    })?
            } else {
                Module::parse(source, None, &mut ctx).map_err(|err| {
                    JsNativeError::syntax()
                        .with_message(format!("could not parse module `{spec_str}`"))
                        .with_cause(err)
                })?
            };

            self.insert(path, module.clone());
            Ok(module)
        })();

        async { result }
    }

    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        module: &Module,
        context: &mut Context,
    ) {
        if let Some(path) = module.path() {
            let _ = import_meta.set(
                js_string!("url"),
                js_string!(format!("file://{}", path.display())),
                false,
                context,
            );
        }
    }
}

// --- Thread-local state ---

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = RefCell::new(None);
    static MODULE_LOADER: RefCell<Option<Rc<NpmModuleLoader>>> = RefCell::new(None);
}

fn ensure_context_initialized() {
    CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            let mut context = Context::default();
            // Register Web API extensions: URL, setTimeout, TextEncoder, console, etc.
            let _ = boa_runtime::register(boa_runtime::extensions::ConsoleExtension::default(), None, &mut context);
            // Polyfill Intl.NumberFormat (currency/percent) and Intl.DateTimeFormat
            node_apis::intl_polyfill::register_intl_polyfills(&mut context);
            // Register globalThis.crypto.getRandomValues (Web Crypto API)
            register_web_crypto(&mut context);
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
        let result = f(ctx);
        // Drain the promise job queue after every interaction so that
        // microtasks (Promise .then callbacks, etc.) execute promptly.
        let _ = ctx.run_jobs();
        result
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
    } else if let Ok(boax_obj) = <&BoaxObject as TryConvert>::try_convert(val) {
        // Unwrap BoaxObject back to its inner JsValue
        Ok(boax_obj.value().clone())
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

    /// Invoke the wrapped JS value directly as a function.
    /// Used for bare function exports: `minimist.call("--foo", "bar")`
    fn js_call(&self, args: &[Value]) -> Result<Value, Error> {
        with_context(|ctx| {
            let obj = self
                .value()
                .as_object()
                .ok_or_else(|| Error::new(ruby_error_class(), "not a JS function"))?;

            if !obj.is_callable() {
                return Err(Error::new(ruby_error_class(), "not a JS function"));
            }

            let js_args: Vec<JsValue> = args
                .iter()
                .map(|a| ruby_to_js(*a, ctx))
                .collect::<Result<_, _>>()?;
            let result = obj
                .call(&JsValue::undefined(), &js_args, ctx)
                .map_err(|e| js_error_to_magnus(e, ctx))?;
            js_to_ruby(&result, ctx)
        })
    }

    fn method_missing(&self, args: &[Value]) -> Result<Value, Error> {
        if args.is_empty() {
            return Err(Error::new(ruby_error_class(), "no method name given"));
        }

        let raw_name: String = args[0].funcall("to_s", ())?;
        let ruby_args = &args[1..];

        // Strip trailing `!` — escape hatch for when Ruby methods shadow JS ones.
        // e.g., `promise.then!(cb)` calls JS `promise.then(cb)` since
        // Ruby's Kernel#then would otherwise intercept the call.
        let method_name = raw_name.strip_suffix('!').unwrap_or(&raw_name);

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
                // If it looks like a constructor (uppercase name + constructable)
                // and called with no args, return as proxy so .new() works.
                // JS convention: constructors are PascalCase, methods are camelCase.
                let prop_obj = prop.as_object().unwrap();
                let looks_like_constructor = ruby_args.is_empty()
                    && prop_obj.is_constructor()
                    && method_name.starts_with(|c: char| c.is_ascii_uppercase());
                if looks_like_constructor {
                    return Ok(BoaxObject::wrap(prop));
                }

                let js_args: Vec<JsValue> = ruby_args
                    .iter()
                    .map(|a| ruby_to_js(*a, ctx))
                    .collect::<Result<_, _>>()?;
                let result = prop_obj
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

        let raw_name: String = args[0].funcall("to_s", ())?;
        let method_name = raw_name.strip_suffix('!').unwrap_or(&raw_name);

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

// --- Module import support ---

/// Import an npm package by creating a synthetic entry module that
/// re-exports everything from the package, then loading/linking/evaluating it.
fn import_module(name: &str, context: &mut Context) -> Result<JsValue, Error> {
    let loader = MODULE_LOADER.with(|l| l.borrow().clone()).ok_or_else(|| {
        Error::new(
            ruby_error_class(),
            format!(
                "module '{name}' not found as a JS global. \
                 Call Boax.init(root: '/path/to/project') to enable npm package imports."
            ),
        )
    })?;

    // Check if we've already loaded this module
    let entry_path = loader.root.join(format!("__boax_entry_{name}__.mjs"));
    if let Some(cached) = loader.get(&entry_path) {
        let namespace = cached.namespace(context);
        return Ok(namespace.into());
    }

    // Create a synthetic entry module. Try with `export { default }` first;
    // if link/evaluate fails (package has no default export), retry without it.
    let entry_variants = [
        format!("export * from '{name}';\nexport {{ default }} from '{name}';"),
        format!("export * from '{name}';"),
    ];

    let mut last_err = None;
    for entry_src in &entry_variants {
        // Remove any previous failed entry from cache
        loader.insert(entry_path.clone(), {
            let source = Source::from_reader(entry_src.as_bytes(), Some(&entry_path));
            match Module::parse(source, None, context) {
                Ok(m) => m,
                Err(e) => {
                    last_err = Some(js_error_to_magnus(e, context));
                    continue;
                }
            }
        });

        let module = loader.get(&entry_path).unwrap();
        let promise = module.load_link_evaluate(context);
        let _ = context.run_jobs();

        match promise.state() {
            PromiseState::Fulfilled(_) => {
                let namespace = module.namespace(context);
                return Ok(namespace.into());
            }
            PromiseState::Rejected(err) => {
                last_err = Some(js_error_to_magnus(JsError::from_opaque(err), context));
                continue;
            }
            PromiseState::Pending => {
                last_err = Some(Error::new(
                    ruby_error_class(),
                    format!("module '{name}' did not finish loading"),
                ));
                continue;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        Error::new(ruby_error_class(), format!("failed to load module '{name}'"))
    }))
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
        // Check for Node built-in modules first (they take priority over globals
        // since we may have registered globals like `crypto` that shadow them)
        if node_apis::resolve_node_builtin(&name).is_some() {
            let ns = import_module(&name, ctx)?;
            return Ok(BoaxObject::wrap(ns));
        }

        // Try as a JS global (Math, JSON, Date, Intl, etc.)
        let global = ctx.global_object();
        let prop = global
            .get(js_string!(&*name), ctx)
            .map_err(|e| js_error_to_magnus(e, ctx))?;

        if !prop.is_undefined() {
            return Ok(BoaxObject::wrap(prop));
        }

        // Not a global — try as an npm module
        let ns = import_module(&name, ctx)?;
        Ok(BoaxObject::wrap(ns))
    })
}

fn boax_require(name: String) -> Result<Value, Error> {
    with_context(|ctx| {
        let result = cjs_require(&name, None, ctx)?;
        Ok(BoaxObject::wrap(result))
    })
}

/// CommonJS require implementation.
/// Wraps the file in (function(exports, require, module, __filename, __dirname) { ... })
/// and executes it, returning module.exports.
fn cjs_require(specifier: &str, referrer: Option<&std::path::Path>, context: &mut Context) -> Result<JsValue, Error> {
    // Check for Node built-ins
    let builtin_name = specifier.strip_prefix("node:").unwrap_or(specifier);
    if node_apis::resolve_node_builtin(builtin_name).is_some() {
        // Return the globalThis object for this builtin
        node_apis::create_node_module(builtin_name, context);
        // The globalThis pattern means the module's exports are available as globals
        // For modules like fs, path, etc., we need to return the default export object
        let global_name = match builtin_name {
            "events" => "EventEmitter",
            "buffer" => "Buffer",
            "string_decoder" => "StringDecoder",
            "stream" => "__BoaxStream",
            _ => {
                // For path, util, fs, process, os, querystring, assert, url, crypto:
                // these are built with Rust NativeFunctions, not on globalThis.
                // We need to create the module and get its namespace.
                // Use an ES module import under the hood.
                let ns = import_module(builtin_name, context)?;
                return Ok(ns);
            }
        };
        let val = context.global_object().get(js_string!(global_name), context)
            .map_err(|e| js_error_to_magnus(e, context))?;
        return Ok(val);
    }

    // Resolve the file path
    let loader = MODULE_LOADER.with(|l| l.borrow().clone()).ok_or_else(|| {
        Error::new(
            ruby_error_class(),
            format!(
                "module '{specifier}' not found. \
                 Call Boax.init(root: '/path/to/project') to enable npm package requires."
            ),
        )
    })?;

    let resolve_dir = referrer
        .and_then(|p| p.parent())
        .unwrap_or(&loader.root);

    let resolved = loader.resolver
        .resolve(resolve_dir, specifier)
        .map_err(|err| {
            Error::new(
                ruby_error_class(),
                format!("could not resolve '{specifier}': {err}"),
            )
        })?;
    let path = resolved.into_path_buf();

    // Check CJS cache
    let cache_key = path.to_string_lossy().to_string();
    let cached = context.global_object()
        .get(js_string!("__boaxCjsCache"), context)
        .ok()
        .and_then(|v| v.as_object());

    if let Some(cache) = &cached {
        let entry = cache.get(js_string!(&*cache_key), context)
            .unwrap_or(JsValue::undefined());
        if !entry.is_undefined() {
            return Ok(entry);
        }
    }

    // Read the file
    let source = std::fs::read_to_string(&path).map_err(|e| {
        Error::new(
            ruby_error_class(),
            format!("could not read '{}': {e}", path.display()),
        )
    })?;

    // Handle JSON files
    if path.extension().is_some_and(|ext| ext == "json") {
        let json_val = context.eval(Source::from_bytes(&format!("({})", source)))
            .map_err(|e| js_error_to_magnus(e, context))?;
        cjs_cache_set(context, &cache_key, &json_val)?;
        return Ok(json_val);
    }

    // Create module object: { exports: {} }
    let module_obj = JsObject::with_object_proto(context.intrinsics());
    let exports_obj = JsObject::with_object_proto(context.intrinsics());
    module_obj.set(js_string!("exports"), JsValue::from(exports_obj.clone()), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;
    module_obj.set(js_string!("id"), js_string!(&*cache_key), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;
    module_obj.set(js_string!("filename"), js_string!(&*path.to_string_lossy().to_string()), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;
    module_obj.set(js_string!("loaded"), JsValue::from(false), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;

    // Cache module.exports BEFORE execution (handles circular deps)
    cjs_cache_set(context, &cache_key, &JsValue::from(exports_obj.clone()))?;

    let dirname = path.parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let filename = path.to_string_lossy().to_string();

    // Wrap in CJS function and execute
    let wrapped = format!(
        "(function(exports, require, module, __filename, __dirname) {{\n{source}\n}})"
    );

    let wrapper_fn = context.eval(Source::from_bytes(&wrapped))
        .map_err(|e| js_error_to_magnus(e, context))?;

    let wrapper_obj = wrapper_fn.as_object().ok_or_else(|| {
        Error::new(ruby_error_class(), format!("failed to wrap CJS module: {specifier}"))
    })?;

    // Build the require function for this module's context
    // Store the dirname so nested require() calls resolve relative to this file
    let require_fn = build_cjs_require_fn(&path, context)?;

    wrapper_obj.call(
        &JsValue::undefined(),
        &[
            JsValue::from(exports_obj),
            JsValue::from(require_fn),
            JsValue::from(module_obj.clone()),
            js_string!(&*filename).into(),
            js_string!(&*dirname).into(),
        ],
        context,
    ).map_err(|e| js_error_to_magnus(e, context))?;

    // Mark as loaded
    let _ = module_obj.set(js_string!("loaded"), JsValue::from(true), false, context);

    // Return module.exports (may have been reassigned)
    let result = module_obj.get(js_string!("exports"), context)
        .map_err(|e| js_error_to_magnus(e, context))?;

    // Update cache with final module.exports
    cjs_cache_set(context, &cache_key, &result)?;

    Ok(result)
}

fn cjs_cache_set(context: &mut Context, key: &str, value: &JsValue) -> Result<(), Error> {
    let global = context.global_object();
    let cache = global.get(js_string!("__boaxCjsCache"), context)
        .unwrap_or(JsValue::undefined());

    let cache_obj = if let Some(obj) = cache.as_object() {
        obj
    } else {
        let obj = JsObject::with_object_proto(context.intrinsics());
        global.set(js_string!("__boaxCjsCache"), JsValue::from(obj.clone()), false, context)
            .map_err(|e| js_error_to_magnus(e, context))?;
        obj
    };

    cache_obj.set(js_string!(key), value.clone(), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;
    Ok(())
}

/// Build a require() function that resolves relative to a specific file.
fn build_cjs_require_fn(referrer_path: &std::path::Path, context: &mut Context) -> Result<JsObject, Error> {
    let dirname = referrer_path.parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Store the referrer dirname in globalThis so the require function can access it
    // We use a unique key per module to avoid collisions
    let key = format!("__boaxCjsDir_{}", referrer_path.to_string_lossy().replace('/', "_"));
    context.global_object()
        .set(js_string!(&*key), js_string!(&*dirname), false, context)
        .map_err(|e| js_error_to_magnus(e, context))?;

    // Create a JS require function that calls back into our Rust require
    // For now, use a simple implementation that stores the referrer path
    let require_src = format!(
        r#"(function() {{
            var _dir = globalThis["{}"];
            function require(id) {{
                // Node builtins
                if (typeof globalThis.__boaxRequire === 'function') {{
                    return globalThis.__boaxRequire(id, _dir);
                }}
                throw new Error("require is not available: " + id);
            }}
            require.resolve = function(id) {{ return id; }};
            require.cache = globalThis.__boaxCjsCache || {{}};
            return require;
        }})()"#,
        key
    );

    let require_fn = context.eval(Source::from_bytes(&require_src))
        .map_err(|e| js_error_to_magnus(e, context))?;

    Ok(require_fn.as_object().unwrap())
}

/// Register globalThis.crypto with getRandomValues (Web Crypto API).
/// Needed by packages like uuid that use the web standard rather than Node's crypto.
fn register_web_crypto(context: &mut Context) {
    let crypto_obj = JsObject::with_object_proto(context.intrinsics());
    let get_random = NativeFunction::from_fn_ptr(web_crypto_get_random_values)
        .to_js_function(context.realm());
    let _ = crypto_obj.set(js_string!("getRandomValues"), JsValue::from(get_random), false, context);
    let random_uuid = NativeFunction::from_fn_ptr(web_crypto_random_uuid)
        .to_js_function(context.realm());
    let _ = crypto_obj.set(js_string!("randomUUID"), JsValue::from(random_uuid), false, context);
    let _ = context.global_object().set(js_string!("crypto"), JsValue::from(crypto_obj), false, context);
}

fn web_crypto_get_random_values(_: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let arr = args.first()
        .and_then(|v| v.as_object())
        .ok_or_else(|| JsNativeError::typ().with_message("argument must be a TypedArray"))?;

    let length = arr.get(js_string!("length"), context)?
        .as_number().unwrap_or(0.0) as usize;

    let mut bytes = vec![0u8; length];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| JsNativeError::typ().with_message(format!("getRandomValues failed: {e}")))?;

    for (i, &b) in bytes.iter().enumerate() {
        arr.set(i as u32, JsValue::from(b as i32), false, context)?;
    }

    Ok(args.first().cloned().unwrap_or(JsValue::undefined()))
}

fn web_crypto_random_uuid(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| JsNativeError::typ().with_message(format!("randomUUID failed: {e}")))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    );
    Ok(js_string!(&*uuid).into())
}

/// Register the native __boaxRequire function on the JS context.
/// This is called from JS require() wrappers to resolve nested CJS requires.
fn register_cjs_require_native(context: &mut Context) {
    let require_native = NativeFunction::from_fn_ptr(native_cjs_require)
        .to_js_function(context.realm());
    let _ = context.global_object().set(
        js_string!("__boaxRequire"),
        JsValue::from(require_native),
        false,
        context,
    );
}

fn native_cjs_require(_: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let specifier = args.first()
        .ok_or_else(|| JsNativeError::typ().with_message("require() needs a specifier"))?
        .to_string(context)?
        .to_std_string_escaped();
    let dirname = args.get(1)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default();

    let referrer = if dirname.is_empty() {
        None
    } else {
        Some(PathBuf::from(&dirname).join("__dummy__.js"))
    };

    cjs_require(&specifier, referrer.as_deref(), context)
        .map_err(|e| JsNativeError::typ().with_message(e.to_string()).into())
}

fn boax_init(root: String) -> Result<Value, Error> {
    let root_path = PathBuf::from(&root);

    // Verify node_modules exists
    let node_modules = root_path.join("node_modules");
    if !node_modules.is_dir() {
        return Err(Error::new(
            ruby_error_class(),
            format!(
                "node_modules not found at {}. Run 'npm install' first.",
                node_modules.display()
            ),
        ));
    }

    // Create the module loader
    let loader = Rc::new(
        NpmModuleLoader::new(root_path)
            .map_err(|e| Error::new(ruby_error_class(), format!("{e}")))?,
    );

    // Rebuild the context with the module loader
    let mut context = Context::builder()
        .module_loader(loader.clone())
        .build()
        .map_err(|e| Error::new(ruby_error_class(), format!("{e}")))?;
    let _ = boa_runtime::register(boa_runtime::extensions::ConsoleExtension::default(), None, &mut context);
    node_apis::intl_polyfill::register_intl_polyfills(&mut context);
    register_web_crypto(&mut context);

    // Register CJS require native callback
    register_cjs_require_native(&mut context);

    // Store both
    MODULE_LOADER.with(|l| *l.borrow_mut() = Some(loader));
    CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(context));

    let ruby = Ruby::get().unwrap();
    Ok(ruby.qnil().as_value())
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
        .define_method("call", method!(BoaxObject::js_call, -1))
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
    module.define_module_function("require", function!(boax_require, 1))?;
    module.define_module_function("init", function!(boax_init, 1))?;

    Ok(())
}
