use boa_engine::{
    Context, JsString, JsValue, Module,
    js_string,
    module::SyntheticModuleInitializer,
};

const EXPORT_NAMES: &[&str] = &["default", "StringDecoder"];

pub fn create_module(context: &mut Context) -> Module {
    ensure_string_decoder_global(context);

    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_module),
        None, None, context,
    )
}

fn ensure_string_decoder_global(context: &mut Context) {
    let global = context.global_object();
    let existing = global.get(js_string!("StringDecoder"), context).unwrap_or(JsValue::undefined());
    if !existing.is_undefined() {
        return;
    }

    let _ = context.eval(boa_engine::Source::from_bytes(STRING_DECODER_JS));
}

fn init_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    let global = context.global_object();
    let ctor = global.get(js_string!("StringDecoder"), context)?;

    module.set_export(&js_string!("default"), ctor.clone())?;
    module.set_export(&js_string!("StringDecoder"), ctor)?;

    Ok(())
}

const STRING_DECODER_JS: &str = r#"
(function() {
    function StringDecoder(encoding) {
        this.encoding = (encoding || 'utf8').toLowerCase();
        this.lastNeed = 0;
        this.lastTotal = 0;
        this.lastChar = '';
    }

    StringDecoder.prototype.write = function(buf) {
        if (typeof buf === 'string') return buf;
        if (!buf || buf.length === 0) return '';
        return String(buf);
    };

    StringDecoder.prototype.end = function(buf) {
        var r = '';
        if (buf && buf.length > 0) r = this.write(buf);
        if (this.lastNeed) return r + '\ufffd';
        return r;
    };

    StringDecoder.prototype.text = StringDecoder.prototype.write;

    globalThis.StringDecoder = StringDecoder;
    return StringDecoder;
})()
"#;
