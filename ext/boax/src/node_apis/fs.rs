use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::SystemTime;

use boa_engine::{
    Context, JsNativeError, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
    object::builtins::JsArray,
};

const EXPORT_NAMES: &[&str] = &[
    "default",
    "readFileSync", "writeFileSync", "appendFileSync",
    "existsSync", "accessSync",
    "mkdirSync", "rmdirSync", "rmSync",
    "readdirSync",
    "statSync", "lstatSync",
    "unlinkSync", "renameSync", "copyFileSync",
    "chmodSync", "chownSync",
    "realpathSync",
];

pub fn create_module(context: &mut Context) -> Module {
    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();

    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_fs_module),
        None,
        None,
        context,
    )
}

fn init_fs_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let obj = build_fs_object(context)?;

    module.set_export(&js_string!("default"), obj.clone().into())?;
    for &name in &EXPORT_NAMES[1..] {
        let val = obj.get(js_string!(name), context)?;
        module.set_export(&js_string!(name), val)?;
    }

    Ok(())
}

fn build_fs_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    set_fn(&obj, "readFileSync", 1, fs_read_file_sync, context)?;
    set_fn(&obj, "writeFileSync", 2, fs_write_file_sync, context)?;
    set_fn(&obj, "appendFileSync", 2, fs_append_file_sync, context)?;
    set_fn(&obj, "existsSync", 1, fs_exists_sync, context)?;
    set_fn(&obj, "accessSync", 1, fs_access_sync, context)?;
    set_fn(&obj, "mkdirSync", 1, fs_mkdir_sync, context)?;
    set_fn(&obj, "rmdirSync", 1, fs_rmdir_sync, context)?;
    set_fn(&obj, "rmSync", 1, fs_rm_sync, context)?;
    set_fn(&obj, "readdirSync", 1, fs_readdir_sync, context)?;
    set_fn(&obj, "statSync", 1, fs_stat_sync, context)?;
    set_fn(&obj, "lstatSync", 1, fs_lstat_sync, context)?;
    set_fn(&obj, "unlinkSync", 1, fs_unlink_sync, context)?;
    set_fn(&obj, "renameSync", 2, fs_rename_sync, context)?;
    set_fn(&obj, "copyFileSync", 2, fs_copy_file_sync, context)?;
    set_fn(&obj, "chmodSync", 2, fs_chmod_sync, context)?;
    set_fn(&obj, "chownSync", 3, fs_chown_sync, context)?;
    set_fn(&obj, "realpathSync", 1, fs_realpath_sync, context)?;

    Ok(obj)
}

fn set_fn(
    obj: &JsObject,
    name: &str,
    _length: usize,
    f: fn(&JsValue, &[JsValue], &mut Context) -> boa_engine::JsResult<JsValue>,
    context: &mut Context,
) -> boa_engine::JsResult<()> {
    let func = NativeFunction::from_fn_ptr(f).to_js_function(context.realm());
    obj.set(js_string!(name), JsValue::from(func), false, context)?;
    Ok(())
}

// --- Helpers ---

fn arg_string(args: &[JsValue], idx: usize, context: &mut Context) -> boa_engine::JsResult<String> {
    args.get(idx)
        .ok_or_else(|| JsNativeError::typ().with_message("missing required argument"))?
        .to_string(context)
        .map(|s| s.to_std_string_escaped())
}

fn io_error(err: std::io::Error, op: &str, path: &str) -> boa_engine::JsError {
    let code = match err.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        std::io::ErrorKind::AlreadyExists => "EEXIST",
        std::io::ErrorKind::NotADirectory => "ENOTDIR",
        std::io::ErrorKind::IsADirectory => "EISDIR",
        _ => "EIO",
    };
    JsNativeError::typ()
        .with_message(format!("{code}: {err}, {op} '{path}'"))
        .into()
}

fn get_option_bool(args: &[JsValue], idx: usize, key: &str, context: &mut Context) -> boa_engine::JsResult<bool> {
    if let Some(opts) = args.get(idx).and_then(|v| v.as_object()) {
        let val = opts.get(js_string!(key), context)?;
        Ok(val.as_boolean().unwrap_or(false))
    } else {
        Ok(false)
    }
}

fn get_option_string(args: &[JsValue], idx: usize, key: &str, context: &mut Context) -> boa_engine::JsResult<Option<String>> {
    if let Some(opts) = args.get(idx).and_then(|v| v.as_object()) {
        let val = opts.get(js_string!(key), context)?;
        if val.is_string() {
            Ok(Some(val.as_string().unwrap().to_std_string_escaped()))
        } else {
            Ok(None)
        }
    } else if let Some(s) = args.get(idx).and_then(|v| v.as_string()) {
        // Second arg can be encoding string directly: readFileSync(path, 'utf8')
        Ok(Some(s.to_std_string_escaped()))
    } else {
        Ok(None)
    }
}

