use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "URL", "URLSearchParams", "parse", "format", "resolve",
    "domainToASCII", "domainToUnicode", "fileURLToPath", "pathToFileURL",
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

    // Re-export the global URL and URLSearchParams constructors
    let global = context.global_object();
    let url_ctor = global.get(js_string!("URL"), context)?;
    let usp_ctor = global.get(js_string!("URLSearchParams"), context)?;
    obj.set(js_string!("URL"), url_ctor, false, context)?;
    obj.set(js_string!("URLSearchParams"), usp_ctor, false, context)?;

    // Legacy url.parse()
    set_fn(&obj, "parse", url_parse, context)?;
    set_fn(&obj, "format", url_format, context)?;
    set_fn(&obj, "resolve", url_resolve, context)?;
    set_fn(&obj, "fileURLToPath", file_url_to_path, context)?;
    set_fn(&obj, "pathToFileURL", path_to_file_url, context)?;
    set_fn(&obj, "domainToASCII", domain_to_ascii, context)?;
    set_fn(&obj, "domainToUnicode", domain_to_unicode, context)?;

    Ok(obj)
}

/// Legacy url.parse(urlString, parseQueryString?)
/// Returns an object with protocol, host, hostname, port, pathname, search, hash, etc.
fn url_parse(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let url_str = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();

    let obj = JsObject::with_object_proto(ctx.intrinsics());

    // Try to use the global URL constructor for parsing
    let global = ctx.global_object();
    let url_ctor = global.get(js_string!("URL"), ctx)?;

    if let Some(ctor_obj) = url_ctor.as_object().filter(|o| o.is_constructor()) {
        // Try parsing as absolute URL
        match ctor_obj.construct(&[js_string!(&*url_str).into()], None, ctx) {
            Ok(url_obj) => {
                // Extract parts from the URL object
                for prop in ["protocol", "hostname", "host", "port", "pathname", "search", "hash", "href", "origin"] {
                    let val = url_obj.get(js_string!(prop), ctx)?;
                    obj.set(js_string!(prop), val, false, ctx)?;
                }
                // Node's url.parse uses "path" (pathname + search) and "query" (without ?)
                let pathname = url_obj.get(js_string!("pathname"), ctx)?
                    .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
                let search = url_obj.get(js_string!("search"), ctx)?
                    .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
                let path = format!("{pathname}{search}");
                obj.set(js_string!("path"), js_string!(&*path), false, ctx)?;
                let query = search.strip_prefix('?').unwrap_or(&search);
                if query.is_empty() {
                    obj.set(js_string!("query"), JsValue::null(), false, ctx)?;
                } else {
                    obj.set(js_string!("query"), js_string!(query), false, ctx)?;
                }
                return Ok(obj.into());
            }
            Err(_) => {
                // Not a valid absolute URL — do basic parsing below
            }
        }
    }

    // Fallback: basic relative URL parsing
    obj.set(js_string!("href"), js_string!(&*url_str), false, ctx)?;
    obj.set(js_string!("protocol"), JsValue::null(), false, ctx)?;
    obj.set(js_string!("host"), JsValue::null(), false, ctx)?;
    obj.set(js_string!("hostname"), JsValue::null(), false, ctx)?;
    obj.set(js_string!("port"), JsValue::null(), false, ctx)?;

    // Split on ? and #
    let (path_part, rest) = url_str.split_once('?').map(|(a, b)| (a, Some(b))).unwrap_or((&url_str, None));
    let (query, hash) = if let Some(rest) = rest {
        rest.split_once('#').map(|(q, h)| (Some(q.to_string()), Some(format!("#{h}")))).unwrap_or((Some(rest.to_string()), None))
    } else {
        let (p, h) = path_part.split_once('#').map(|(a, b)| (a, Some(format!("#{b}")))).unwrap_or((path_part, None));
        obj.set(js_string!("pathname"), js_string!(p), false, ctx)?;
        (None, h)
    };

    if !path_part.contains('#') {
        obj.set(js_string!("pathname"), js_string!(path_part), false, ctx)?;
    }

    match &query {
        Some(q) => {
            obj.set(js_string!("search"), js_string!(&*format!("?{q}")), false, ctx)?;
            obj.set(js_string!("query"), js_string!(&**q), false, ctx)?;
        }
        None => {
            obj.set(js_string!("search"), JsValue::null(), false, ctx)?;
            obj.set(js_string!("query"), JsValue::null(), false, ctx)?;
        }
    }

    match &hash {
        Some(h) => obj.set(js_string!("hash"), js_string!(&**h), false, ctx)?,
        None => obj.set(js_string!("hash"), JsValue::null(), false, ctx)?,
    };

    Ok(obj.into())
}

/// url.format(urlObject)
fn url_format(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let obj = match args.first().and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return Ok(js_string!("").into()),
    };

    // Check if it's a URL instance (has href property that's a full URL)
    let href = obj.get(js_string!("href"), ctx)?;
    if href.is_string() {
        return Ok(href);
    }

    // Build from parts
    let protocol = obj.get(js_string!("protocol"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let hostname = obj.get(js_string!("hostname"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let port = obj.get(js_string!("port"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let pathname = obj.get(js_string!("pathname"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_else(|| "/".to_string());
    let search = obj.get(js_string!("search"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let hash = obj.get(js_string!("hash"), ctx)?.as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();

    let mut result = String::new();
    if !protocol.is_empty() {
        result.push_str(&protocol);
        if !protocol.ends_with(':') { result.push(':'); }
        result.push_str("//");
    }
    result.push_str(&hostname);
    if !port.is_empty() {
        result.push(':');
        result.push_str(&port);
    }
    result.push_str(&pathname);
    result.push_str(&search);
    result.push_str(&hash);

    Ok(js_string!(&*result).into())
}

/// url.resolve(from, to)
fn url_resolve(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let from = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    let to = args.get(1)
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();

    // Use the global URL constructor: new URL(to, from)
    let global = ctx.global_object();
    let url_ctor = global.get(js_string!("URL"), ctx)?;
    if let Some(ctor_obj) = url_ctor.as_object().filter(|o| o.is_constructor()) {
        if let Ok(url_obj) = ctor_obj.construct(&[js_string!(&*to).into(), js_string!(&*from).into()], None, ctx) {
            let href = url_obj.get(js_string!("href"), ctx)?;
            return Ok(href);
        }
    }

    // Fallback: simple concatenation
    Ok(js_string!(&*format!("{from}{to}")).into())
}

/// url.fileURLToPath(url)
fn file_url_to_path(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let url_str = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();

    let path = if let Some(stripped) = url_str.strip_prefix("file://") {
        stripped.to_string()
    } else {
        url_str
    };

    Ok(js_string!(&*path).into())
}

/// url.pathToFileURL(path)
fn path_to_file_url(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let path = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();

    // Return a URL object
    let url_str = format!("file://{path}");
    let global = ctx.global_object();
    let url_ctor = global.get(js_string!("URL"), ctx)?;
    if let Some(ctor_obj) = url_ctor.as_object().filter(|o| o.is_constructor()) {
        if let Ok(url_obj) = ctor_obj.construct(&[js_string!(&*url_str).into()], None, ctx) {
            return Ok(url_obj.into());
        }
    }
    Ok(js_string!(&*url_str).into())
}

fn domain_to_ascii(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let domain = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    // Pass through — real punycode conversion would need a dependency
    Ok(js_string!(&*domain).into())
}

fn domain_to_unicode(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let domain = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    Ok(js_string!(&*domain).into())
}
