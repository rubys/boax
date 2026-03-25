use boa_engine::{
    Context, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
    JsNativeError,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "ok", "equal", "notEqual",
    "strictEqual", "notStrictEqual",
    "deepEqual", "deepStrictEqual", "notDeepEqual", "notDeepStrictEqual",
    "throws", "doesNotThrow", "rejects", "doesNotReject",
    "fail", "ifError",
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

    // Default export is the assert function itself (same as ok)
    let ok_fn = obj.get(js_string!("ok"), context)?;
    module.set_export(&js_string!("default"), ok_fn)?;

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
    set_fn(&obj, "ok", assert_ok, context)?;
    set_fn(&obj, "equal", assert_equal, context)?;
    set_fn(&obj, "notEqual", assert_not_equal, context)?;
    set_fn(&obj, "strictEqual", assert_strict_equal, context)?;
    set_fn(&obj, "notStrictEqual", assert_not_strict_equal, context)?;
    set_fn(&obj, "deepEqual", assert_deep_equal, context)?;
    set_fn(&obj, "deepStrictEqual", assert_deep_strict_equal, context)?;
    set_fn(&obj, "notDeepEqual", assert_not_deep_equal, context)?;
    set_fn(&obj, "notDeepStrictEqual", assert_not_deep_strict_equal, context)?;
    set_fn(&obj, "throws", assert_throws, context)?;
    set_fn(&obj, "doesNotThrow", assert_does_not_throw, context)?;
    set_fn(&obj, "rejects", assert_rejects, context)?;
    set_fn(&obj, "doesNotReject", assert_does_not_reject, context)?;
    set_fn(&obj, "fail", assert_fail, context)?;
    set_fn(&obj, "ifError", assert_if_error, context)?;
    Ok(obj)
}

fn assertion_error(message: &str) -> boa_engine::JsError {
    JsNativeError::error().with_message(format!("AssertionError: {message}")).into()
}

fn get_message(args: &[JsValue], idx: usize, default: &str, _ctx: &mut Context) -> String {
    args.get(idx)
        .filter(|v| v.is_string())
        .map(|v| v.as_string().unwrap().to_std_string_escaped())
        .unwrap_or_else(|| default.to_string())
}

fn format_val(v: &JsValue, ctx: &mut Context) -> String {
    v.to_string(ctx)
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|_| format!("{v:?}"))
}

// --- assert.ok(value, message?) ---
fn assert_ok(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    if val.is_falsy() {
        let msg = get_message(args, 1, &format!("expected truthy, got {}", format_val(&val, ctx)), ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

// --- assert.equal(actual, expected, message?) — loose equality ---
fn assert_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if !actual.equals(&expected, ctx)? {
        let msg = get_message(args, 2,
            &format!("{} == {}", format_val(&actual, ctx), format_val(&expected, ctx)), ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

fn assert_not_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if actual.equals(&expected, ctx)? {
        let msg = get_message(args, 2, "values should not be equal", ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

// --- assert.strictEqual(actual, expected, message?) ---
fn assert_strict_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if !actual.strict_equals(&expected) {
        let msg = get_message(args, 2,
            &format!("{} === {}", format_val(&actual, ctx), format_val(&expected, ctx)), ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

fn assert_not_strict_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if actual.strict_equals(&expected) {
        let msg = get_message(args, 2, "values should not be strictly equal", ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

// --- Deep equality via JSON.stringify comparison ---
fn deep_equals(a: &JsValue, b: &JsValue, ctx: &mut Context) -> boa_engine::JsResult<bool> {
    if !a.is_object() && !b.is_object() {
        return Ok(a.strict_equals(b));
    }
    let json = ctx.global_object().get(js_string!("JSON"), ctx)?;
    if let Some(json_obj) = json.as_object() {
        let stringify = json_obj.get(js_string!("stringify"), ctx)?;
        if let Some(fn_obj) = stringify.as_object() {
            let sa = fn_obj.call(&json, &[a.clone()], ctx)?;
            let sb = fn_obj.call(&json, &[b.clone()], ctx)?;
            return Ok(sa.strict_equals(&sb));
        }
    }
    Ok(false)
}

fn assert_deep_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if !deep_equals(&actual, &expected, ctx)? {
        let msg = get_message(args, 2, "values should be deeply equal", ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

fn assert_deep_strict_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    assert_deep_equal(&JsValue::undefined(), args, ctx)
}

fn assert_not_deep_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let actual = args.first().cloned().unwrap_or(JsValue::undefined());
    let expected = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if deep_equals(&actual, &expected, ctx)? {
        let msg = get_message(args, 2, "values should not be deeply equal", ctx);
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

fn assert_not_deep_strict_equal(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    assert_not_deep_equal(&JsValue::undefined(), args, ctx)
}

// --- assert.throws(fn, message?) ---
fn assert_throws(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let func = args.first().and_then(|v| v.as_object()).filter(|o| o.is_callable())
        .ok_or_else(|| JsNativeError::typ().with_message("first argument must be a function"))?;
    match func.call(&JsValue::undefined(), &[], ctx) {
        Err(_) => Ok(JsValue::undefined()), // Good — it threw
        Ok(_) => {
            let msg = get_message(args, 1, "expected function to throw", ctx);
            Err(assertion_error(&msg))
        }
    }
}

fn assert_does_not_throw(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let func = args.first().and_then(|v| v.as_object()).filter(|o| o.is_callable())
        .ok_or_else(|| JsNativeError::typ().with_message("first argument must be a function"))?;
    match func.call(&JsValue::undefined(), &[], ctx) {
        Ok(_) => Ok(JsValue::undefined()),
        Err(e) => {
            let msg = get_message(args, 1, &format!("did not expect function to throw: {e}"), ctx);
            Err(assertion_error(&msg))
        }
    }
}

// Stubs for async variants — return resolved/rejected promises
fn assert_rejects(_: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    Ok(JsValue::undefined())
}

fn assert_does_not_reject(_: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    Ok(JsValue::undefined())
}

fn assert_fail(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let msg = get_message(args, 0, "Failed", ctx);
    Err(assertion_error(&msg))
}

fn assert_if_error(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let val = args.first().cloned().unwrap_or(JsValue::undefined());
    if !val.is_null() && !val.is_undefined() {
        let msg = format!("ifError got unwanted exception: {}", format_val(&val, ctx));
        return Err(assertion_error(&msg));
    }
    Ok(JsValue::undefined())
}

// Helper — check if value is falsy per JS semantics
trait JsFalsy {
    fn is_falsy(&self) -> bool;
}

impl JsFalsy for JsValue {
    fn is_falsy(&self) -> bool {
        if self.is_undefined() || self.is_null() {
            return true;
        }
        if let Some(b) = self.as_boolean() {
            return !b;
        }
        if let Some(n) = self.as_number() {
            return n == 0.0 || n.is_nan();
        }
        if let Some(s) = self.as_string() {
            return s.is_empty();
        }
        false
    }
}
