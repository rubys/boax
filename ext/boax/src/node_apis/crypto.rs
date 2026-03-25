use boa_engine::{
    Context, JsNativeError, JsString, JsValue, Module, NativeFunction,
    js_string,
    module::SyntheticModuleInitializer,
    object::JsObject,
};

const EXPORT_NAMES: &[&str] = &[
    "default", "createHash", "createHmac", "randomBytes", "randomUUID",
    "getHashes",
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
    let obj = build_crypto_object(context)?;
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

fn build_crypto_object(context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());
    set_fn(&obj, "createHash", crypto_create_hash, context)?;
    set_fn(&obj, "createHmac", crypto_create_hmac, context)?;
    set_fn(&obj, "randomBytes", crypto_random_bytes, context)?;
    set_fn(&obj, "randomUUID", crypto_random_uuid, context)?;
    set_fn(&obj, "getHashes", crypto_get_hashes, context)?;
    Ok(obj)
}

fn arg_string(args: &[JsValue], idx: usize, context: &mut Context) -> boa_engine::JsResult<String> {
    args.get(idx)
        .ok_or_else(|| JsNativeError::typ().with_message("missing required argument"))?
        .to_string(context)
        .map(|s| s.to_std_string_escaped())
}

fn arg_bytes(args: &[JsValue], idx: usize, context: &mut Context) -> boa_engine::JsResult<Vec<u8>> {
    let val = args.get(idx)
        .ok_or_else(|| JsNativeError::typ().with_message("missing required argument"))?;

    if val.is_string() {
        Ok(val.as_string().unwrap().to_std_string_escaped().into_bytes())
    } else if let Some(obj) = val.as_object() {
        // Try to read Buffer._bytes or treat as array-like
        let bytes_prop = obj.get(js_string!("_bytes"), context)?;
        if let Some(bytes_obj) = bytes_prop.as_object() {
            let len = bytes_obj.get(js_string!("length"), context)?
                .as_number().unwrap_or(0.0) as u32;
            let mut bytes = Vec::with_capacity(len as usize);
            for i in 0..len {
                let b = bytes_obj.get(i, context)?.as_number().unwrap_or(0.0) as u8;
                bytes.push(b);
            }
            Ok(bytes)
        } else {
            Ok(val.to_string(context)?.to_std_string_escaped().into_bytes())
        }
    } else {
        Ok(val.to_string(context)?.to_std_string_escaped().into_bytes())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        result.push(CHARS[(b0 >> 2) as usize] as char);
        result.push(CHARS[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 15) << 2 | b2 >> 6) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(b2 & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn encode_digest(bytes: &[u8], encoding: &str) -> String {
    match encoding {
        "hex" => hex_encode(bytes),
        "base64" => base64_encode(bytes),
        "latin1" | "binary" => bytes.iter().map(|&b| b as char).collect(),
        _ => hex_encode(bytes), // default to hex
    }
}

/// Build a Hash object with .update() and .digest() methods.
/// The hasher state is stored as a hex string of accumulated input
/// (since we can't store Rust state in a JS object without Trace).
/// We hash all at once in .digest().
fn make_hash_object(algorithm: &str, context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    // Store algorithm and accumulated data
    obj.set(js_string!("_algorithm"), js_string!(algorithm), false, context)?;
    obj.set(js_string!("_data"), js_string!(""), false, context)?;

    set_fn(&obj, "update", hash_update, context)?;
    set_fn(&obj, "digest", hash_digest, context)?;

    Ok(obj)
}

fn hash_update(this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    let obj = this.as_object()
        .ok_or_else(|| JsNativeError::typ().with_message("not a Hash object"))?;
    let input = arg_bytes(args, 0, context)?;
    let existing = obj.get(js_string!("_data"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();

    // Accumulate as hex
    let mut combined = existing;
    combined.push_str(&hex_encode(&input));
    obj.set(js_string!("_data"), js_string!(&*combined), false, context)?;

    Ok(this.clone()) // return this for chaining
}

fn hash_digest(this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    use digest::Digest;

    let obj = this.as_object()
        .ok_or_else(|| JsNativeError::typ().with_message("not a Hash object"))?;
    let algorithm = obj.get(js_string!("_algorithm"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let data_hex = obj.get(js_string!("_data"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();

    // Decode accumulated hex data
    let data = hex_decode(&data_hex);

    let hash_bytes: Vec<u8> = match algorithm.as_str() {
        "sha256" | "SHA256" | "sha-256" => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        "sha512" | "SHA512" | "sha-512" => {
            let mut hasher = sha2::Sha512::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        "sha384" | "SHA384" | "sha-384" => {
            let mut hasher = sha2::Sha384::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        "sha224" | "SHA224" | "sha-224" => {
            let mut hasher = sha2::Sha224::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        "sha1" | "SHA1" | "sha-1" => {
            let mut hasher = sha1::Sha1::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        "md5" | "MD5" => {
            let mut hasher = md5::Md5::new();
            hasher.update(&data);
            hasher.finalize().to_vec()
        }
        _ => {
            return Err(JsNativeError::typ()
                .with_message(format!("unsupported hash algorithm: {algorithm}"))
                .into());
        }
    };

    let encoding = args.first()
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "hex".to_string());

    // If no encoding specified, return a Buffer
    if args.is_empty() || encoding == "buffer" {
        // Return hex-encoded for now; ideally would return a Buffer object
        return Ok(js_string!(&*hex_encode(&hash_bytes)).into());
    }

    Ok(js_string!(&*encode_digest(&hash_bytes, &encoding)).into())
}

fn hex_decode(hex: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut i = 0;
    let hex_bytes = hex.as_bytes();
    while i + 1 < hex_bytes.len() {
        if let Ok(b) = u8::from_str_radix(&hex[i..i + 2], 16) {
            bytes.push(b);
        }
        i += 2;
    }
    bytes
}

/// Build an HMAC object with .update() and .digest() methods.
fn make_hmac_object(algorithm: &str, key: &[u8], context: &mut Context) -> boa_engine::JsResult<JsObject> {
    let obj = JsObject::with_object_proto(context.intrinsics());

    obj.set(js_string!("_algorithm"), js_string!(algorithm), false, context)?;
    obj.set(js_string!("_key"), js_string!(&*hex_encode(key)), false, context)?;
    obj.set(js_string!("_data"), js_string!(""), false, context)?;

    set_fn(&obj, "update", hmac_update, context)?;
    set_fn(&obj, "digest", hmac_digest, context)?;

    Ok(obj)
}

fn hmac_update(this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    // Same as hash_update — accumulate data
    hash_update(this, args, context)
}

fn hmac_digest(this: &JsValue, args: &[JsValue], context: &mut Context) -> boa_engine::JsResult<JsValue> {
    use hmac::{Hmac, Mac};

    let obj = this.as_object()
        .ok_or_else(|| JsNativeError::typ().with_message("not an Hmac object"))?;
    let algorithm = obj.get(js_string!("_algorithm"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let key_hex = obj.get(js_string!("_key"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let data_hex = obj.get(js_string!("_data"), context)?
        .as_string().map(|s| s.to_std_string_escaped()).unwrap_or_default();

    let key = hex_decode(&key_hex);
    let data = hex_decode(&data_hex);

    let hmac_bytes: Vec<u8> = match algorithm.as_str() {
        "sha256" | "SHA256" | "sha-256" => {
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&key)
                .map_err(|e| JsNativeError::typ().with_message(format!("invalid key: {e}")))?;
            mac.update(&data);
            mac.finalize().into_bytes().to_vec()
        }
        "sha512" | "SHA512" | "sha-512" => {
            let mut mac = Hmac::<sha2::Sha512>::new_from_slice(&key)
                .map_err(|e| JsNativeError::typ().with_message(format!("invalid key: {e}")))?;
            mac.update(&data);
            mac.finalize().into_bytes().to_vec()
        }
        "sha1" | "SHA1" | "sha-1" => {
            let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&key)
                .map_err(|e| JsNativeError::typ().with_message(format!("invalid key: {e}")))?;
            mac.update(&data);
            mac.finalize().into_bytes().to_vec()
        }
        "md5" | "MD5" => {
            let mut mac = Hmac::<md5::Md5>::new_from_slice(&key)
                .map_err(|e| JsNativeError::typ().with_message(format!("invalid key: {e}")))?;
            mac.update(&data);
            mac.finalize().into_bytes().to_vec()
        }
        _ => {
            return Err(JsNativeError::typ()
                .with_message(format!("unsupported hmac algorithm: {algorithm}"))
                .into());
        }
    };

    let encoding = args.first()
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| "hex".to_string());

    Ok(js_string!(&*encode_digest(&hmac_bytes, &encoding)).into())
}

// --- Top-level functions ---

fn crypto_create_hash(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let algorithm = arg_string(args, 0, ctx)?;
    let obj = make_hash_object(&algorithm, ctx)?;
    Ok(obj.into())
}

fn crypto_create_hmac(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let algorithm = arg_string(args, 0, ctx)?;
    let key = arg_bytes(args, 1, ctx)?;
    let obj = make_hmac_object(&algorithm, &key, ctx)?;
    Ok(obj.into())
}

fn crypto_random_bytes(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let size = args.first()
        .and_then(|v| v.as_number())
        .ok_or_else(|| JsNativeError::typ().with_message("size must be a number"))? as usize;

    let mut bytes = vec![0u8; size];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| JsNativeError::typ().with_message(format!("randomBytes failed: {e}")))?;

    // Return as a Buffer if Buffer is available, otherwise as a hex string
    let global = ctx.global_object();
    let buffer_ctor = global.get(js_string!("Buffer"), ctx)?;
    if let Some(ctor) = buffer_ctor.as_object().filter(|o| o.is_constructor()) {
        // Create array of byte values
        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for &b in &bytes {
            arr.push(JsValue::from(b as i32), ctx)?;
        }
        let buf = ctor.construct(&[arr.into()], None, ctx)?;
        Ok(buf.into())
    } else {
        // Fallback: return hex string
        Ok(js_string!(&*hex_encode(&bytes)).into())
    }
}

fn crypto_random_uuid(_: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| JsNativeError::typ().with_message(format!("randomUUID failed: {e}")))?;

    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    );
    Ok(js_string!(&*uuid).into())
}

fn crypto_get_hashes(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> boa_engine::JsResult<JsValue> {
    let arr = boa_engine::object::builtins::JsArray::new(ctx);
    for name in ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"] {
        arr.push(js_string!(name), ctx)?;
    }
    Ok(arr.into())
}
