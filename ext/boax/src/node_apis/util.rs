use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "format", "inspect", "types", "inherits", "isDeepStrictEqual",
];

pub fn create_module(context: &mut Context) -> Module {
    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();

    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_util_module),
        None,
        None,
        context,
    )
}

fn init_util_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let obj = build_util_object(context)?;

    module.set_export(&js_string!("default"), obj.clone().into())?;
    for &name in &EXPORT_NAMES[1..] {
        let val = obj.get(js_string!(name), context)?;
        module.set_export(&js_string!(name), val)?;
    }

    Ok(())
}

fn build_util_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    set_fn(&obj, "format", 1, util_format, context)?;
    set_fn(&obj, "inspect", 1, util_inspect, context)?;
    set_fn(&obj, "inherits", 2, util_inherits, context)?;
    set_fn(&obj, "isDeepStrictEqual", 2, util_is_deep_strict_equal, context)?;

    // util.types namespace
    let types = build_types_object(context)?;
    obj.set(js_string!("types"), JsValue::from(types), false, context)?;

    Ok(obj)
}

fn build_types_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());
    set_fn(&obj, "isDate", 1, types_is_date, context)?;
    set_fn(&obj, "isRegExp", 1, types_is_regexp, context)?;
    set_fn(&obj, "isNativeError", 1, types_is_native_error, context)?;
    set_fn(&obj, "isPromise", 1, types_is_promise, context)?;
    set_fn(&obj, "isArrayBuffer", 1, types_is_array_buffer, context)?;
    set_fn(&obj, "isMap", 1, types_is_map, context)?;
    set_fn(&obj, "isSet", 1, types_is_set, context)?;
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

// --- util.format(fmt, ...args) ---
// Simplified implementation of Node's util.format
fn util_format(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    if args.is_empty() {
        return Ok(js_string!("").into());
    }

    let first = &args[0];

    // If first arg is not a string, just join all with spaces
    if !first.is_string() {
        let parts: Vec<String> = args.iter()
            .map(|a| format_value(a, context))
            .collect::<boa_engine::JsResult<_>>()?;
        return Ok(js_string!(&*parts.join(" ")).into());
    }

    let fmt = first.as_string().unwrap().to_std_string_escaped();
    let mut result = String::new();
    let mut arg_idx = 1;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&spec) = chars.peek() {
                match spec {
                    's' => {
                        chars.next();
                        if arg_idx < args.len() {
                            let s = args[arg_idx].to_string(context)?.to_std_string_escaped();
                            result.push_str(&s);
                            arg_idx += 1;
                        } else {
                            result.push_str("%s");
                        }
                    }
                    'd' | 'i' => {
                        chars.next();
                        if arg_idx < args.len() {
                            if let Some(n) = args[arg_idx].as_number() {
                                if spec == 'i' {
                                    result.push_str(&(n as i64).to_string());
                                } else {
                                    result.push_str(&n.to_string());
                                }
                            } else {
                                result.push_str("NaN");
                            }
                            arg_idx += 1;
                        } else {
                            result.push('%');
                            result.push(spec);
                        }
                    }
                    'f' => {
                        chars.next();
                        if arg_idx < args.len() {
                            if let Some(n) = args[arg_idx].as_number() {
                                result.push_str(&n.to_string());
                            } else {
                                result.push_str("NaN");
                            }
                            arg_idx += 1;
                        } else {
                            result.push_str("%f");
                        }
                    }
                    'j' => {
                        chars.next();
                        if arg_idx < args.len() {
                            let json = context.global_object().get(js_string!("JSON"), context)?;
                            if let Some(json_obj) = json.as_object() {
                                let stringify = json_obj.get(js_string!("stringify"), context)?;
                                if let Some(fn_obj) = stringify.as_object() {
                                    let s = fn_obj.call(&json, &[args[arg_idx].clone()], context)?;
                                    result.push_str(&s.to_string(context)?.to_std_string_escaped());
                                }
                            }
                            arg_idx += 1;
                        } else {
                            result.push_str("%j");
                        }
                    }
                    'o' | 'O' => {
                        chars.next();
                        if arg_idx < args.len() {
                            let s = format_value(&args[arg_idx], context)?;
                            result.push_str(&s);
                            arg_idx += 1;
                        } else {
                            result.push('%');
                            result.push(spec);
                        }
                    }
                    '%' => {
                        chars.next();
                        result.push('%');
                    }
                    _ => {
                        result.push('%');
                    }
                }
            } else {
                result.push('%');
            }
        } else {
            result.push(c);
        }
    }

    // Append remaining args
    for i in arg_idx..args.len() {
        result.push(' ');
        result.push_str(&format_value(&args[i], context)?);
    }

    Ok(js_string!(&*result).into())
}

