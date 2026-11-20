//! Build the SQLite amalgamation with the Linux AMD64 GNU C toolchain.
//! Tool selection, environment precedence, and archiver fallback derive from cc-rs.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "rustflags.rs"]
mod rustflags;

pub const PLATFORM: &str = "x86_64-unknown-linux-gnu";

#[derive(Debug)]
pub enum Error {
    Disabled,
    InvalidEnvironment(String),
    Io {
        operation: String,
        source: io::Error,
    },
    Command {
        program: OsString,
        output: Output,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("CC_FORCE_DISABLE prevents building bundled SQLite"),
            Self::InvalidEnvironment(name) => {
                write!(f, "invalid build environment variable {name}")
            }
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::Command { program, output } => write!(
                f,
                "{program:?} failed with {}\n{}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn build_sqlite(definitions: &[String]) -> Result<(), Error> {
    if enabled(tracked("CC_FORCE_DISABLE").as_deref()) {
        return Err(Error::Disabled);
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| invalid("OUT_DIR"))?);
    let object = out.join("sqlite3.o");
    let archive = out.join("libsqlite3.a");
    let pending = out.join("libsqlite3-pending.a");
    let encoded = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let rustflags = rustflags::Flags::parse(&encoded);
    let default_cc = rustflags
        .linker_plugin_lto
        .then(default_clang)
        .flatten()
        .unwrap_or_else(|| OsString::from("cc"));
    let mut cc = tool("CC", default_cc)?;
    let ar = tool("AR", "ar".into())?;
    let shell_escaped = enabled(tracked("CC_SHELL_ESCAPED_FLAGS").as_deref());
    let cflags = flags("CFLAGS", shell_escaped)?;
    let arflags = flags("ARFLAGS", shell_escaped)?;
    let verbose = enabled(tracked("CC_ENABLE_DEBUG_OUTPUT").as_deref());
    let defaults = !enabled(tracked("CRATE_CC_NO_DEFAULTS").as_deref());
    let opt = env::var("OPT_LEVEL").map_err(|_| invalid("OPT_LEVEL"))?;
    let debug = env::var("DEBUG").unwrap_or_default();
    if defaults && !matches!(opt.as_str(), "0" | "1" | "2" | "3" | "s" | "z") {
        return Err(invalid("OPT_LEVEL"));
    }

    configure_wrapper(&mut cc);
    // Detect the driver only when a clang-specific setting can affect the build.
    let clang = ((defaults && (opt == "z" || debug == "line-directives-only"))
        || rustflags.uses_clang_options())
        && execute(cc.command().arg("--version"), verbose).is_ok_and(|output| {
            output.status.success()
                && output
                    .stdout
                    .windows(13)
                    .any(|bytes| bytes == b"clang version")
        });
    let mut compile = cc.command();
    compile.env("LC_ALL", "C");
    let has_debug = !matches!(debug.as_str(), "" | "0" | "false" | "none");
    if defaults {
        let optimize = if opt == "z" && !clang { "s" } else { &opt };
        // An explicit tool can include an optimization argument before the default flags.
        if !cc.args.iter().any(|arg| arg.as_bytes().starts_with(b"-O")) {
            compile.arg(format!("-O{optimize}"));
        }
        compile.args(["-ffunction-sections", "-fdata-sections", "-fPIC", "-m64"]);
        if has_debug {
            let level = match debug.as_str() {
                "line-directives-only" if clang => "-gline-directives-only",
                "1" | "limited" | "line-tables-only" | "line-directives-only" => "-g1",
                _ => "-g",
            };
            compile.args([level, "-gdwarf-4", "-fno-omit-frame-pointer"]);
        }
    }

    let mut inherited = rustflags.c_flags(clang);
    if defaults
        && has_debug
        && !inherited
            .iter()
            .any(|flag| flag == "-mno-omit-leaf-frame-pointer")
    {
        inherited.push("-mno-omit-leaf-frame-pointer".to_owned());
    }
    for flag in inherited {
        // Feed an empty translation unit. Some drivers report unsupported flags as warnings.
        let supported = execute(
            cc.command().env("LC_ALL", "C").arg(&flag).args([
                "-m64",
                "-x",
                "c",
                "-c",
                "/dev/null",
                "-o",
                "/dev/null",
            ]),
            verbose,
        )?;
        if supported.status.success() && supported.stderr.is_empty() {
            compile.arg(flag);
        } else {
            println!("cargo:warning=SQLite C compiler ignored unsupported inherited flag {flag:?}");
        }
    }

    let compiled = execute(
        compile
            .arg("-w")
            .args(definitions)
            .args(cflags)
            .args(["-c", "sqlite3/sqlite3.c", "-o"])
            .arg(&object),
        verbose,
    )?;
    check(&compile, compiled)?;

    // Publish only a completed archive. The fixed Cargo OUT_DIR belongs to this build unit.
    remove_pending(&pending)?;
    let mut archiver = ar.command();
    archiver
        .args(&arflags)
        .arg("crsD")
        .arg(&pending)
        .arg(&object);
    let archived = execute(&mut archiver, verbose)?;
    if !archived.status.success() {
        remove_pending(&pending)?;
        let mut fallback = ar.command();
        fallback
            .env("ZERO_AR_DATE", "1")
            .args(arflags)
            .arg("crs")
            .arg(&pending)
            .arg(&object);
        let archived = execute(&mut fallback, verbose)?;
        check(&fallback, archived)?;
    }
    fs::rename(&pending, &archive).map_err(|source| Error::Io {
        operation: format!("publish {}", archive.display()),
        source,
    })?;
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=sqlite3");
    Ok(())
}

#[derive(Debug)]
struct Tool {
    program: OsString,
    args: Vec<OsString>,
}

impl Tool {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }
}