/// Build a Stats-like JS object from std::fs::Metadata.
fn metadata_to_stats(meta: &std::fs::Metadata, context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    // Size
    obj.set(js_string!("size"), JsValue::from(meta.len() as f64), false, context)?;

    // Mode, uid, gid (Unix)
    obj.set(js_string!("mode"), JsValue::from(meta.mode() as f64), false, context)?;
    obj.set(js_string!("uid"), JsValue::from(meta.uid() as f64), false, context)?;
    obj.set(js_string!("gid"), JsValue::from(meta.gid() as f64), false, context)?;
    obj.set(js_string!("nlink"), JsValue::from(meta.nlink() as f64), false, context)?;
    obj.set(js_string!("ino"), JsValue::from(meta.ino() as f64), false, context)?;
    obj.set(js_string!("dev"), JsValue::from(meta.dev() as f64), false, context)?;
    obj.set(js_string!("rdev"), JsValue::from(meta.rdev() as f64), false, context)?;
    obj.set(js_string!("blksize"), JsValue::from(meta.blksize() as f64), false, context)?;
    obj.set(js_string!("blocks"), JsValue::from(meta.blocks() as f64), false, context)?;

    // Timestamps as Date objects
    set_time_field(&obj, "atime", "atimeMs", meta.accessed(), context)?;
    set_time_field(&obj, "mtime", "mtimeMs", meta.modified(), context)?;
    set_time_field(&obj, "ctime", "ctimeMs", meta.modified(), context)?; // ctime ≈ mtime on macOS
    set_time_field(&obj, "birthtime", "birthtimeMs", meta.created(), context)?;

    // Type flags for method implementations
    let is_file = meta.is_file();
    let is_dir = meta.is_dir();
    let is_symlink = meta.is_symlink();

    // isFile()
    let is_file_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, is_file, _ctx| Ok(JsValue::from(*is_file)),
        is_file,
    ).to_js_function(context.realm());
    obj.set(js_string!("isFile"), JsValue::from(is_file_fn), false, context)?;

    // isDirectory()
    let is_dir_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, is_dir, _ctx| Ok(JsValue::from(*is_dir)),
        is_dir,
    ).to_js_function(context.realm());
    obj.set(js_string!("isDirectory"), JsValue::from(is_dir_fn), false, context)?;

    // isSymbolicLink()
    let is_symlink_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, is_symlink, _ctx| Ok(JsValue::from(*is_symlink)),
        is_symlink,
    ).to_js_function(context.realm());
    obj.set(js_string!("isSymbolicLink"), JsValue::from(is_symlink_fn), false, context)?;

    // isBlockDevice(), isCharacterDevice(), isFIFO(), isSocket() — always false for now
    for name in ["isBlockDevice", "isCharacterDevice", "isFIFO", "isSocket"] {
        let false_fn = NativeFunction::from_copy_closure(
            |_, _, _ctx| Ok(JsValue::from(false)),
        ).to_js_function(context.realm());
        obj.set(js_string!(name), JsValue::from(false_fn), false, context)?;
    }

    Ok(obj)
}

fn set_time_field(
    obj: &JsObject,
    name: &str,
    ms_name: &str,
    time: std::io::Result<SystemTime>,
    context: &mut Context,
) -> boa_engine::JsResult<()> {
    let ms = time
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);

    // Set millisecond timestamp
    obj.set(js_string!(ms_name), JsValue::from(ms), false, context)?;

    // Set Date object
    let date_ctor = context.global_object().get(js_string!("Date"), context)?;
    if let Some(date_obj) = date_ctor.as_object() {
        if let Ok(date) = date_obj.construct(&[JsValue::from(ms)], None, context) {
            obj.set(js_string!(name), JsValue::from(date), false, context)?;
        }
    }

    Ok(())
}

/// Build a Dirent-like JS object.
fn dirent_object(name: &str, file_type: std::fs::FileType, context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());
    obj.set(js_string!("name"), js_string!(name), false, context)?;

    let is_file = file_type.is_file();
    let is_dir = file_type.is_dir();
    let is_symlink = file_type.is_symlink();

    let is_file_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, v, _| Ok(JsValue::from(*v)), is_file,
    ).to_js_function(context.realm());
    obj.set(js_string!("isFile"), JsValue::from(is_file_fn), false, context)?;

    let is_dir_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, v, _| Ok(JsValue::from(*v)), is_dir,
    ).to_js_function(context.realm());
    obj.set(js_string!("isDirectory"), JsValue::from(is_dir_fn), false, context)?;

    let is_symlink_fn = NativeFunction::from_copy_closure_with_captures(
        |_, _, v, _| Ok(JsValue::from(*v)), is_symlink,
    ).to_js_function(context.realm());
    obj.set(js_string!("isSymbolicLink"), JsValue::from(is_symlink_fn), false, context)?;

    Ok(obj)
}

// --- fs functions ---

