//! Compile the repository's SQLite source for the supported native product.
use std::env;

#[path = "build/compiler.rs"]
mod compiler;

fn main() {
    for platform in ["HOST", "TARGET"] {
        assert_eq!(
            env::var(platform).as_deref(),
            Ok(compiler::PLATFORM),
            "SQLite requires Linux amd64 GNU for {platform}"
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/compiler.rs");
    println!("cargo:rerun-if-changed=build/rustflags.rs");
    println!("cargo:rerun-if-changed=sqlite3/sqlite3.c");
    println!("cargo:rerun-if-env-changed=LIBSQLITE3_SYS_USE_PKG_CONFIG");
    assert!(
        !env::var_os("LIBSQLITE3_SYS_USE_PKG_CONFIG").is_some_and(|value| value != "0"),
        "the product uses its maintained SQLite source; external SQLite linking is unsupported"
    );

    for name in [
        "SQLITE3_LIB_DIR",
        "SQLITE3_INCLUDE_DIR",
        "SQLITE3_STATIC",
        "SQLCIPHER_LIB_DIR",
        "SQLCIPHER_INCLUDE_DIR",
        "SQLCIPHER_STATIC",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        assert!(
            env::var_os(name).is_none(),
            "{name} selects an external SQLite build; this product always builds its maintained source"
        );
    }

    // Preserve the existing product's compiled SQLite behavior.
    let mut definitions = [
        "SQLITE_CORE",
        "SQLITE_DEFAULT_FOREIGN_KEYS=1",
        "SQLITE_ENABLE_API_ARMOR",
        "SQLITE_ENABLE_COLUMN_METADATA",
        "SQLITE_ENABLE_DBSTAT_VTAB",
        "SQLITE_ENABLE_FTS3",
        "SQLITE_ENABLE_FTS3_PARENTHESIS",
        "SQLITE_ENABLE_FTS5",
        "SQLITE_ENABLE_JSON1",
        "SQLITE_ENABLE_LOAD_EXTENSION=1",
        "SQLITE_ENABLE_MEMORY_MANAGEMENT",
        "SQLITE_ENABLE_RTREE",
        "SQLITE_ENABLE_STAT4",
        "SQLITE_SOUNDEX",
        "SQLITE_THREADSAFE=1",
        "SQLITE_USE_URI",
        "HAVE_USLEEP=1",
        "HAVE_ISNAN",
        "_POSIX_THREAD_SAFE_FUNCTIONS",
        "HAVE_LOCALTIME_R",
    ]
    .into_iter()
    .map(|option| format!("-D{option}"))
    .collect::<Vec<_>>();
    for name in [
        "SQLITE_MAX_VARIABLE_NUMBER",
        "SQLITE_MAX_EXPR_DEPTH",
        "SQLITE_MAX_COLUMN",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        if let Ok(limit) = env::var(name) {
            let limit: u32 = limit
                .parse()
                .unwrap_or_else(|_| panic!("{name} must be an unsigned integer"));
            definitions.push(format!("-D{name}={limit}"));
        }
    }
    println!("cargo:rerun-if-env-changed=LIBSQLITE3_FLAGS");
    if let Ok(flags) = env::var("LIBSQLITE3_FLAGS") {
        for flag in flags.split_whitespace() {
            if flag.starts_with("-D") || flag.starts_with("-U") {
                definitions.push(flag.to_owned());
            } else if flag.starts_with("SQLITE_") {
                definitions.push(format!("-D{flag}"));
            } else {
                panic!("unsupported SQLite preprocessor flag: {flag}");
            }
        }
    }
    if let Err(error) = compiler::build_sqlite(&definitions) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
