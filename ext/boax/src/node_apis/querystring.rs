use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
    property::PropertyKey,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "parse", "stringify", "escape", "unescape", "decode", "encode",
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
    set_fn(&obj, "parse", qs_parse, context)?;
    set_fn(&obj, "stringify", qs_stringify, context)?;
    set_fn(&obj, "escape", qs_escape, context)?;
    set_fn(&obj, "unescape", qs_unescape, context)?;
    // decode/encode are aliases
    set_fn(&obj, "decode", qs_parse, context)?;
    set_fn(&obj, "encode", qs_stringify, context)?;
    Ok(obj)
}

// querystring.parse(str, sep?, eq?)
fn qs_parse(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let input = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    let sep = args.get(1)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "&".to_string());
    let eq = args.get(2)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "=".to_string());

    let obj = JsObject::with_object_proto(ctx.intrinsics());

    for pair in input.split(&sep) {
        if pair.is_empty() { continue; }
        let (key, val) = if let Some(idx) = pair.find(&eq) {
            (&pair[..idx], &pair[idx + eq.len()..])
        } else {
            (pair, "")
        };
        let key = percent_decode(key);
        let val = percent_decode(val);

        let existing = obj.get(js_string!(&*key), ctx)?;
        if existing.is_undefined() {
            obj.set(js_string!(&*key), js_string!(&*val), false, ctx)?;
        } else if let Some(arr_obj) = existing.as_object().filter(|o| o.is_array()) {
            // Already an array, push
            let len = arr_obj.get(js_string!("length"), ctx)?.as_number().unwrap_or(0.0) as u32;
            arr_obj.set(len, js_string!(&*val), false, ctx)?;
        } else {
            // Convert to array
            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            arr.push(existing, ctx)?;
            arr.push(js_string!(&*val), ctx)?;
            obj.set(js_string!(&*key), JsValue::from(arr), false, ctx)?;
        }
    }

    Ok(obj.into())
}

// querystring.stringify(obj, sep?, eq?)
fn qs_stringify(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let obj = match args.first().and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return Ok(js_string!("").into()),
    };
    let sep = args.get(1)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "&".to_string());
    let eq = args.get(2)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "=".to_string());

    let keys = obj.own_property_keys(ctx)?;
    let mut parts = Vec::new();

    for key in keys {
        let key_str = match &key {
            PropertyKey::String(s) => s.to_std_string_escaped(),
            PropertyKey::Index(i) => i.get().to_string(),
            PropertyKey::Symbol(_) => continue,
        };
        let val = obj.get(key.clone(), ctx)?;

        if let Some(arr_obj) = val.as_object().filter(|o| o.is_array()) {
            let len = arr_obj.get(js_string!("length"), ctx)?.as_number().unwrap_or(0.0) as u32;
            for i in 0..len {
                let item = arr_obj.get(i, ctx)?;
                let item_str = item.to_string(ctx)?.to_std_string_escaped();
                parts.push(format!("{}{}{}", percent_encode(&key_str), eq, percent_encode(&item_str)));
            }
        } else {
            let val_str = val.to_string(ctx)?.to_std_string_escaped();
            parts.push(format!("{}{}{}", percent_encode(&key_str), eq, percent_encode(&val_str)));
        }
    }

    Ok(js_string!(&*parts.join(&sep)).into())
}

fn qs_escape(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let s = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    Ok(js_string!(&*percent_encode(&s)).into())
}

fn qs_unescape(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let s = args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()?
        .unwrap_or_default();
    Ok(js_string!(&*percent_decode(&s)).into())
}

fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{byte:02X}"));
            }
        }
    }
    result
}

fn percent_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &s[i + 1..i + 3], 16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}
