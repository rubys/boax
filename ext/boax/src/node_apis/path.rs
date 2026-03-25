use std::path::{Path, PathBuf, Component};

use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "join", "resolve", "basename", "dirname", "extname",
    "parse", "format", "normalize", "isAbsolute", "relative",
    "sep", "delimiter", "posix",
];

pub fn create_module(context: &mut Context) -> Module {
    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();

    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_path_module),
        None,
        None,
        context,
    )
}

fn init_path_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let path_obj = build_path_object(context)?;

    // Export the object as default
    module.set_export(&js_string!("default"), path_obj.clone().into())?;

    // Export each function/property individually as named exports
    for &name in &EXPORT_NAMES[1..] {  // skip "default"
        let val = path_obj.get(js_string!(name), context)?;
        module.set_export(&js_string!(name), val)?;
    }

    Ok(())
}

fn build_path_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    // sep and delimiter
    obj.set(js_string!("sep"), js_string!("/"), false, context)?;
    obj.set(js_string!("delimiter"), js_string!(":"), false, context)?;

    // Functions
    set_fn(&obj, "join", 0, path_join, context)?;
    set_fn(&obj, "resolve", 0, path_resolve, context)?;
    set_fn(&obj, "basename", 1, path_basename, context)?;
    set_fn(&obj, "dirname", 1, path_dirname, context)?;
    set_fn(&obj, "extname", 1, path_extname, context)?;
    set_fn(&obj, "parse", 1, path_parse, context)?;
    set_fn(&obj, "format", 1, path_format, context)?;
    set_fn(&obj, "normalize", 1, path_normalize, context)?;
    set_fn(&obj, "isAbsolute", 1, path_is_absolute, context)?;
    set_fn(&obj, "relative", 2, path_relative, context)?;

    // posix is self-referential (posix implementation is the default on unix)
    obj.set(js_string!("posix"), JsValue::from(obj.clone()), false, context)?;

    Ok(obj)
}

fn set_fn(
    obj: &JsObject,
    name: &str,
    _length: usize,
    f: fn(&JsValue, &[JsValue], &mut Context) -> boa_engine::JsResult<JsValue>,
    context: &mut Context,
) -> boa_engine::JsResult<()> {
    let func = NativeFunction::from_fn_ptr(f)
        .to_js_function(context.realm());
    obj.set(js_string!(name), JsValue::from(func), false, context)?;
    Ok(())
}

// --- Helper: extract string arg ---

fn arg_str(args: &[JsValue], idx: usize, context: &mut Context) -> boa_engine::JsResult<String> {
    args.get(idx)
        .map(|v| v.to_string(context).map(|s| s.to_std_string_escaped()))
        .unwrap_or(Ok(String::new()))
}

// --- path.join(...segments) ---

fn path_join(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    if args.is_empty() {
        return Ok(js_string!(".").into());
    }

    let mut result = String::new();
    for (i, arg) in args.iter().enumerate() {
        let s = arg.to_string(context)?.to_std_string_escaped();
        if i > 0 && !result.is_empty() && !result.ends_with('/') {
            result.push('/');
        }
        result.push_str(&s);
    }

    Ok(js_string!(&*normalize_path_str(&result)).into())
}

// --- path.resolve(...segments) ---

fn path_resolve(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut resolved = cwd;

    for arg in args {
        let s = arg.to_string(context)?.to_std_string_escaped();
        let p = Path::new(&s);
        if p.is_absolute() {
            resolved = p.to_path_buf();
        } else {
            resolved = resolved.join(p);
        }
    }

    // Normalize
    let normalized = normalize_pathbuf(&resolved);
    let s = normalized.to_string_lossy().to_string();
    Ok(js_string!(&*s).into())
}

// --- path.basename(path, ext?) ---

fn path_basename(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    let ext = args.get(1).map(|v| v.to_string(context).map(|s| s.to_std_string_escaped())).transpose()?;

    let base = Path::new(&p)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let result = if let Some(ext) = ext {
        if base.ends_with(&ext) {
            base[..base.len() - ext.len()].to_string()
        } else {
            base
        }
    } else {
        base
    };

    Ok(js_string!(&*result).into())
}

// --- path.dirname(path) ---