fn format_value(val: &JsValue, context: &mut Context) -> boa_engine::JsResult<String> {
    if val.is_string() {
        Ok(format!("'{}'", val.as_string().unwrap().to_std_string_escaped()))
    } else {
        Ok(val.to_string(context)?.to_std_string_escaped())
    }
}

// --- util.inspect(obj) ---
// Simplified: delegates to toString for now
fn util_inspect(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    let s = val.to_string(context)?;
    Ok(s.into())
}

// --- util.inherits(ctor, superCtor) ---
fn util_inherits(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let ctor = args.first().and_then(|v| v.as_object()).ok_or_else(|| {
        boa_engine::JsNativeError::typ().with_message("The \"ctor\" argument must be a function")
    })?;
    let super_ctor = args.get(1).and_then(|v| v.as_object()).ok_or_else(|| {
        boa_engine::JsNativeError::typ().with_message("The \"superCtor\" argument must be a function")
    })?;

    // ctor.prototype = Object.create(superCtor.prototype)
    let super_proto = super_ctor.get(js_string!("prototype"), context)?;
    let object_ctor = context.global_object().get(js_string!("Object"), context)?;
    if let Some(object_obj) = object_ctor.as_object() {
        let create_fn = object_obj.get(js_string!("create"), context)?;
        if let Some(create_obj) = create_fn.as_object() {
            let new_proto = create_obj.call(&object_ctor, &[super_proto], context)?;
            ctor.set(js_string!("prototype"), new_proto, false, context)?;
        }
    }

    // ctor.super_ = superCtor
    ctor.set(js_string!("super_"), JsValue::from(super_ctor.clone()), false, context)?;

    Ok(JsValue::undefined())
}

// --- util.isDeepStrictEqual(a, b) ---
// Simplified: uses JSON.stringify comparison
fn util_is_deep_strict_equal(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let a = args.first().cloned().unwrap_or(JsValue::undefined());
    let b = args.get(1).cloned().unwrap_or(JsValue::undefined());

    // For primitives, use strict equality
    if !a.is_object() && !b.is_object() {
        return Ok(JsValue::from(a.strict_equals(&b)));
    }

    // For objects, use JSON serialization comparison (simplified)
    let json = context.global_object().get(js_string!("JSON"), context)?;
    if let Some(json_obj) = json.as_object() {
        let stringify = json_obj.get(js_string!("stringify"), context)?;
        if let Some(fn_obj) = stringify.as_object() {
            let sa = fn_obj.call(&json, &[a], context)?;
            let sb = fn_obj.call(&json, &[b], context)?;
            return Ok(JsValue::from(sa.strict_equals(&sb)));
        }
    }

    Ok(JsValue::from(false))
}

// --- util.types.* ---
// Use Object.prototype.toString.call() for reliable type checking,
// since Boa's internal types are not all public.

fn check_to_string_tag(val: &JsValue, tag: &str, context: &mut Context) -> boa_engine::JsResult<bool> {
    if !val.is_object() {
        return Ok(false);
    }
    let to_str = context.eval(boa_engine::Source::from_bytes(
        "Object.prototype.toString"
    ))?;
    if let Some(fn_obj) = to_str.as_object() {
        let result = fn_obj.call(val, &[], context)?;
        let s = result.to_string(context)?.to_std_string_escaped();
        Ok(s == tag)
    } else {
        Ok(false)
    }
}

fn types_is_date(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object Date]", context)?))
}

fn types_is_regexp(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object RegExp]", context)?))
}

fn types_is_native_error(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object Error]", context)?))
}

fn types_is_promise(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object Promise]", context)?))
}

fn types_is_array_buffer(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object ArrayBuffer]", context)?))
}

fn types_is_map(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object Map]", context)?))
}

fn types_is_set(_this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    Ok(JsValue::from(check_to_string_tag(&val, "[object Set]", context)?))
}
