//! BOO-140 purity guard: the kernel stays a pure compatibility consumer.
//!
//! The authenticated finance API boundary belongs to a separate backend
//! (`book-finance-api`, not created yet). This test fails if the kernel gains a
//! dependency or a source path that would let it open a network connection, a
//! database, an auth/credential provider, the process environment, or the
//! filesystem at runtime. The policy lives in
//! `fixtures/gates/finance-api-boundary.json` (`purity_policy`).

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn gate_fixture() -> Value {
    let text = fs::read_to_string(Path::new(ROOT).join("fixtures/gates/finance-api-boundary.json"))
        .expect("gate fixture is readable");
    serde_json::from_str(&text).expect("gate fixture is valid JSON")
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("policy entry is an array")
        .iter()
        .map(|item| item.as_str().expect("policy entry is a string").to_owned())
        .collect()
}

// --- Cargo manifest and lockfile ---------------------------------------------------------------

/// Every dependency declared in any dependency table of a Cargo manifest,
/// grouped by table kind (`dependencies`, `dev-dependencies`,
/// `build-dependencies`), including `[target.*.dependencies]`,
/// `[workspace.dependencies]` and `[dependencies.<name>]` table forms.
fn manifest_dependencies(manifest: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut section = String::new();
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_owned();
            // `[dependencies.foo]` declares `foo` directly.
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                for prefix in [format!("{kind}."), format!("workspace.{kind}.")] {
                    if let Some(name) = section.strip_prefix(&prefix) {
                        found.push((kind.to_owned(), name.trim_matches('"').to_owned()));
                    }
                }
                if let Some(index) = section.find(&format!(".{kind}.")) {
                    if section.starts_with("target.") {
                        let name = &section[index + kind.len() + 2..];
                        found.push((kind.to_owned(), name.trim_matches('"').to_owned()));
                    }
                }
            }
            continue;
        }
        let kind = ["dev-dependencies", "build-dependencies", "dependencies"]
            .into_iter()
            .find(|kind| {
                section == *kind
                    || section == format!("workspace.{kind}")
                    || (section.starts_with("target.") && section.ends_with(&format!(".{kind}")))
            });
        if let (Some(kind), Some((key, _))) = (kind, line.split_once('=')) {
            found.push((kind.to_owned(), key.trim().trim_matches('"').to_owned()));
        }
    }
    found
}

fn manifest_package_keys(manifest: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut in_package = false;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some((key, _)) = line.split_once('=') {
                keys.insert(key.trim().to_owned());
            }
        }
    }
    keys
}

#[test]
fn cargo_manifest_dependencies_match_the_allowlist() {
    let policy = &gate_fixture()["purity_policy"];
    let manifest = fs::read_to_string(Path::new(ROOT).join("Cargo.toml")).expect("Cargo.toml");
    let found = manifest_dependencies(&manifest);
    for (kind, key) in [
        ("dependencies", "dependencies"),
        ("dev-dependencies", "dev_dependencies"),
        ("build-dependencies", "build_dependencies"),
    ] {
        let declared: BTreeSet<String> = found
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, name)| name.clone())
            .collect();
        assert_eq!(
            declared,
            string_set(&policy[key]),
            "[{kind}] drifted from purity_policy.{key}; a new kernel dependency needs an owner decision (BOO-140)"
        );
    }

    let package_keys = manifest_package_keys(&manifest);
    for forbidden in ["build", "links"] {
        assert!(
            !package_keys.contains(forbidden),
            "[package].{forbidden} would run or link native code at build time"
        );
    }
    assert!(
        !Path::new(ROOT).join("build.rs").exists(),
        "build.rs runs arbitrary code with filesystem/network access at build time"
    );
}

#[test]
fn cargo_lockfile_resolves_only_allowlisted_registry_crates() {
    let policy = &gate_fixture()["purity_policy"];
    let lock = fs::read_to_string(Path::new(ROOT).join("Cargo.lock")).expect("Cargo.lock");
    let mut names = BTreeSet::new();
    for line in lock.lines() {
        if let Some(name) = line.strip_prefix("name = ") {
            names.insert(name.trim_matches('"').to_owned());
        }
        if let Some(source) = line.strip_prefix("source = ") {
            assert_eq!(
                source.trim_matches('"'),
                "registry+https://github.com/rust-lang/crates.io-index",
                "only crates.io registry sources are allowed in the kernel lockfile"
            );
        }
    }
    names.remove("book-finance-core");
    assert_eq!(
        names,
        string_set(&policy["lockfile_packages"]),
        "Cargo.lock resolved a crate outside purity_policy.lockfile_packages"
    );
}

// --- Rust source scan --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    PathSep,
    Punct(char),
}