fn path_dirname(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    let dir = Path::new(&p)
        .parent()
        .map(|d| {
            let s = d.to_string_lossy().to_string();
            if s.is_empty() { ".".to_string() } else { s }
        })
        .unwrap_or_else(|| ".".to_string());
    Ok(js_string!(&*dir).into())
}

// --- path.extname(path) ---

fn path_extname(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    let ext = Path::new(&p)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    Ok(js_string!(&*ext).into())
}

// --- path.parse(path) ---

fn path_parse(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    let path = Path::new(&p);

    let root = if path.is_absolute() { "/" } else { "" };
    let dir = path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let base = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

    let obj = JsObject::with_object_proto(context.intrinsics());
    obj.set(js_string!("root"), js_string!(root), false, context)?;
    obj.set(js_string!("dir"), js_string!(&*dir), false, context)?;
    obj.set(js_string!("base"), js_string!(&*base), false, context)?;
    obj.set(js_string!("ext"), js_string!(&*ext), false, context)?;
    obj.set(js_string!("name"), js_string!(&*name), false, context)?;

    Ok(obj.into())
}

// --- path.format(pathObject) ---

fn path_format(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let obj = args.first().and_then(|v| v.as_object());
    let obj = match obj {
        Some(o) => o,
        None => return Ok(js_string!("").into()),
    };

    let dir = obj.get(js_string!("dir"), context)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let root = obj.get(js_string!("root"), context)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let base = obj.get(js_string!("base"), context)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let name = obj.get(js_string!("name"), context)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let ext = obj.get(js_string!("ext"), context)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();

    let filename = if !base.is_empty() {
        base
    } else {
        let mut f = name;
        if !ext.is_empty() {
            if ext.starts_with('.') {
                f.push_str(&ext);
            } else {
                f.push('.');
                f.push_str(&ext);
            }
        }
        f
    };

    let result = if !dir.is_empty() {
        if dir.ends_with('/') {
            format!("{dir}{filename}")
        } else {
            format!("{dir}/{filename}")
        }
    } else if !root.is_empty() {
        format!("{root}{filename}")
    } else {
        filename
    };

    Ok(js_string!(&*result).into())
}

// --- path.normalize(path) ---

fn path_normalize(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    if p.is_empty() {
        return Ok(js_string!(".").into());
    }
    Ok(js_string!(&*normalize_path_str(&p)).into())
}

// --- path.isAbsolute(path) ---

fn path_is_absolute(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let p = arg_str(args, 0, context)?;
    Ok(JsValue::from(p.starts_with('/')))
}

// --- path.relative(from, to) ---

fn path_relative(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let from = arg_str(args, 0, context)?;
    let to = arg_str(args, 1, context)?;

    // Make both absolute
    let cwd = std::env::current_dir().unwrap_or_default();
    let from_abs = if Path::new(&from).is_absolute() {
        normalize_pathbuf(&PathBuf::from(&from))
    } else {
        normalize_pathbuf(&cwd.join(&from))
    };
    let to_abs = if Path::new(&to).is_absolute() {
        normalize_pathbuf(&PathBuf::from(&to))
    } else {
        normalize_pathbuf(&cwd.join(&to))
    };

    // Find common prefix
    let from_parts: Vec<_> = from_abs.components().collect();
    let to_parts: Vec<_> = to_abs.components().collect();

    let common_len = from_parts.iter().zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();
    for _ in common_len..from_parts.len() {
        result.push("..");
    }
    for part in &to_parts[common_len..] {
        result.push(part);
    }

    let s = result.to_string_lossy().to_string();
    Ok(js_string!(if s.is_empty() { "." } else { &s }).into())
}

// --- Normalize helpers ---

fn normalize_path_str(p: &str) -> String {
    let is_absolute = p.starts_with('/');
    let trailing_slash = p.len() > 1 && p.ends_with('/');

    let mut parts: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !is_absolute && (parts.is_empty() || parts.last() == Some(&"..")) {
                    parts.push("..");
                } else if !parts.is_empty() && parts.last() != Some(&"..") {
                    parts.pop();
                }
            }
            _ => parts.push(part),
        }
    }

    let mut result = if is_absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    };

    if trailing_slash && !result.ends_with('/') && result != "/" {
        result.push('/');
    }

    result
}

fn normalize_pathbuf(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => { result.pop(); }
            Component::CurDir => {}
            _ => result.push(component),
        }
    }
    result
}
