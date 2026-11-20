use anyhow::Error;
use std::fmt::Write;
use walrus::Module;

// Function to ensure we always append a valid typescript parameter name based
// on parameter index
fn push_index_identifier(i: usize, s: &mut String) {
    let letter = b'a' + ((i % 26) as u8);
    s.push(letter as char);
    if i >= 26 {
        write!(s, "{}", i / 26).unwrap();
    }
}

fn args_are_optional(name: &str) -> bool {
    name == "__wbindgen_thread_destroy"
}

pub fn interface(module: &Module) -> Result<String, Error> {
    let mut exports = String::new();
    module_export_types(module, |name, ty| {
        if name.contains(':') {
            // This can happen when `name` is namespaced, like `__wbgt__reference_test::foo`.
            // We should quote the name, as : is not valid in TypeScript identifiers.
            writeln!(exports, "  readonly {name:?}: {ty};").unwrap();
        } else {
            writeln!(exports, "  readonly {name}: {ty};").unwrap();
        }
    });
    Ok(exports)
}

pub fn typescript(module: &Module) -> Result<String, Error> {
    let mut exports = "/* tslint:disable */\n/* eslint-disable */\n".to_string();
    module_export_types(module, |name, ty| {
        writeln!(exports, "export const {name}: {ty};").unwrap();
    });
    Ok(exports)
}

/// Iterates over all the exports in a module and generates TypeScript types. All
/// name-type pairs are passed to the `export` function.
fn module_export_types(module: &Module, mut export: impl FnMut(&str, &str)) {
    for entry in module.exports.iter() {
        match entry.item {
            walrus::ExportItem::Function(id) => {
                let func = module.funcs.get(id);
                let ty = module.types.get(func.ty());
                let ts_type = function_type_to_ts(ty, args_are_optional(&entry.name));
                export(&entry.name, &ts_type);
            }
            walrus::ExportItem::Memory(_) => export(&entry.name, "WebAssembly.Memory"),
            walrus::ExportItem::Table(_) => export(&entry.name, "WebAssembly.Table"),
            walrus::ExportItem::Global(_) => continue,
            walrus::ExportItem::Tag(_) => export(&entry.name, "WebAssembly.Tag"),
        };
    }
}
fn val_type_to_ts(ty: walrus::ValType) -> &'static str {
    // see https://webassembly.github.io/spec/js-api/index.html#towebassemblyvalue
    // and https://webassembly.github.io/spec/js-api/index.html#tojsvalue
    match ty {
        walrus::ValType::I32 | walrus::ValType::F32 | walrus::ValType::F64 => "number",
        walrus::ValType::I64 => "bigint",
        // there could be anything behind a reference
        walrus::ValType::Ref(_) => "any",
        // V128 currently isn't supported in JS and therefore doesn't have a
        // specific type in the spec. When it does get support, this type will
        // still be technically correct, but should be updated to something more
        // specific.
        walrus::ValType::V128 => "any",
    }
}
fn function_type_to_ts(function: &walrus::Type, all_args_optional: bool) -> String {
    let mut out = String::new();

    // parameters
    out.push('(');
    for (i, arg_type) in function.params().iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }

        push_index_identifier(i, &mut out);
        if all_args_optional {
            out.push('?');
        }
        out.push_str(": ");
        out.push_str(val_type_to_ts(*arg_type));
    }
    out.push(')');

    // arrow
    out.push_str(" => ");

    // results
    let results = function.results();
    // this match follows the spec:
    // https://webassembly.github.io/spec/js-api/index.html#exported-function-exotic-objects
    match results.len() {
        0 => out.push_str("void"),
        1 => out.push_str(val_type_to_ts(results[0])),
        _ => {
            out.push('[');
            for (i, result) in results.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(val_type_to_ts(*result));
            }
            out.push(']');
        }
    }

    out
}

#[path = "../../../../../test/foundation/wasm/browser-module/unit/src/wasm_typescript/tests.rs"]
#[cfg(test)]
mod tests;
