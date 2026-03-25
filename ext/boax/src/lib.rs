mod node_apis;

use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsString, JsValue, Module, Source,
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
            condition_names: vec!["import".into(), "module".into(), "default".into()],
            extensions: vec![".js".into(), ".mjs".into(), ".json".into()],
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

    // Create a synthetic entry module: export * from '<name>'
    let entry_src = format!("export * from '{name}';\nexport {{ default }} from '{name}';");

    let source = Source::from_reader(entry_src.as_bytes(), Some(&entry_path));
    let module = Module::parse(source, None, context)
        .map_err(|e| js_error_to_magnus(e, context))?;

    // Insert entry module into the loader's cache so it can be found
    loader.insert(entry_path, module.clone());

    // Load, link, evaluate
    let promise = module.load_link_evaluate(context);
    context.run_jobs().map_err(|e| js_error_to_magnus(e, context))?;

    // Check promise state
    match promise.state() {
        PromiseState::Fulfilled(_) => {}
        PromiseState::Rejected(err) => {
            let js_err = JsError::from_opaque(err);
            return Err(js_error_to_magnus(js_err, context));
        }
        PromiseState::Pending => {
            return Err(Error::new(
                ruby_error_class(),
                format!("module '{name}' did not finish loading"),
            ));
        }
    }

    // Get the module namespace (contains all exports)
    let namespace = module.namespace(context);
    Ok(namespace.into())
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
        // First, try as a JS global (Math, JSON, Date, etc.)
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
    let context = Context::builder()
        .module_loader(loader.clone())
        .build()
        .map_err(|e| Error::new(ruby_error_class(), format!("{e}")))?;

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
    module.define_module_function("init", function!(boax_init, 1))?;

    Ok(())
}
