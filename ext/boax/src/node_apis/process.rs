use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
    object::builtins::JsArray,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "env", "cwd", "platform", "arch", "version",
    "versions", "argv", "argv0", "pid", "ppid",
    "nextTick", "exit", "stdout", "stderr",
    "hrtime",
];

pub fn create_module(context: &mut Context) -> Module {
    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_module),
        None, None, context,
    )
}

fn init_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let obj = build_object(context)?;
    module.set_export(&js_string!("default"), obj.clone().into())?;
    for &name in &EXPORT_NAMES[1..] {
        let val = obj.get(js_string!(name), context)?;
        module.set_export(&js_string!(name), val)?;
    }
    Ok(())
}

fn set_fn(
    obj: &JsObject, name: &str,
    f: fn(&JsValue, &[JsValue], &mut Context) -> boa_engine::JsResult<JsValue>,
    context: &mut Context,
) -> boa_engine::JsResult<()> {
    let func = NativeFunction::from_fn_ptr(f).to_js_function(context.realm());
    obj.set(js_string!(name), JsValue::from(func), false, context)?;
    Ok(())
}

fn build_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    // process.env — proxy to actual environment variables
    let env_obj = JsObject::with_object_proto(context.intrinsics());
    for (key, val) in std::env::vars() {
        env_obj.set(js_string!(&*key), js_string!(&*val), false, context)?;
    }
    obj.set(js_string!("env"), JsValue::from(env_obj), false, context)?;

    // process.platform
    let platform = if cfg!(target_os = "macos") { "darwin" }
        else if cfg!(target_os = "linux") { "linux" }
        else if cfg!(target_os = "windows") { "win32" }
        else { "unknown" };
    obj.set(js_string!("platform"), js_string!(platform), false, context)?;

    // process.arch
    let arch = if cfg!(target_arch = "x86_64") { "x64" }
        else if cfg!(target_arch = "aarch64") { "arm64" }
        else if cfg!(target_arch = "x86") { "ia32" }
        else { "unknown" };
    obj.set(js_string!("arch"), js_string!(arch), false, context)?;

    // process.version
    obj.set(js_string!("version"), js_string!("v21.0.0"), false, context)?;

    // process.versions
    let versions = JsObject::with_object_proto(context.intrinsics());
    versions.set(js_string!("node"), js_string!("21.0.0"), false, context)?;
    versions.set(js_string!("boa"), js_string!("0.21.0"), false, context)?;
    obj.set(js_string!("versions"), JsValue::from(versions), false, context)?;

    // process.argv, argv0
    let argv = JsArray::new(context);
    argv.push(js_string!("boax"), context)?;
    obj.set(js_string!("argv"), JsValue::from(argv), false, context)?;
    obj.set(js_string!("argv0"), js_string!("boax"), false, context)?;

    // process.pid, ppid
    obj.set(js_string!("pid"), JsValue::from(std::process::id() as f64), false, context)?;
    #[cfg(unix)]
    {
        obj.set(js_string!("ppid"), JsValue::from(unsafe { libc::getppid() } as f64), false, context)?;
    }
    #[cfg(not(unix))]
    {
        obj.set(js_string!("ppid"), JsValue::from(0.0), false, context)?;
    }

    // Functions
    set_fn(&obj, "cwd", process_cwd, context)?;
    set_fn(&obj, "exit", process_exit, context)?;
    set_fn(&obj, "nextTick", process_next_tick, context)?;
    set_fn(&obj, "hrtime", process_hrtime, context)?;

    // process.stdout / stderr — minimal writable interface
    let stdout = make_writable("stdout", context)?;
    let stderr = make_writable("stderr", context)?;
    obj.set(js_string!("stdout"), JsValue::from(stdout), false, context)?;
    obj.set(js_string!("stderr"), JsValue::from(stderr), false, context)?;

    Ok(obj)
}

fn make_writable(name: &str, context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());
    obj.set(js_string!("isTTY"), JsValue::from(false), false, context)?;
    if name == "stdout" {
        set_fn(&obj, "write", stdout_write, context)?;
    } else {
        set_fn(&obj, "write", stderr_write, context)?;
    }
    Ok(obj)
}

fn process_cwd(_this: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let cwd = std::env::current_dir().unwrap_or_default();
    Ok(js_string!(&*cwd.to_string_lossy().to_string()).into())
}

fn process_exit(_this: &JsValue, args: &[JsValue], _ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let code = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as i32;
    std::process::exit(code);
}

fn process_next_tick(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    // Synchronous execution — call the callback immediately
    if let Some(cb) = args.first().and_then(|v| v.as_object()).filter(|o| o.is_callable()) {
        let cb_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();
        cb.call(&JsValue::undefined(), &cb_args, ctx)?;
    }
    Ok(JsValue::undefined())
}

fn process_hrtime(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();

    // If called with a previous hrtime, return the difference
    if let Some(prev) = args.first().and_then(|v| v.as_object()).filter(|o| o.is_array()) {
        let prev_s = prev.get(0u32, ctx)?.as_number().unwrap_or(0.0) as u64;
        let prev_n = prev.get(1u32, ctx)?.as_number().unwrap_or(0.0) as u64;
        let diff_total_ns = (secs * 1_000_000_000 + nanos as u64) - (prev_s * 1_000_000_000 + prev_n);
        let result = JsArray::new(ctx);
        result.push(JsValue::from((diff_total_ns / 1_000_000_000) as f64), ctx)?;
        result.push(JsValue::from((diff_total_ns % 1_000_000_000) as f64), ctx)?;
        return Ok(result.into());
    }

    let result = JsArray::new(ctx);
    result.push(JsValue::from(secs as f64), ctx)?;
    result.push(JsValue::from(nanos as f64), ctx)?;
    Ok(result.into())
}

fn stdout_write(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    use std::io::Write;
    if let Some(s) = args.first() {
        let text = s.to_string(ctx)?.to_std_string_escaped();
        let _ = std::io::stdout().write_all(text.as_bytes());
    }
    Ok(JsValue::from(true))
}

fn stderr_write(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    use std::io::Write;
    if let Some(s) = args.first() {
        let text = s.to_string(ctx)?.to_std_string_escaped();
        let _ = std::io::stderr().write_all(text.as_bytes());
    }
    Ok(JsValue::from(true))
}