/// Tokenize Rust source into identifiers and punctuation, dropping comments,
/// string/char literals and numbers, so a mention in docs or in a string never
/// counts and a real path never hides inside one.
fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if (c == 'r' || c == 'b')
            && matches!(next, Some('"') | Some('#') | Some('r'))
            && raw_string_end(&chars, i).is_some()
        {
            i = raw_string_end(&chars, i).expect("checked above");
        } else if c == '"' || (c == 'b' && next == Some('"')) {
            i += if c == 'b' { 2 } else { 1 };
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
        } else if c == '\'' || (c == 'b' && next == Some('\'')) {
            let start = if c == 'b' { i + 1 } else { i };
            if chars.get(start + 1) == Some(&'\\') {
                i = start + 3;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if chars.get(start + 2) == Some(&'\'') {
                i = start + 3;
            } else {
                i = start + 1; // a lifetime; its name is tokenized as an identifier
            }
        } else if c == ':' && next == Some(':') {
            tokens.push(Token::PathSep);
            i += 2;
        } else if c.is_alphabetic() || c == '_' {
            if c == 'r' && next == Some('#') {
                i += 2; // raw identifier `r#name`
            }
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident(ident));
        } else if c.is_ascii_digit() {
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
        } else {
            if !c.is_whitespace() {
                tokens.push(Token::Punct(c));
            }
            i += 1;
        }
    }
    tokens
}

/// End index of a raw (byte) string literal starting at `start`, if any.
fn raw_string_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    if chars.get(i) == Some(&'b') {
        i += 1;
    }
    if chars.get(i) != Some(&'r') {
        return None;
    }
    i += 1;
    let mut hashes = 0;
    while chars.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    if chars.get(i) != Some(&'"') {
        return None;
    }
    i += 1;
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(chars.len())
}