fn tool(name: &str, default: OsString) -> Result<Tool, Error> {
    let configured = variants(name)
        .into_iter()
        .map(|key| (tracked(&key), key))
        .fold(None, |first, (value, key)| {
            first.or_else(|| value.map(|value| (value, key)))
        });
    if let Some((value, key)) = configured {
        let value = value.as_bytes().trim_ascii();
        if !value.is_empty() {
            if Path::new(OsStr::from_bytes(value)).is_file() {
                return Ok(Tool {
                    program: OsStr::from_bytes(value).to_owned(),
                    args: Vec::new(),
                });
            }
            let mut args = split_flags(value, true, &key)?.into_iter();
            let program = args
                .next()
                .filter(|program| !program.is_empty())
                .ok_or_else(|| invalid(key))?;
            return Ok(Tool {
                program,
                args: args.collect(),
            });
        }
    }
    Ok(Tool {
        program: default,
        args: Vec::new(),
    })
}

fn default_clang() -> Option<OsString> {
    let path = tracked("PATH")?;
    env::split_paths(&path)
        .map(|path| path.join("clang"))
        .find(|path| path.is_file())
        .map(PathBuf::into_os_string)
}

fn configure_wrapper(cc: &mut Tool) {
    const CACHE_WRAPPERS: &[&str] = &["sccache", "cachepot", "buildcache", "kache"];
    let custom = tracked("CC_KNOWN_WRAPPER_CUSTOM");
    let wrapper = tracked("RUSTC_WRAPPER");
    let program_name = Path::new(&cc.program).file_stem().and_then(OsStr::to_str);
    if program_name.is_some_and(|name| {
        CACHE_WRAPPERS.contains(&name)
            || matches!(name, "ccache" | "distcc" | "icecc")
            || custom.as_deref() == Some(OsStr::new(name))
    }) {
        return;
    }
    if let Some(wrapper) = wrapper.filter(|path| {
        Path::new(path)
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|name| CACHE_WRAPPERS.contains(&name))
    }) {
        cc.args
            .insert(0, std::mem::replace(&mut cc.program, wrapper));
    }
}

fn variants(name: &str) -> [String; 4] {
    [
        format!("{name}_{PLATFORM}"),
        format!("{name}_x86_64_unknown_linux_gnu"),
        format!("HOST_{name}"),
        name.to_owned(),
    ]
}

fn tracked(name: &str) -> Option<OsString> {
    println!("cargo:rerun-if-env-changed={name}");
    env::var_os(name)
}

fn enabled(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !matches!(value.as_bytes(), b"" | b"0" | b"no" | b"false"))
}

fn flags(name: &str, shell_escaped: bool) -> Result<Vec<OsString>, Error> {
    let mut flags = Vec::new();
    for key in variants(name).into_iter().rev() {
        if let Some(value) = tracked(&key) {
            flags.extend(split_flags(value.as_bytes(), shell_escaped, &key)?);
        }
    }
    Ok(flags)
}

fn split_flags(value: &[u8], shell_escaped: bool, key: &str) -> Result<Vec<OsString>, Error> {
    if shell_escaped {
        let args = shlex::bytes::split(value).ok_or_else(|| invalid(key))?;
        return Ok(args.into_iter().map(OsString::from_vec).collect());
    }
    Ok(value
        .split(u8::is_ascii_whitespace)
        .filter(|arg| !arg.is_empty())
        .map(|arg| OsStr::from_bytes(arg).to_owned())
        .collect())
}

fn invalid(name: impl Into<String>) -> Error {
    Error::InvalidEnvironment(name.into())
}

fn execute(command: &mut Command, verbose: bool) -> Result<Output, Error> {
    if verbose {
        println!("cargo:warning=running {command:?}");
    }
    command.output().map_err(|source| Error::Io {
        operation: format!("start {:?}", command.get_program()),
        source,
    })
}

fn check(command: &Command, output: Output) -> Result<(), Error> {
    if !output.status.success() {
        return Err(Error::Command {
            program: command.get_program().to_owned(),
            output,
        });
    }
    Ok(())
}

fn remove_pending(pending: &Path) -> Result<(), Error> {
    match fs::remove_file(pending) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            operation: format!("replace {}", pending.display()),
            source,
        }),
    }
}