fn fs_read_file_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let encoding = get_option_string(args, 1, "encoding", context)?;

    let contents = std::fs::read_to_string(&path)
        .map_err(|e| io_error(e, "readFileSync", &path))?;

    // Always return string for now (encoding defaults to utf8)
    let _ = encoding; // acknowledged but utf8 is the only supported encoding
    Ok(js_string!(&*contents).into())
}

fn fs_write_file_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let data = arg_string(args, 1, context)?;

    std::fs::write(&path, &data)
        .map_err(|e| io_error(e, "writeFileSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_append_file_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    use std::io::Write;
    let path = arg_string(args, 0, context)?;
    let data = arg_string(args, 1, context)?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| io_error(e, "appendFileSync", &path))?;

    file.write_all(data.as_bytes())
        .map_err(|e| io_error(e, "appendFileSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_exists_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    Ok(JsValue::from(Path::new(&path).exists()))
}

fn fs_access_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    // Just check if the path exists and is accessible
    std::fs::metadata(&path)
        .map_err(|e| io_error(e, "accessSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_mkdir_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let recursive = get_option_bool(args, 1, "recursive", context)?;

    if recursive {
        std::fs::create_dir_all(&path)
    } else {
        std::fs::create_dir(&path)
    }
    .map_err(|e| io_error(e, "mkdirSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_rmdir_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    std::fs::remove_dir(&path)
        .map_err(|e| io_error(e, "rmdirSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_rm_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let recursive = get_option_bool(args, 1, "recursive", context)?;
    let force = get_option_bool(args, 1, "force", context)?;

    let p = Path::new(&path);

    if !p.exists() {
        if force {
            return Ok(JsValue::undefined());
        }
        return Err(io_error(
            std::io::Error::new(std::io::ErrorKind::NotFound, "no such file or directory"),
            "rmSync", &path,
        ));
    }

    if p.is_dir() {
        if recursive {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_dir(&path)
        }
    } else {
        std::fs::remove_file(&path)
    }
    .map_err(|e| io_error(e, "rmSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_readdir_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let with_file_types = get_option_bool(args, 1, "withFileTypes", context)?;

    let entries = std::fs::read_dir(&path)
        .map_err(|e| io_error(e, "readdirSync", &path))?;

    let arr = JsArray::new(context);

    for entry in entries {
        let entry = entry.map_err(|e| io_error(e, "readdirSync", &path))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if with_file_types {
            let ft = entry.file_type().map_err(|e| io_error(e, "readdirSync", &path))?;
            let dirent = dirent_object(&name, ft, context)?;
            arr.push(JsValue::from(dirent), context)?;
        } else {
            arr.push(js_string!(&*name), context)?;
        }
    }

    Ok(arr.into())
}

fn fs_stat_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    let meta = std::fs::metadata(&path)
        .map_err(|e| io_error(e, "statSync", &path))?;

    let stats = metadata_to_stats(&meta, context)?;
    Ok(stats.into())
}

fn fs_lstat_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    let meta = std::fs::symlink_metadata(&path)
        .map_err(|e| io_error(e, "lstatSync", &path))?;

    let stats = metadata_to_stats(&meta, context)?;
    Ok(stats.into())
}

fn fs_unlink_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    std::fs::remove_file(&path)
        .map_err(|e| io_error(e, "unlinkSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_rename_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let old_path = arg_string(args, 0, context)?;
    let new_path = arg_string(args, 1, context)?;

    std::fs::rename(&old_path, &new_path)
        .map_err(|e| io_error(e, "renameSync", &old_path))?;

    Ok(JsValue::undefined())
}

fn fs_copy_file_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let src = arg_string(args, 0, context)?;
    let dest = arg_string(args, 1, context)?;

    std::fs::copy(&src, &dest)
        .map_err(|e| io_error(e, "copyFileSync", &src))?;

    Ok(JsValue::undefined())
}

fn fs_chmod_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let mode = args.get(1)
        .and_then(|v| v.as_number())
        .ok_or_else(|| JsNativeError::typ().with_message("mode must be a number"))? as u32;

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| io_error(e, "chmodSync", &path))?;

    Ok(JsValue::undefined())
}

fn fs_chown_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;
    let uid = args.get(1)
        .and_then(|v| v.as_number())
        .ok_or_else(|| JsNativeError::typ().with_message("uid must be a number"))? as u32;
    let gid = args.get(2)
        .and_then(|v| v.as_number())
        .ok_or_else(|| JsNativeError::typ().with_message("gid must be a number"))? as u32;

    // Use libc chown
    let c_path = std::ffi::CString::new(path.as_bytes())
        .map_err(|_| JsNativeError::typ().with_message("invalid path"))?;
    let result = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if result != 0 {
        return Err(io_error(std::io::Error::last_os_error(), "chownSync", &path));
    }

    Ok(JsValue::undefined())
}

fn fs_realpath_sync(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = arg_string(args, 0, context)?;

    let real = std::fs::canonicalize(&path)
        .map_err(|e| io_error(e, "realpathSync", &path))?;

    Ok(js_string!(&*real.to_string_lossy().to_string()).into())
}
