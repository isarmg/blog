use super::Context;
use crate::transforms::threads::PAGE_SIZE;
use anyhow::Error;
use std::fmt::Write;

impl Context<'_> {
    pub(super) fn ts_for_init_fn(
        &self,
        has_memory: bool,
        has_module_or_path_optional: bool,
    ) -> Result<String, Error> {
        let output = crate::wasm_typescript::interface(self.module)?;

        let memory_param = if has_memory {
            ", memory?: WebAssembly.Memory"
        } else {
            ""
        };
        let stack_size = if self.threads_enabled {
            ", thread_stack_size?: number"
        } else {
            ""
        };
        let arg_optional = if has_module_or_path_optional { "?" } else { "" };
        let init_sync_fn = format!(
            "\
            /**\n\
            * Instantiates the given `module`, which can either be bytes or\n\
            * a precompiled `WebAssembly.Module`.\n\
            *\n\
            * @param {{{{ module: SyncInitInput{memory_param}{stack_size} }}}} options\n\
            *\n\
            * @returns {{InitOutput}}\n\
            */\n\
            export function initSync(options: {{ module: SyncInitInput{memory_param}{stack_size} }}): InitOutput;\n\
            "
        );

        let sync_init_function = format!(
            "export type SyncInitInput = BufferSource | WebAssembly.Module;\n\n{init_sync_fn}\n"
        );
        Ok(format!(
            "\n\
            export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;\n\
            \n\
            export interface InitOutput {{\n\
            {output}}}\n\
            \n\
            {sync_init_function}\
            /**\n\
            * If `module_or_path` is {{RequestInfo}} or {{URL}}, makes a request and\n\
            * for everything else, calls `WebAssembly.instantiate` directly.\n\
            *\n\
            * @param {{{{ module_or_path{arg_optional}: InitInput | Promise<InitInput>{memory_param}{stack_size} }}}} options\n\
            *\n\
            * @returns {{Promise<InitOutput>}}\n\
            */\n\
            export default function __wbg_init \
                (options{arg_optional}: {{ module_or_path{arg_optional}: InitInput | Promise<InitInput>{memory_param}{stack_size} }}): Promise<InitOutput>;\n",
        ))
    }

    fn generate_web_loading(
        &self,
        needs_manual_start: bool,
        default_module_path: &str,
        has_memory: bool,
    ) -> String {
        let mut init_memviews = String::new();
        for &(num, ref views) in self.memories.values() {
            for kind in views {
                writeln!(
                    init_memviews,
                    // Reset the memory views to null in case `init` gets called multiple times.
                    // Without this, the `length = 0` check would never detect that the view was
                    // outdated.
                    "cached{kind}Memory{num} = null;",
                )
                .unwrap()
            }
        }
        format!(
            "let wasmModule, wasmInstance, wasm;
            function __wbg_finalize_init(instance, module{init_stack_size_arg}) {{
                wasmInstance = instance;
                wasm = instance.exports;
                wasmModule = module;
                {init_memviews}{init_stack_size_check}{start}return wasm;
            }}

            async function __wbg_load(module, imports) {{
                if (typeof Response === 'function' && module instanceof Response) {{
                    if (!module.ok) {{
                        throw new Error(`failed to fetch Wasm: ${{module.status}} ${{module.statusText}} fetching '${{module.url}}'`);
                    }}

                    if (typeof WebAssembly.instantiateStreaming === 'function') {{
                        try {{
                            return await WebAssembly.instantiateStreaming(module, imports);
                        }} catch (e) {{
                            const validResponse = expectedResponseType(module.type);

                            if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {{
                                console.warn(\"`WebAssembly.instantiateStreaming` failed \
                                                because your server does not serve Wasm with \
                                                `application/wasm` MIME type. Falling back to \
                                                `WebAssembly.instantiate` which is slower. Original \
                                                error:\\n\", e);

                            }} else {{ throw e; }}
                        }}
                    }}

                    const bytes = await module.arrayBuffer();
                    return await WebAssembly.instantiate(bytes, imports);
                }} else {{
                    const instance = await WebAssembly.instantiate(module, imports);

                    if (instance instanceof WebAssembly.Instance) {{
                        return {{ instance, module }};
                    }} else {{
                        return instance;
                    }}
                }}

                function expectedResponseType(type) {{
                    switch (type) {{
                        case 'basic': case 'cors': case 'default': return true;
                    }}
                    return false;
                }}
            }}

            function initSync(options) {{
                if (options == null || Object.getPrototypeOf(options) !== Object.prototype || arguments.length > 1) {{
                    throw new TypeError('initSync requires a single options object');
                }}
                if (wasm !== undefined) return wasm;

                let {{module{init_memory_arg}{init_stack_size_arg}}} = options;

                const imports = __wbg_get_imports({init_memory_arg_alone});
                if (!(module instanceof WebAssembly.Module)) {{
                    module = new WebAssembly.Module(module);
                }}
                const instance = new WebAssembly.Instance(module, imports);
                return __wbg_finalize_init(instance, module{init_stack_size_arg});
            }}

            async function __wbg_init(options = {{}}) {{
                if (options === null || Object.getPrototypeOf(options) !== Object.prototype || arguments.length > 1) {{
                    throw new TypeError('init requires a single options object');
                }}
                if (wasm !== undefined) return wasm;

                let {{module_or_path{init_memory_arg}{init_stack_size_arg}}} = options;

                {default_module_path}
                const imports = __wbg_get_imports({init_memory_arg_alone});

                if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {{
                    module_or_path = fetch(module_or_path);
                }}

                const {{ instance, module }} = await __wbg_load(await module_or_path, imports);

                return __wbg_finalize_init(instance, module{init_stack_size_arg});
            }}
            ",
            start = if needs_manual_start && self.threads_enabled {
                "wasm.__wbindgen_start(thread_stack_size);\n"
            } else if needs_manual_start {
                "wasm.__wbindgen_start();\n"
            } else {
                ""
            },
            init_stack_size_arg = if self.threads_enabled {
                ", thread_stack_size"
            } else {
                ""
            },
            init_stack_size_check = if self.threads_enabled {
                format!(
                    "if (typeof thread_stack_size !== 'undefined' && (typeof thread_stack_size !== 'number' || thread_stack_size === 0 || thread_stack_size % {} !== 0)) {{
                        throw new Error('invalid stack size');
                    }}\n\n",
                    PAGE_SIZE,
                )
            } else {
                String::new()
            },
            init_memory_arg = if has_memory {
                ", memory"
            } else {
                ""
            },
            init_memory_arg_alone = if has_memory {
                "memory"
            } else {
                ""
            },
        )
    }

    pub(super) fn generate_wasm_loading(
        &mut self,
        module_name: &str,
        needs_manual_start: bool,
        has_memory: bool,
    ) -> String {
        let default_module_path = if self.config.omit_default_module_path {
            ""
        } else {
            &format!(
                "if (module_or_path === undefined) {{
                        module_or_path = new URL('{module_name}_bg.wasm', import.meta.url);
                    }}"
            )
        };
        let mut loading =
            self.generate_web_loading(needs_manual_start, default_module_path, has_memory);

        loading.push_str("\nexport { initSync, __wbg_init as default };");

        loading
    }
}
