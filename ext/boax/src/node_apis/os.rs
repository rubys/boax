use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
    object::builtins::JsArray,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "platform", "arch", "type", "release",
    "tmpdir", "homedir", "hostname", "cpus", "totalmem", "freemem",
    "EOL", "endianness", "uptime", "userInfo", "networkInterfaces",
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

    // Constants
    obj.set(js_string!("EOL"), js_string!("\n"), false, context)?;

    // Functions that return static values
    set_fn(&obj, "platform", os_platform, context)?;
    set_fn(&obj, "arch", os_arch, context)?;
    set_fn(&obj, "type", os_type, context)?;
    set_fn(&obj, "release", os_release, context)?;
    set_fn(&obj, "tmpdir", os_tmpdir, context)?;
    set_fn(&obj, "homedir", os_homedir, context)?;
    set_fn(&obj, "hostname", os_hostname, context)?;
    set_fn(&obj, "cpus", os_cpus, context)?;
    set_fn(&obj, "totalmem", os_totalmem, context)?;
    set_fn(&obj, "freemem", os_freemem, context)?;
    set_fn(&obj, "endianness", os_endianness, context)?;
    set_fn(&obj, "uptime", os_uptime, context)?;
    set_fn(&obj, "userInfo", os_user_info, context)?;
    set_fn(&obj, "networkInterfaces", os_network_interfaces, context)?;

    Ok(obj)
}

fn os_platform(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = if cfg!(target_os = "macos") { "darwin" }
        else if cfg!(target_os = "linux") { "linux" }
        else if cfg!(target_os = "windows") { "win32" }
        else { std::env::consts::OS };
    Ok(js_string!(p).into())
}

fn os_arch(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let a = if cfg!(target_arch = "x86_64") { "x64" }
        else if cfg!(target_arch = "aarch64") { "arm64" }
        else if cfg!(target_arch = "x86") { "ia32" }
        else { std::env::consts::ARCH };
    Ok(js_string!(a).into())
}

fn os_type(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let t = if cfg!(target_os = "macos") { "Darwin" }
        else if cfg!(target_os = "linux") { "Linux" }
        else if cfg!(target_os = "windows") { "Windows_NT" }
        else { "Unknown" };
    Ok(js_string!(t).into())
}

fn os_release(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    // Best-effort from uname
    #[cfg(unix)]
    {
        let mut utsname = unsafe { std::mem::zeroed::<libc::utsname>() };
        if unsafe { libc::uname(&mut utsname) } == 0 {
            let release = unsafe { std::ffi::CStr::from_ptr(utsname.release.as_ptr()) };
            return Ok(js_string!(&*release.to_string_lossy().to_string()).into());
        }
    }
    Ok(js_string!("0.0.0").into())
}

fn os_tmpdir(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let tmp = std::env::temp_dir();
    Ok(js_string!(&*tmp.to_string_lossy().to_string()).into())
}

fn os_homedir(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    Ok(js_string!(&*home).into())
}

fn os_hostname(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) } == 0 {
            let name = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr() as *const _) };
            return Ok(js_string!(&*name.to_string_lossy().to_string()).into());
        }
    }
    Ok(js_string!("localhost").into())
}

fn os_cpus(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let arr = JsArray::new(ctx);
    for _ in 0..count {
        let cpu = JsObject::with_object_proto(ctx.intrinsics());
        cpu.set(js_string!("model"), js_string!("unknown"), false, ctx)?;
        cpu.set(js_string!("speed"), JsValue::from(0), false, ctx)?;
        arr.push(JsValue::from(cpu), ctx)?;
    }
    Ok(arr.into())
}

fn os_totalmem(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    // Return 0 as fallback — accurate value would need sysinfo crate
    Ok(JsValue::from(0))
}

fn os_freemem(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    Ok(JsValue::from(0))
}

fn os_endianness(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    let e = if cfg!(target_endian = "little") { "LE" } else { "BE" };
    Ok(js_string!(e).into())
}

fn os_uptime(_: &JsValue, _: &[JsValue], _: &mut Context) -> boa_engine::JsResult<JsValue> {
    Ok(JsValue::from(0))
}

fn os_user_info(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let obj = JsObject::with_object_proto(ctx.intrinsics());
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let homedir = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    obj.set(js_string!("username"), js_string!(&*username), false, ctx)?;
    obj.set(js_string!("homedir"), js_string!(&*homedir), false, ctx)?;
    obj.set(js_string!("shell"), js_string!(std::env::var("SHELL").unwrap_or_default().as_str()), false, ctx)?;
    #[cfg(unix)]
    {
        obj.set(js_string!("uid"), JsValue::from(unsafe { libc::getuid() } as f64), false, ctx)?;
        obj.set(js_string!("gid"), JsValue::from(unsafe { libc::getgid() } as f64), false, ctx)?;
    }
    Ok(obj.into())
}

fn os_network_interfaces(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    // Return empty object — full implementation would need getifaddrs
    Ok(JsObject::with_object_proto(ctx.intrinsics()).into())
}