fn ident(token: Option<&Token>) -> Option<&str> {
    match token {
        Some(Token::Ident(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// What one source file reaches through `std::`, `core::` or `alloc::`.
#[derive(Debug, Default)]
struct StdUses {
    /// Full paths below the root, use-trees expanded: `use std::{fmt, io::{self, Read}}`
    /// gives `fmt`, `io`, `io::Read`; `std::process::exit(1)` gives `process::exit`.
    paths: Vec<String>,
    /// `std::*` or a glob anywhere inside a std use-tree.
    glob: bool,
    /// `as` anywhere inside a std use-tree (`std::{self as s}`, `std::env as e`)
    /// or directly after the root (`use std as s`).
    alias: bool,
}

/// Parse one use-tree (or expression path) at `i`, below `prefix`.
/// Always advances at least one token.
fn std_tree(tokens: &[Token], mut i: usize, prefix: &[String], out: &mut StdUses) -> usize {
    match tokens.get(i) {
        Some(Token::Punct('*')) => {
            out.glob = true;
            i + 1
        }
        Some(Token::Punct('{')) => {
            i += 1;
            while let Some(token) = tokens.get(i) {
                match token {
                    Token::Punct('}') => return i + 1,
                    Token::Punct(',') => i += 1,
                    _ => i = std_tree(tokens, i, prefix, out),
                }
            }
            i
        }
        Some(Token::Ident(name)) => {
            let mut path = prefix.to_vec();
            if name != "self" {
                path.push(name.clone());
            }
            i += 1;
            if ident(tokens.get(i)) == Some("as") {
                out.alias = true;
                out.paths.push(path.join("::"));
                return i + 2;
            }
            if tokens.get(i) == Some(&Token::PathSep)
                && matches!(
                    tokens.get(i + 1),
                    Some(Token::Ident(_) | Token::Punct('{') | Token::Punct('*'))
                )
            {
                return std_tree(tokens, i + 1, &path, out);
            }
            out.paths.push(path.join("::"));
            i
        }
        _ => i + 1,
    }
}

fn std_uses(tokens: &[Token]) -> StdUses {
    let mut out = StdUses::default();
    for i in 0..tokens.len() {
        if !matches!(ident(tokens.get(i)), Some("std" | "core" | "alloc")) {
            continue;
        }
        if tokens.get(i + 1) == Some(&Token::PathSep) {
            std_tree(tokens, i + 2, &[], &mut out);
        } else if ident(tokens.get(i + 1)) == Some("as") {
            out.alias = true;
        }
    }
    out
}

/// What a file may reach. The library may reach no runtime I/O module at all;
/// a binary adapter may reach only the listed std items (`io::stdin`,
/// `process::ExitCode`, ...) and read at most one declared env constant.
enum Scope<'a> {
    Library,
    Binary {
        allowed_items: &'a BTreeSet<String>,
        env_var: Option<&'a str>,
    },
}

/// Purity violations in one source file.
fn violations(source: &str, policy: &Value, scope: &Scope) -> Vec<String> {
    let tokens = tokenize(source);
    let forbidden_std = string_set(&policy["forbidden_std_modules"]);
    let forbidden_crates = string_set(&policy["forbidden_crates"]);
    let forbidden_idents = string_set(&policy["forbidden_identifiers"]);
    let env_var_allowance = match scope {
        Scope::Library => None,
        Scope::Binary { env_var, .. } => *env_var,
    };
    let mut found = Vec::new();

    let uses = std_uses(&tokens);
    if uses.glob {
        found.push("std glob import".to_owned());
    }
    if uses.alias {
        found.push("std alias (`as`)".to_owned());
    }
    for path in &uses.paths {
        let root = path.split("::").next().unwrap_or("");
        let allowed = match scope {
            Scope::Library => !forbidden_std.contains(root),
            Scope::Binary { allowed_items, .. } => {
                allowed_items.contains(path) || !forbidden_std.contains(root)
            }
        };
        if !allowed {
            found.push(format!("std::{path}"));
        }
    }
    for (i, token) in tokens.iter().enumerate() {
        let Token::Ident(name) = token else { continue };
        if forbidden_crates.contains(name) {
            found.push(format!("crate {name}"));
        }
        if forbidden_idents.contains(name) {
            found.push(format!("identifier {name}"));
        }
        let prev = i.checked_sub(1).and_then(|p| tokens.get(p));
        let next = tokens.get(i + 1);
        if matches!(name.as_str(), "env" | "option_env") && next == Some(&Token::Punct('!')) {
            found.push(format!("macro {name}!"));
        }
        if name == "extern" {
            found.push("extern (crate alias or FFI)".to_owned());
        }
        // A binary reaches an imported module as `io::stdin()`; that item must
        // be allowlisted as well.
        if let Scope::Binary { allowed_items, .. } = scope {
            if forbidden_std.contains(name)
                && prev != Some(&Token::PathSep)
                && next == Some(&Token::PathSep)
            {
                if let Some(item) = ident(tokens.get(i + 2)) {
                    let path = format!("{name}::{item}");
                    if !allowed_items.contains(&path) {
                        found.push(format!("std::{path}"));
                    }
                }
            }
        }
        if name == "env" && next == Some(&Token::PathSep) {
            let call = ident(tokens.get(i + 2));
            let arg_ok = env_var_allowance.is_some()
                && call == Some("var")
                && tokens.get(i + 3) == Some(&Token::Punct('('))
                && ident(tokens.get(i + 4)) == env_var_allowance
                && tokens.get(i + 5) == Some(&Token::Punct(')'));
            if !arg_ok {
                found.push(format!("env::{}", call.unwrap_or("?")));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source dir is readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn kernel_sources() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    rust_files(&Path::new(ROOT).join("src"), &mut files);
    files.sort();
    let bin_dir = Path::new(ROOT).join("src/bin");
    files
        .into_iter()
        .partition(|path| !path.starts_with(&bin_dir))
}

#[test]
fn library_source_has_no_runtime_io_paths() {
    let policy = gate_fixture()["purity_policy"].clone();
    let (library, _) = kernel_sources();
    assert!(!library.is_empty(), "library sources found");
    for path in library {
        let source = fs::read_to_string(&path).expect("library source");
        let found = violations(&source, &policy, &Scope::Library);
        assert!(
            found.is_empty(),
            "{} breaks kernel purity: {found:?}",
            path.strip_prefix(ROOT).unwrap_or(&path).display()
        );
    }
}

#[test]
fn binaries_only_use_allowlisted_stdio_and_exit_items_and_the_declared_flag() {
    let policy = gate_fixture()["purity_policy"].clone();
    let adapters = policy["binary_adapters"]
        .as_object()
        .expect("binary_adapters object");
    let (_, binaries) = kernel_sources();
    let found_names: BTreeSet<String> = binaries
        .iter()
        .map(|path| {
            path.strip_prefix(ROOT)
                .expect("in repo")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let declared: BTreeSet<String> = adapters.keys().cloned().collect();
    assert_eq!(
        found_names, declared,
        "every binary adapter must be declared in purity_policy.binary_adapters"
    );
    for path in binaries {
        let relative = path
            .strip_prefix(ROOT)
            .expect("in repo")
            .to_string_lossy()
            .into_owned();
        let adapter = &adapters[&relative];
        let mut allowed = string_set(&policy["binary_std_items"]);
        allowed.extend(string_set(&adapter["extra_std_items"]));
        let scope = Scope::Binary {
            allowed_items: &allowed,
            env_var: adapter["env_var"].as_str(),
        };
        let source = fs::read_to_string(&path).expect("binary source");
        let found = violations(&source, &policy, &scope);
        assert!(
            found.is_empty(),
            "{relative} breaks kernel purity: {found:?}"
        );
    }
}

// The guard's own regression cases: each mutation must be caught, each
// harmless mention must not be.
#[test]
fn scanner_catches_impure_mutations_and_ignores_comments_and_strings() {
    let policy = gate_fixture()["purity_policy"].clone();
    let library = Scope::Library;
    for impure in [
        "use std::net::TcpStream;",
        "use std::{collections::HashMap, fs};",
        "use std::{fmt, io::{self, Read}};",
        "fn f() { let _ = std::env::var(\"X\"); }",
        "use std::env; fn f() { let _ = env::vars(); }",
        "fn f() { let _ = std::process::Command::new(\"sh\"); }",
        "fn f() { let _ = ::std::fs::read(\"x\"); }",
        "async fn f() { tokio::spawn(async {}); }",
        "fn f() { let _ = reqwest::get(\"http://x\"); }",
        "fn f() -> sqlx::PgPool { todo!() }",
        "const T: &str = env!(\"HOME\");",
        "extern crate std as s;",
        "use std as s;",
        "fn f() { let _ = std::time::SystemTime::now(); }",
        "fn f() { let _ = jsonwebtoken::decode::<()>; }",
        // Review bypasses (BOO-140 PR #1): glob and aliased std imports.
        "use std::*; pub(crate) fn leak() -> usize { fs::read(\"/etc/passwd\").map(|b| b.len()).unwrap_or(0) }",
        "use std::{self as s}; pub(crate) fn leak() -> usize { s::fs::read(\"/etc/passwd\").map(|b| b.len()).unwrap_or(0) }",
        "use core::*;",
        "use alloc::{vec as v};",
        "use std::{collections::HashMap as Map};",
        "use r#std::fs;",
    ] {
        assert!(
            !violations(impure, &policy, &library).is_empty(),
            "library guard missed: {impure}"
        );
    }
    for pure in [
        "// uses std::net only in a comment\nfn f() {}",
        "/* std::fs::read */ fn f() {}",
        "const DOC: &str = \"std::env::var tokio reqwest\";",
        "const RAW: &str = r#\"use std::fs;\"#;",
        "fn f<'a>(x: &'a str) -> &'a str { x }",
        "use std::{collections::HashSet, fmt, str::FromStr};",
        "struct T { net: i64 }",
        "use serde::de::Error as DeserializeError;",
    ] {
        assert_eq!(
            violations(pure, &policy, &library),
            Vec::<String>::new(),
            "false positive: {pure}"
        );
    }

    let mut pilot_items = string_set(&policy["binary_std_items"]);
    pilot_items.extend(["env", "env::var"].map(String::from));
    let pilot = Scope::Binary {
        allowed_items: &pilot_items,
        env_var: Some("FLAG"),
    };
    let shadow_items = string_set(&policy["binary_std_items"]);
    let shadow = Scope::Binary {
        allowed_items: &shadow_items,
        env_var: None,
    };
    for (scope, impure) in [
        (&pilot, "use std::env; fn f() { env::var(OTHER_FLAG); }"),
        (&shadow, "use std::env; fn f() { env::var(FLAG); }"),
        // Review bypasses (BOO-140 PR #1): subprocess egress and an aliased env module.
        (
            &shadow,
            "fn main() { let _ = std::process::Command::new(\"curl\").arg(\"https://example.com\").status(); }",
        ),
        (
            &pilot,
            "use std::env as e; fn main() { let _ = e::vars(); }",
        ),
        (&shadow, "use std::process::{Stdio, Child};"),
        (&shadow, "use std::process; fn main() { process::abort(); }"),
        (&shadow, "use std::io; fn main() { let _ = io::copy; }"),
        (&shadow, "use std::io::{self as i};"),
        (
            &shadow,
            "use std::fs; fn main() { let _ = fs::read(\"x\"); }",
        ),
        (&shadow, "use std::io::*;"),
    ] {
        assert!(
            !violations(impure, &policy, scope).is_empty(),
            "binary guard missed: {impure}"
        );
    }
    for (scope, pure) in [
        (&pilot, "use std::env; fn f() { env::var(FLAG); }"),
        (
            &shadow,
            "use std::io::{self, Read, Write}; use std::process::ExitCode; fn main() -> ExitCode { let _ = io::stdin(); let _ = io::stdout(); ExitCode::SUCCESS }",
        ),
        (&shadow, "fn main() { std::process::exit(1) }"),
    ] {
        assert_eq!(
            violations(pure, &policy, scope),
            Vec::<String>::new(),
            "binary false positive: {pure}"
        );
    }

    assert!(!manifest_dependencies("[dependencies]\ntokio = \"1\"\n").is_empty());
    assert_eq!(
        manifest_dependencies("[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"),
        vec![("dependencies".to_owned(), "libc".to_owned())]
    );
    assert_eq!(
        manifest_dependencies("[dev-dependencies.reqwest]\nversion = \"0.12\"\n"),
        vec![("dev-dependencies".to_owned(), "reqwest".to_owned())]
    );
}
