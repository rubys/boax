use boa_engine::{
    Context, JsString, Module,
    js_string,
    module::SyntheticModuleInitializer,
};

const EXPORT_NAMES: &[&str] = &["default", "StringDecoder"];

pub fn create_module(context: &mut Context) -> Module {
    let export_names: Vec<JsString> = EXPORT_NAMES.iter().map(|n| js_string!(*n)).collect();
    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure(init_module),
        None, None, context,
    )
}

fn init_module(module: &boa_engine::module::SyntheticModule, context: &mut Context) -> boa_engine::JsResult<()> {
    // StringDecoder is a simple class that decodes Buffer chunks to strings,
    // handling multi-byte characters that may be split across chunks.
    // For our purposes (no real Buffer yet), a minimal implementation suffices.
    let src = r#"
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

    return StringDecoder;
})()
"#;

    let ctor = context.eval(boa_engine::Source::from_bytes(src))
        .map_err(|e| {
            boa_engine::JsNativeError::syntax()
                .with_message(format!("failed to create StringDecoder: {e}"))
        })?;

    module.set_export(&js_string!("default"), ctor.clone())?;
    module.set_export(&js_string!("StringDecoder"), ctor)?;

    Ok(())
}
