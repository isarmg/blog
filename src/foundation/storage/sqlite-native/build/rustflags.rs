//! SQLite-compatible Linux AMD64 code generation options derived from cc-rs.

#[derive(Default)]
pub(super) struct Flags<'a> {
    code_model: Option<&'a str>,
    no_vectorize_loops: bool,
    no_vectorize_slp: bool,
    profile_generate: Option<&'a str>,
    profile_use: Option<&'a str>,
    pub linker_plugin_lto: bool,
    relocation_model: Option<&'a str>,
    embed_bitcode: Option<bool>,
    frame_pointers: bool,
    no_redzone: Option<bool>,
    dwarf_version: Option<u32>,
    stack_protector: Option<&'a str>,
    target_features: Vec<&'a str>,
}

impl<'a> Flags<'a> {
    pub(super) fn parse(encoded: &'a str) -> Self {
        let mut flags = Self::default();
        let mut args = encoded.split('\u{1f}');
        while let Some(arg) = args.next() {
            let option = match arg {
                "--codegen" | "-C" => args.next(),
                "-Z" => args
                    .next()
                    .filter(|arg| arg.starts_with("stack-protector=")),
                "-A" | "--allow" | "-D" | "--deny" | "-F" | "--forbid" | "-L" | "-l" | "-o"
                | "-W" | "--warn" => {
                    args.next();
                    None
                }
                _ => arg
                    .strip_prefix("--codegen=")
                    .or_else(|| arg.strip_prefix("-C"))
                    .or_else(|| {
                        arg.strip_prefix("-Z")
                            .filter(|arg| arg.starts_with("stack-protector="))
                    }),
            };
            let Some(option) = option else { continue };
            let (name, value) = option.split_once('=').unwrap_or((option, ""));
            let on = || matches!(value, "" | "y" | "yes" | "on" | "true");
            match name {
                "code-model" => flags.code_model = Some(value),
                "no-vectorize-loops" => flags.no_vectorize_loops = true,
                "no-vectorize-slp" => flags.no_vectorize_slp = true,
                "profile-generate" => flags.profile_generate = Some(value),
                "profile-use" => flags.profile_use = Some(value),
                "linker-plugin-lto" => flags.linker_plugin_lto = value != "no",
                "relocation-model" => flags.relocation_model = Some(value),
                "embed-bitcode" => flags.embed_bitcode = Some(on()),
                "force-frame-pointers" => flags.frame_pointers = on(),
                "no-redzone" => flags.no_redzone = Some(on()),
                "dwarf-version" => flags.dwarf_version = value.parse().ok(),
                "stack-protector" => flags.stack_protector = Some(value),
                "target-feature" => flags.target_features.extend(value.split(',')),
                _ => {}
            }
        }
        flags
    }

    pub(super) fn uses_clang_options(&self) -> bool {
        self.profile_generate.is_some()
            || self.profile_use.is_some()
            || self.linker_plugin_lto
            || self.embed_bitcode.is_some()
            || self.no_vectorize_loops
            || self.no_vectorize_slp
    }

    pub(super) fn c_flags(self, clang: bool) -> Vec<String> {
        let mut flags = Vec::new();
        if let Some(value) = self.code_model {
            flags.push(format!("-mcmodel={value}"));
        }
        if self.no_vectorize_loops {
            flags.push(
                if clang {
                    "-fno-vectorize"
                } else {
                    "-fno-tree-loop-vectorize"
                }
                .into(),
            );
        }
        if self.no_vectorize_slp {
            flags.push(
                if clang {
                    "-fno-slp-vectorize"
                } else {
                    "-fno-tree-slp-vectorize"
                }
                .into(),
            );
        }
        match self.relocation_model {
            Some("pic") => flags.push("-fPIC".into()),
            Some("pie") => flags.push("-fPIE".into()),
            _ => {}
        }
        if self.frame_pointers {
            flags.extend([
                "-fno-omit-frame-pointer".into(),
                "-mno-omit-leaf-frame-pointer".into(),
            ]);
        }
        if let Some(value) = self.no_redzone {
            flags.push(if value { "-mno-red-zone" } else { "-mred-zone" }.into());
        }
        if let Some(value) = self.dwarf_version {
            flags.push(format!("-gdwarf-{value}"));
        }
        match self.stack_protector {
            Some("strong") => flags.push("-fstack-protector-strong".into()),
            Some("all") => flags.push("-fstack-protector-all".into()),
            _ => {}
        }
        flags.extend(self.target_features.into_iter().filter_map(target_feature));
        if clang {
            if let Some(value) = self.profile_generate {
                flags.push(if value.is_empty() {
                    "-fprofile-generate".into()
                } else {
                    format!("-fprofile-generate={value}")
                });
            }
            if let Some(value) = self.profile_use {
                flags.push(format!("-fprofile-use={value}"));
            }
            if let Some(value) = self.embed_bitcode {
                flags.push(
                    if value {
                        "-fembed-bitcode=all"
                    } else {
                        "-fembed-bitcode=off"
                    }
                    .into(),
                );
            }
            if self.linker_plugin_lto {
                flags.push("-flto=thin".into());
            }
        }
        flags
    }
}

fn target_feature(feature: &str) -> Option<String> {
    let (prefix, name) = if let Some(feature) = feature.strip_prefix('+') {
        ("-m", feature)
    } else {
        ("-mno-", feature.strip_prefix('-')?)
    };
    // Keep the baseline SSE/SSE2 ABI even when a Rust target feature is disabled.
    let driver_name = match name {
        "adx" | "aes" | "avx" | "avx2" | "avx512bf16" | "avx512bitalg" | "avx512bw"
        | "avx512cd" | "avx512dq" | "avx512f" | "avx512fp16" | "avx512ifma" | "avx512vbmi"
        | "avx512vbmi2" | "avx512vl" | "avx512vnni" | "avx512vpopcntdq" | "avxvnni" | "bmi2"
        | "f16c" | "fma" | "fxsr" | "gfni" | "lzcnt" | "movbe" | "popcnt" | "rdseed" | "sha"
        | "sse3" | "sse4.1" | "sse4.2" | "sse4a" | "ssse3" | "vaes" | "vpclmulqdq" | "xsave"
        | "xsavec" | "xsaveopt" | "xsaves" => name,
        "bmi1" => "bmi",
        "cmpxchg16b" => "cx16",
        "pclmulqdq" => "pclmul",
        "rdrand" => "rdrnd",
        _ => return None,
    };
    Some(format!("{prefix}{driver_name}"))
}
