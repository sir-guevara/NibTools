use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

#[cfg(feature = "gui")]
mod gui;

const APP_NAME: &str = "nibpass";
const VAULT_DIR: &str = ".nibpass";
const VAULT_FILE: &str = "vault";
const VAULT_MAGIC: &str = "NIBPASS-VAULT-1";
const ENTRY_MAGIC: &str = "NIBPASS-ENTRY-1";
const RECOVERY_MAGIC: &str = "NIBPASS-RECOVERY-1";
const DEVICE_KEY_LEN: usize = 32;
const CLIPBOARD_CLEAR_SECONDS: u64 = 30;
const SESSION_AGENT_TTL_SECONDS: u64 = 12 * 60 * 60;
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_PASSES: u32 = 3;
const ARGON2_LANES: u32 = 1;

type Result<T> = std::result::Result<T, String>;

fn main() {
    if let Err(err) = run() {
        eprintln!("nibpass: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }

    let mut copy = false;
    let mut no_agent = false;
    args.retain(|arg| {
        if arg == "-c" || arg == "--clip" {
            copy = true;
            false
        } else if arg == "--no-agent" {
            no_agent = true;
            false
        } else {
            true
        }
    });

    let store = Store::new()?;
    match args[0].as_str() {
        "init" => cmd_init(&store, &args[1..]),
        "gen" | "generate" => cmd_gen(&args[1..]),
        "unlock" => cmd_agent(&store, &args[1..]),
        "lock" => agent_lock(&store),
        "status" => agent_status(&store),
        "agent" => cmd_agent(&store, &args[1..]),
        "clear-clipboard" => cmd_clear_clipboard(&args[1..]),
        "add" => cmd_add(&store, &args[1..], no_agent),
        "set" => cmd_set(&store, &args[1..], no_agent),
        "edit" => cmd_edit(&store, &args[1..], no_agent),
        "show" => cmd_show(&store, &args[1..], copy, no_agent),
        "find" | "search" => cmd_find(&store, &args[1..]),
        "audit" => cmd_audit(&store, no_agent),
        "ls" | "list" => cmd_list(&store),
        "rm" | "remove" => cmd_remove(&store, &args[1..]),
        "2fa" | "otp" | "totp" => cmd_2fa(&store, &args[1..], copy, no_agent),
        "git" => cmd_git(&store, &args[1..]),
        "sync" => cmd_sync(&store, &args[1..]),
        "rotate" => cmd_rotate(&store, &args[1..], no_agent),
        "completion" | "completions" => cmd_completion(&args[1..]),
        "recovery" => cmd_recovery(&store, &args[1..]),
        "backup" => cmd_backup(&store, &args[1..]),
        "import" => cmd_import(&store, &args[1..], no_agent),
        "export" => cmd_export(&store, &args[1..], no_agent),
        "browser" => cmd_browser(&store, &args[1..], no_agent),
        "gui" => cmd_gui(&store),
        "install-shell" => cmd_install_shell(&args[1..]),
        "shellenv" => cmd_shellenv(&args[1..]),
        name if copy => cmd_show(&store, &[name.to_string()], true, no_agent),
        account => cmd_account(&store, account, &args[1..], copy, no_agent),
    }
}

fn print_help() {
    println!(
        "NibPass\n\n\
Usage:\n\
  nibpass init\n\
  nibpass gen [--length N|--words N]\n\
  nibpass add [<name>] [--dialog] [--username USER] [--url URL] [--2fa SECRET] [--notes TEXT] [--password-stdin]\n\
  nibpass set <name> <field> [value]\n\
  nibpass edit <name>\n\
  nibpass show <name> [--field FIELD]\n\
  nibpass find <query>\n\
  nibpass audit\n\
  nibpass -c <name>\n\
  nibpass 2fa [-c] <name>\n\
  nibpass <name> add 2fa [SECRET|OTPAUTH_URL]\n\
  nibpass ls\n\
  nibpass rm <name>\n\
  nibpass agent [--session|--ttl SECONDS]\n\
  nibpass agent status\n\
  nibpass agent lock\n\
  nibpass recovery export <file>\n\
  nibpass recovery import <file>\n\
  nibpass recovery verify <file>\n\
  nibpass recovery status\n\
  nibpass sync [init <repo>|status]\n\
  nibpass rotate <master|device>\n\
  nibpass completion <zsh|bash|fish>\n\
  nibpass git <status|log|commit|undo|...>\n\
  nibpass browser <host|manifest|install> [chrome|firefox]\n\
  nibpass gui\n\
  nibpass import pass <path>\n\
  nibpass import csv <path> [--format bitwarden|chrome|apple|firefox|1password|xpass|generic]\n\
  nibpass import <bitwarden|chrome|apple|firefox|1password|xpass> <path>\n\n\
  nibpass export csv <path> --plaintext\n\n\
Setup:\n\
  nibpass install-shell [--shell zsh|bash|fish] [--bin-dir DIR]\n\
  nibpass shellenv [--shell zsh|bash|fish] [--bin-dir DIR]\n\n\
Environment:\n\
  NIBPASS_STORE  Override the store directory\n\
  --no-agent     Always ask for master password for this command\n"
    );
}

struct Store {
    root: PathBuf,
}

impl Store {
    fn new() -> Result<Self> {
        let root = match env::var_os("NIBPASS_STORE") {
            Some(path) => PathBuf::from(path),
            None => data_dir()?.join(APP_NAME),
        };
        Ok(Self { root })
    }

    fn entry_path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.root.join(self.entry_relative_path(name)?))
    }

    fn entry_relative_path(&self, name: &str) -> Result<PathBuf> {
        Ok(clean_entry_name(name)?.with_extension("nib"))
    }

    fn vault_path(&self) -> PathBuf {
        self.root.join(VAULT_DIR).join(VAULT_FILE)
    }

    fn device_key_path(&self) -> Result<PathBuf> {
        if let Some(path) = env::var_os("NIBPASS_DEVICE_KEY") {
            return Ok(PathBuf::from(path));
        }
        let root_hash = hex_encode(&sha1(self.root.to_string_lossy().as_bytes()));
        Ok(config_dir()?
            .join(APP_NAME)
            .join(format!("device-{root_hash}.key")))
    }

    fn ensure_exists(&self) -> Result<()> {
        if self.root.is_dir() && self.vault_path().is_file() {
            Ok(())
        } else {
            Err(format!(
                "vault does not exist at {}. run `nibpass init` first",
                self.root.display()
            ))
        }
    }

    fn is_git_repo(&self) -> bool {
        self.root.join(".git").is_dir()
    }
}

fn data_dir() -> Result<PathBuf> {
    if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is not set".to_string())
    } else if let Some(path) = env::var_os("XDG_DATA_HOME") {
        Ok(PathBuf::from(path))
    } else {
        let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        Ok(PathBuf::from(home).join(".local/share"))
    }
}

fn config_dir() -> Result<PathBuf> {
    if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is not set".to_string())
    } else if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(path))
    } else {
        let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        Ok(PathBuf::from(home).join(".config"))
    }
}

fn clean_entry_name(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if name.trim().is_empty() || path.is_absolute() {
        return Err("entry name must be a relative path".to_string());
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            _ => return Err("entry name cannot contain '.', '..', or path prefixes".to_string()),
        }
    }
    Ok(clean)
}

fn cmd_init(store: &Store, args: &[String]) -> Result<()> {
    if !args.is_empty() && args.iter().any(|arg| arg != "--git") {
        return Err("usage: nibpass init".to_string());
    }
    fs::create_dir_all(&store.root).map_err(|err| err.to_string())?;
    fs::create_dir_all(store.root.join(VAULT_DIR)).map_err(|err| err.to_string())?;
    if !store.vault_path().is_file() {
        let mut password = read_secret("Create master password: ")?;
        if password.len() < 12 {
            password.zeroize();
            return Err("master password must be at least 12 characters".to_string());
        }
        let mut confirm = read_secret("Confirm master password: ")?;
        if password != confirm {
            password.zeroize();
            confirm.zeroize();
            return Err("master passwords do not match".to_string());
        }

        let vault_key = random_array::<32>();
        let device_key = ensure_device_key(store)?;
        write_vault_file(store, &vault_key, password.as_bytes(), &device_key)?;
        password.zeroize();
        confirm.zeroize();
        eprintln!("backup the recovery key with: nibpass recovery export <safe-offline-file>");
    }
    ensure_git_repo(store);
    auto_commit_paths(
        store,
        &[PathBuf::from(VAULT_DIR).join(VAULT_FILE)],
        "initialize vault",
    );
    println!("initialized {}", store.root.display());
    Ok(())
}

fn cmd_add(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    let vault_key = unlock_vault(store, no_agent)?;
    let (provided_name, mut fields) = parse_name_and_flags(args)?;
    let dialog = fields.remove("dialog").is_some() || provided_name.is_none();
    let name = match provided_name {
        Some(name) => name,
        None => read_required("Entry name: ")?,
    };

    let password = if fields.remove("generate").is_some() {
        generate_password(24)
    } else if fields.remove("password-stdin").is_some() {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| err.to_string())?;
        input.trim_end_matches(['\r', '\n']).to_string()
    } else {
        read_secret("Password: ")?
    };

    if password.is_empty() {
        return Err("password cannot be empty".to_string());
    }

    let mut entry = BTreeMap::new();
    entry.insert("password".to_string(), password);
    if let Some(secret) = fields.remove("2fa") {
        fields.insert("otp".to_string(), secret);
    }

    for key in ["username", "url", "notes"] {
        let value = if dialog {
            match fields.remove(key) {
                Some(value) => value,
                None => read_optional(&format!("{}: ", field_label(key)))?,
            }
        } else {
            fields.remove(key).unwrap_or_default()
        };
        if !value.is_empty() {
            entry.insert(key.to_string(), value);
        }
    }
    if let Some(value) = fields.remove("otp") {
        if !value.is_empty() {
            entry.insert("otp".to_string(), value);
        }
    }
    if !fields.is_empty() {
        let keys = fields.keys().cloned().collect::<Vec<_>>().join(", ");
        return Err(format!("unknown option(s): {keys}"));
    }

    let relative_path = store.entry_relative_path(&name)?;
    let out_path = store.root.join(&relative_path);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    write_entry_to_path_with_key(&vault_key, &entry, &out_path)?;
    auto_commit_paths(store, &[relative_path], &format!("add {name}"));
    println!("saved {}", name);
    Ok(())
}

fn cmd_gen(args: &[String]) -> Result<()> {
    let mut length = 24usize;
    let mut words = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--length" | "-l" => {
                length = args
                    .get(i + 1)
                    .ok_or_else(|| "--length requires a value".to_string())?
                    .parse::<usize>()
                    .map_err(|_| "--length must be a number".to_string())?;
                i += 2;
            }
            "--words" | "-w" => {
                words = Some(
                    args.get(i + 1)
                        .ok_or_else(|| "--words requires a value".to_string())?
                        .parse::<usize>()
                        .map_err(|_| "--words must be a number".to_string())?,
                );
                i += 2;
            }
            other => return Err(format!("unknown gen option '{other}'")),
        }
    }
    if let Some(count) = words {
        println!("{}", generate_words(count.max(3)));
    } else {
        println!("{}", generate_password(length.max(12)));
    }
    Ok(())
}

fn cmd_set(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    if args.len() < 2 {
        return Err("usage: nibpass set <name> <field> [value]".to_string());
    }
    let name = &args[0];
    let field = field_key(&args[1]).to_string();
    if !matches!(
        field.as_str(),
        "password" | "username" | "url" | "otp" | "notes"
    ) {
        return Err("field must be password, username, url, 2fa, or notes".to_string());
    }
    let vault_key = unlock_vault(store, no_agent)?;
    let mut entry = read_entry_with_key(store, name, &vault_key)?;
    let value = if args.len() > 2 {
        args[2..].join(" ")
    } else if field == "password" {
        read_secret("Password: ")?
    } else if field == "otp" {
        normalize_2fa_secret(&read_required("2FA secret or otpauth URL: ")?)?
    } else {
        read_required(&format!("{}: ", field_label(&field)))?
    };
    if field == "otp" {
        base32_decode(&value)?;
    }
    entry.insert(field, value);
    write_entry_with_key(store, name, &entry, &vault_key)?;
    auto_commit_paths(
        store,
        &[store.entry_relative_path(name)?],
        &format!("set {} for {name}", args[1]),
    );
    println!("updated {name}");
    Ok(())
}

fn cmd_edit(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    let name = args
        .first()
        .ok_or_else(|| "usage: nibpass edit <name>".to_string())?;
    let vault_key = unlock_vault(store, no_agent)?;
    let mut entry = read_entry_with_key(store, name, &vault_key)?;
    for key in ["username", "url", "notes"] {
        let current = entry.get(key).cloned().unwrap_or_default();
        let value = read_optional(&format!("{} [{}]: ", field_label(key), current))?;
        if !value.is_empty() {
            entry.insert(key.to_string(), value);
        }
    }
    if yes_no("Change password? [y/N]: ")? {
        entry.insert("password".to_string(), read_secret("Password: ")?);
    }
    write_entry_with_key(store, name, &entry, &vault_key)?;
    auto_commit_paths(
        store,
        &[store.entry_relative_path(name)?],
        &format!("edit {name}"),
    );
    println!("updated {name}");
    Ok(())
}

fn cmd_show(store: &Store, args: &[String], copy: bool, no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    let vault_key = unlock_vault(store, no_agent)?;
    let name = args
        .first()
        .ok_or_else(|| "usage: nibpass show <name>".to_string())?;
    let fields = parse_flags(&args[1..])?;
    let entry = read_entry_with_key(store, name, &vault_key)?;
    let requested_field = fields
        .get("field")
        .map(String::as_str)
        .unwrap_or("password");
    let field = field_key(requested_field);

    if copy {
        let value = entry
            .get(field)
            .ok_or_else(|| format!("entry has no field '{requested_field}'"))?;
        copy_to_clipboard(value)?;
        println!("copied {requested_field} for {name}");
    } else if fields.contains_key("field") {
        let value = entry
            .get(field)
            .ok_or_else(|| format!("entry has no field '{requested_field}'"))?;
        println!("{value}");
    } else {
        for (key, value) in entry {
            if key == "password" || key == "otp" {
                println!("{}: [hidden]", field_label(&key));
            } else {
                println!("{key}: {value}");
            }
        }
    }
    Ok(())
}

fn cmd_find(store: &Store, args: &[String]) -> Result<()> {
    store.ensure_exists()?;
    let query = args
        .first()
        .ok_or_else(|| "usage: nibpass find <query>".to_string())?
        .to_ascii_lowercase();
    let mut entries = Vec::new();
    collect_entries(&store.root, &store.root, &mut entries)?;
    for entry in entries {
        if entry.to_ascii_lowercase().contains(&query) {
            println!("{entry}");
        }
    }
    Ok(())
}

fn cmd_audit(store: &Store, no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    let vault_key = unlock_vault(store, no_agent)?;
    let mut names = Vec::new();
    collect_entries(&store.root, &store.root, &mut names)?;
    let mut password_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut missing_2fa = Vec::new();
    let mut weak = Vec::new();

    for name in &names {
        let entry = read_entry_with_key(store, name, &vault_key)?;
        if !entry.contains_key("otp") {
            missing_2fa.push(name.clone());
        }
        if let Some(password) = entry.get("password") {
            if is_weak_password(password) {
                weak.push(name.clone());
            }
            password_owners
                .entry(password.clone())
                .or_default()
                .push(name.clone());
        }
    }

    println!("accounts: {}", names.len());
    println!("weak passwords: {}", weak.len());
    for name in &weak {
        println!("  weak: {name}");
    }
    let duplicates = password_owners
        .values()
        .filter(|owners| owners.len() > 1)
        .count();
    println!("duplicate passwords: {duplicates}");
    for owners in password_owners.values().filter(|owners| owners.len() > 1) {
        println!("  duplicate: {}", owners.join(", "));
    }
    println!("missing 2fa: {}", missing_2fa.len());
    for name in missing_2fa {
        println!("  no 2fa: {name}");
    }
    cmd_recovery_status(store)?;
    Ok(())
}

fn cmd_list(store: &Store) -> Result<()> {
    store.ensure_exists()?;
    let mut entries = Vec::new();
    collect_entries(&store.root, &store.root, &mut entries)?;
    for entry in entries {
        println!("{entry}");
    }
    Ok(())
}

fn cmd_remove(store: &Store, args: &[String]) -> Result<()> {
    store.ensure_exists()?;
    let name = args
        .first()
        .ok_or_else(|| "usage: nibpass rm <name>".to_string())?;
    let relative_path = store.entry_relative_path(name)?;
    let path = store.root.join(&relative_path);
    fs::remove_file(&path).map_err(|err| err.to_string())?;
    auto_commit_paths(store, &[relative_path], &format!("remove {name}"));
    println!("removed {name}");
    Ok(())
}

fn cmd_2fa(store: &Store, args: &[String], copy: bool, no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    let vault_key = unlock_vault(store, no_agent)?;
    let name = args
        .first()
        .ok_or_else(|| "usage: nibpass 2fa <name>".to_string())?;
    let entry = read_entry_with_key(store, name, &vault_key)?;
    let secret = entry
        .get("otp")
        .ok_or_else(|| format!("{name} has no 2fa field"))?;
    let code = totp(secret, 6, 30)?;
    if copy {
        copy_to_clipboard(&code)?;
        println!("copied 2fa for {name}");
    } else {
        println!("{code}");
    }
    Ok(())
}

fn cmd_account(
    store: &Store,
    account: &str,
    args: &[String],
    copy: bool,
    no_agent: bool,
) -> Result<()> {
    store.ensure_exists()?;
    match args {
        [] if copy => cmd_show(store, &[account.to_string()], true, no_agent),
        [action, kind] if action == "add" && is_2fa_alias(kind) => {
            cmd_account_add_2fa(store, account, None, no_agent)
        }
        [action, kind, secret] if action == "add" && is_2fa_alias(kind) => {
            cmd_account_add_2fa(store, account, Some(secret.as_str()), no_agent)
        }
        [action, kind, flag, secret]
            if action == "add" && is_2fa_alias(kind) && flag == "--secret" =>
        {
            cmd_account_add_2fa(store, account, Some(secret.as_str()), no_agent)
        }
        [action, field] if action == "set" => {
            cmd_set(store, &[account.to_string(), field.to_string()], no_agent)
        }
        [action, field, value @ ..] if action == "set" => {
            let mut set_args = vec![account.to_string(), field.to_string()];
            set_args.extend(value.iter().cloned());
            cmd_set(store, &set_args, no_agent)
        }
        [action] if action == "edit" => cmd_edit(store, &[account.to_string()], no_agent),
        _ => Err(format!(
            "unknown account command for '{account}'\n\ntry `nibpass {account} add 2fa`"
        )),
    }
}

fn cmd_account_add_2fa(
    store: &Store,
    account: &str,
    provided_secret: Option<&str>,
    no_agent: bool,
) -> Result<()> {
    let vault_key = unlock_vault(store, no_agent)?;
    let mut entry = read_entry_with_key(store, account, &vault_key)?;
    let raw_secret = match provided_secret {
        Some(secret) => secret.to_string(),
        None => read_required("2FA secret or otpauth URL: ")?,
    };
    let secret = normalize_2fa_secret(&raw_secret)?;
    base32_decode(&secret)?;

    entry.insert("otp".to_string(), secret);
    write_entry_with_key(store, account, &entry, &vault_key)?;
    auto_commit_paths(
        store,
        &[store.entry_relative_path(account)?],
        &format!("add 2fa for {account}"),
    );
    println!("saved 2fa for {account}");
    Ok(())
}

fn cmd_git(store: &Store, args: &[String]) -> Result<()> {
    store.ensure_exists()?;
    if args.is_empty() {
        return run_in(&store.root, "git", ["status"]);
    }
    if args[0] == "undo" {
        return run_in(&store.root, "git", ["revert", "--no-edit", "HEAD"]);
    }
    run_passthrough(&store.root, "git", args)
}

fn cmd_sync(store: &Store, args: &[String]) -> Result<()> {
    store.ensure_exists()?;
    if !store.is_git_repo() {
        return Err("git repository is not initialized. run `nibpass init`".to_string());
    }
    match args.first().map(String::as_str) {
        Some("init") => {
            let repo = args
                .get(1)
                .ok_or_else(|| "usage: nibpass sync init <repo>".to_string())?;
            run_in(
                &store.root,
                "git",
                ["remote", "add", "origin", repo.as_str()],
            )?;
            return cmd_sync(store, &[]);
        }
        Some("status") => {
            cmd_recovery_status(store)?;
            return run_in(&store.root, "git", ["status", "--short"]);
        }
        Some(other) => return Err(format!("unknown sync command '{other}'")),
        None => {}
    }
    if !git_has_remote(store, "origin") {
        return Err(
            "no git remote named origin. add one with `nibpass git remote add origin <repo>`"
                .to_string(),
        );
    }
    run_in(&store.root, "git", ["pull", "--rebase"])?;
    run_in(&store.root, "git", ["push"])?;
    Ok(())
}

fn cmd_recovery(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("export") => {
            let path = args
                .get(1)
                .ok_or_else(|| "usage: nibpass recovery export <file>".to_string())?;
            cmd_recovery_export(store, Path::new(path))
        }
        Some("import") => {
            let path = args
                .get(1)
                .ok_or_else(|| "usage: nibpass recovery import <file>".to_string())?;
            cmd_recovery_import(store, Path::new(path))
        }
        Some("verify") => {
            let path = args
                .get(1)
                .ok_or_else(|| "usage: nibpass recovery verify <file>".to_string())?;
            cmd_recovery_verify(store, Path::new(path))
        }
        Some("status") | None => cmd_recovery_status(store),
        Some(other) => Err(format!("unknown recovery command '{other}'")),
    }
}

fn cmd_backup(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => cmd_recovery_status(store),
        Some(other) => Err(format!("unknown backup command '{other}'")),
    }
}

fn cmd_recovery_export(store: &Store, out_path: &Path) -> Result<()> {
    store.ensure_exists()?;
    let device_key = read_device_key(store)?;
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    fs::write(out_path, encode_recovery_file(&device_key)).map_err(|err| err.to_string())?;
    set_owner_only_permissions(out_path);
    println!("exported recovery key to {}", out_path.display());
    println!("store this file separately from your git vault backup");
    Ok(())
}

fn cmd_recovery_import(store: &Store, in_path: &Path) -> Result<()> {
    if !store.root.is_dir() {
        fs::create_dir_all(&store.root).map_err(|err| err.to_string())?;
    }
    let text = fs::read_to_string(in_path).map_err(|err| err.to_string())?;
    let device_key = decode_recovery_file(&text)?;
    let device_path = store.device_key_path()?;
    if let Some(parent) = device_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&device_path, hex_encode(&device_key)).map_err(|err| err.to_string())?;
    set_owner_only_permissions(&device_path);
    println!("imported recovery key to {}", device_path.display());
    Ok(())
}

fn cmd_recovery_verify(store: &Store, in_path: &Path) -> Result<()> {
    store.ensure_exists()?;
    let text = fs::read_to_string(in_path).map_err(|err| err.to_string())?;
    let device_key = decode_recovery_file(&text)?;
    let vault_file = fs::read_to_string(store.vault_path()).map_err(|err| err.to_string())?;
    let (salt, nonce, encrypted_key) = decode_vault_file(&vault_file)?;
    let mut password = read_secret("Master password: ")?;
    let master_key = derive_master_key(password.as_bytes(), &device_key, &salt)?;
    password.zeroize();
    let key_bytes = decrypt_bytes(&master_key, &nonce, b"nibpass:vault-key", &encrypted_key)?;
    if key_bytes.len() == 32 {
        println!("recovery key verified");
        Ok(())
    } else {
        Err("recovery key did not unlock the vault".to_string())
    }
}

fn cmd_recovery_status(store: &Store) -> Result<()> {
    let vault_exists = store.vault_path().is_file();
    let device_path = store.device_key_path()?;
    let device_key_exists = device_path.is_file();
    let git_repo = store.is_git_repo();
    let git_remote = git_repo && git_has_remote(store, "origin");

    println!(
        "vault: {}",
        if vault_exists { "present" } else { "missing" }
    );
    println!(
        "device key: {} ({})",
        if device_key_exists {
            "present"
        } else {
            "missing"
        },
        device_path.display()
    );
    println!("git repo: {}", if git_repo { "present" } else { "missing" });
    println!(
        "git remote origin: {}",
        if git_remote { "configured" } else { "missing" }
    );

    if vault_exists && device_key_exists {
        println!("recovery: ready if you also know the master password");
    } else {
        println!("recovery: incomplete");
    }
    Ok(())
}

fn cmd_rotate(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    match args.first().map(String::as_str) {
        Some("master") => cmd_rotate_master(store, no_agent),
        Some("device") => cmd_rotate_device(store, no_agent),
        _ => Err("usage: nibpass rotate <master|device>".to_string()),
    }
}

fn cmd_rotate_master(store: &Store, no_agent: bool) -> Result<()> {
    let vault_key = unlock_vault(store, no_agent)?;
    let mut password = read_secret("New master password: ")?;
    if password.len() < 12 {
        password.zeroize();
        return Err("master password must be at least 12 characters".to_string());
    }
    let mut confirm = read_secret("Confirm new master password: ")?;
    if password != confirm {
        password.zeroize();
        confirm.zeroize();
        return Err("master passwords do not match".to_string());
    }
    let device_key = read_device_key(store)?;
    write_vault_file(store, &vault_key, password.as_bytes(), &device_key)?;
    password.zeroize();
    confirm.zeroize();
    auto_commit_paths(
        store,
        &[PathBuf::from(VAULT_DIR).join(VAULT_FILE)],
        "rotate master password",
    );
    println!("rotated master password");
    Ok(())
}

fn cmd_rotate_device(store: &Store, no_agent: bool) -> Result<()> {
    let vault_key = unlock_vault(store, no_agent)?;
    let mut password = read_secret("Master password: ")?;
    let new_device_key = random_array::<DEVICE_KEY_LEN>();
    write_vault_file(store, &vault_key, password.as_bytes(), &new_device_key)?;
    password.zeroize();
    let device_path = store.device_key_path()?;
    fs::write(&device_path, hex_encode(&new_device_key)).map_err(|err| err.to_string())?;
    set_owner_only_permissions(&device_path);
    auto_commit_paths(
        store,
        &[PathBuf::from(VAULT_DIR).join(VAULT_FILE)],
        "rotate device key",
    );
    println!("rotated device key");
    println!("export a new recovery key with: nibpass recovery export <safe-offline-file>");
    Ok(())
}

fn cmd_import(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    if args.len() < 2 {
        return Err(
            "usage: nibpass import pass <path> | nibpass import csv <path> [--format FORMAT]"
                .to_string(),
        );
    }

    let kind = args[0].to_ascii_lowercase();
    let source = PathBuf::from(&args[1]);
    let mut flags = parse_flags(&args[2..])?;
    let vault_key = unlock_vault(store, no_agent)?;

    if kind == "pass" {
        if !flags.is_empty() {
            let keys = flags.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(format!("unknown option(s): {keys}"));
        }
        if !source.is_dir() {
            return Err(format!("{} is not a directory", source.display()));
        }
        let mut imported = 0usize;
        let mut imported_paths = Vec::new();
        import_pass_dir(
            &source,
            &source,
            &store.root,
            &mut imported,
            &mut imported_paths,
            &vault_key,
        )?;
        if !imported_paths.is_empty() {
            auto_commit_paths(store, &imported_paths, "import pass store");
        }
        println!("imported {imported} encrypted entries");
        return Ok(());
    }

    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    let dry_run = flags.remove("dry-run").is_some();
    let delete_after = flags.remove("delete-after").is_some();

    let format = if kind == "csv" {
        flags
            .remove("format")
            .unwrap_or_else(|| "generic".to_string())
            .to_ascii_lowercase()
    } else if is_csv_import_alias(&kind) {
        kind
    } else {
        return Err(format!("unknown import source '{}'", args[0]));
    };

    if !flags.is_empty() {
        let keys = flags.keys().cloned().collect::<Vec<_>>().join(", ");
        return Err(format!("unknown option(s): {keys}"));
    }

    let mut imported = 0usize;
    let mut imported_paths = Vec::new();
    import_csv_file(
        store,
        &source,
        &format,
        &vault_key,
        &mut imported,
        &mut imported_paths,
        dry_run,
    )?;
    if dry_run {
        println!("would import {imported} accounts");
        return Ok(());
    }
    if !imported_paths.is_empty() {
        auto_commit_paths(store, &imported_paths, &format!("import {format} csv"));
    }
    if delete_after {
        fs::remove_file(&source).map_err(|err| err.to_string())?;
        println!("deleted plaintext import file {}", source.display());
    } else {
        eprintln!(
            "warning: CSV exports are plaintext. Delete {} after import.",
            source.display()
        );
    }
    println!("imported {imported} accounts");
    Ok(())
}

fn cmd_export(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    store.ensure_exists()?;
    if args.len() < 3 || args[0] != "csv" || args[2] != "--plaintext" {
        return Err("usage: nibpass export csv <path> --plaintext".to_string());
    }
    let out_path = Path::new(&args[1]);
    let vault_key = unlock_vault(store, no_agent)?;
    let mut names = Vec::new();
    collect_entries(&store.root, &store.root, &mut names)?;
    let mut out = String::from("name,url,username,password,2fa,notes\n");
    for name in names {
        let entry = read_entry_with_key(store, &name, &vault_key)?;
        out.push_str(&csv_escape(&name));
        out.push(',');
        out.push_str(&csv_escape(
            entry.get("url").map(String::as_str).unwrap_or(""),
        ));
        out.push(',');
        out.push_str(&csv_escape(
            entry.get("username").map(String::as_str).unwrap_or(""),
        ));
        out.push(',');
        out.push_str(&csv_escape(
            entry.get("password").map(String::as_str).unwrap_or(""),
        ));
        out.push(',');
        out.push_str(&csv_escape(
            entry.get("otp").map(String::as_str).unwrap_or(""),
        ));
        out.push(',');
        out.push_str(&csv_escape(
            entry.get("notes").map(String::as_str).unwrap_or(""),
        ));
        out.push('\n');
    }
    fs::write(out_path, out).map_err(|err| err.to_string())?;
    set_owner_only_permissions(out_path);
    eprintln!(
        "warning: wrote plaintext CSV export. Delete or re-encrypt {} as soon as possible.",
        out_path.display()
    );
    Ok(())
}

fn browser_host(store: &Store, no_agent: bool) -> Result<()> {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    loop {
        let Some(request) = read_native_message(&mut stdin)? else {
            return Ok(());
        };
        let response = handle_browser_request(store, &request, no_agent)
            .unwrap_or_else(|err| format!(r#"{{"ok":false,"error":{}}}"#, json_string(&err)));
        write_native_message(&mut stdout, &response)?;
    }
}

fn handle_browser_request(store: &Store, request: &str, no_agent: bool) -> Result<String> {
    let cmd = json_field(request, "cmd").unwrap_or_else(|| "get".to_string());
    match cmd.as_str() {
        "list" => {
            let mut entries = Vec::new();
            collect_entries(&store.root, &store.root, &mut entries)?;
            Ok(format!(
                r#"{{"ok":true,"accounts":[{}]}}"#,
                entries
                    .iter()
                    .map(|entry| json_string(entry))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        "get" => {
            let account = json_field(request, "account")
                .ok_or_else(|| "browser request missing account".to_string())?;
            let requested_field =
                json_field(request, "field").unwrap_or_else(|| "password".to_string());
            let field = field_key(&requested_field);
            let vault_key = unlock_vault(store, no_agent)?;
            let entry = read_entry_with_key(store, &account, &vault_key)?;
            let value = entry
                .get(field)
                .ok_or_else(|| format!("{account} has no field {requested_field}"))?;
            Ok(format!(
                r#"{{"ok":true,"account":{},"field":{},"value":{}}}"#,
                json_string(&account),
                json_string(&requested_field),
                json_string(value)
            ))
        }
        other => Err(format!("unknown browser command '{other}'")),
    }
}

fn read_native_message(input: &mut impl Read) -> Result<Option<String>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.to_string()),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > 1024 * 1024 {
        return Err("browser message too large".to_string());
    }
    let mut buf = vec![0u8; len];
    input.read_exact(&mut buf).map_err(|err| err.to_string())?;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|err| err.to_string())
}

fn write_native_message(output: &mut impl Write, message: &str) -> Result<()> {
    let bytes = message.as_bytes();
    output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|err| err.to_string())?;
    output.write_all(bytes).map_err(|err| err.to_string())?;
    output.flush().map_err(|err| err.to_string())
}

fn json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!(r#""{field}""#);
    let rest = json.split_once(&pattern)?.1;
    let rest = rest.split_once(':')?.1.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in rest.chars() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn browser_manifest(browser: &str) -> Result<String> {
    let host_path = browser_host_wrapper_path()?;
    let path = host_path.display();
    match browser {
        "chrome" | "chromium" | "brave" | "edge" => Ok(format!(
            r#"{{
  "name": "dev.nibtools.nibpass",
  "description": "NibPass native messaging host",
  "path": "{path}",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://REPLACE_WITH_EXTENSION_ID/"
  ]
}}"#
        )),
        "firefox" => Ok(format!(
            r#"{{
  "name": "dev.nibtools.nibpass",
  "description": "NibPass native messaging host",
  "path": "{path}",
  "type": "stdio",
  "allowed_extensions": [
    "nibpass@example.com"
  ]
}}"#
        )),
        other => Err(format!("unsupported browser '{other}'")),
    }
}

fn install_browser_manifest(browser: &str) -> Result<()> {
    install_browser_host_wrapper()?;
    let manifest = browser_manifest(browser)?;
    let path = browser_manifest_path(browser)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&path, manifest).map_err(|err| err.to_string())?;
    println!("installed native messaging manifest at {}", path.display());
    println!("edit the manifest extension id before publishing the browser extension");
    Ok(())
}

fn install_browser_host_wrapper() -> Result<()> {
    let exe = env::current_exe().map_err(|err| err.to_string())?;
    let path = browser_host_wrapper_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let script = if cfg!(target_os = "windows") {
        format!("@echo off\r\n\"{}\" browser host\r\n", exe.display())
    } else {
        format!("#!/bin/sh\nexec \"{}\" browser host\n", exe.display())
    };
    fs::write(&path, script).map_err(|err| err.to_string())?;
    set_executable_permissions(&path);
    Ok(())
}

fn browser_host_wrapper_path() -> Result<PathBuf> {
    let dir = config_dir()?.join(APP_NAME).join("browser");
    Ok(dir.join(if cfg!(target_os = "windows") {
        "nibpass-browser-host.cmd"
    } else {
        "nibpass-browser-host"
    }))
}

fn browser_manifest_path(browser: &str) -> Result<PathBuf> {
    let home = home_dir()?;
    let filename = "dev.nibtools.nibpass.json";
    match (env::consts::OS, browser) {
        ("macos", "chrome") => Ok(home.join(format!(
            "Library/Application Support/Google/Chrome/NativeMessagingHosts/{filename}"
        ))),
        ("macos", "chromium") => Ok(home.join(format!(
            "Library/Application Support/Chromium/NativeMessagingHosts/{filename}"
        ))),
        ("macos", "brave") => Ok(home.join(format!(
            "Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/{filename}"
        ))),
        ("macos", "edge") => Ok(home.join(format!(
            "Library/Application Support/Microsoft Edge/NativeMessagingHosts/{filename}"
        ))),
        ("macos", "firefox") => Ok(home.join(format!(
            "Library/Application Support/Mozilla/NativeMessagingHosts/{filename}"
        ))),
        ("linux", "firefox") => Ok(home.join(format!(".mozilla/native-messaging-hosts/{filename}"))),
        ("linux", _) => Ok(home.join(format!(".config/google-chrome/NativeMessagingHosts/{filename}"))),
        ("windows", _) => {
            Err("browser manifest installation on Windows is not automated yet".to_string())
        }
        (_, other) => Err(format!("unsupported browser '{other}'")),
    }
}

fn cmd_browser(store: &Store, args: &[String], no_agent: bool) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("host") => browser_host(store, no_agent),
        Some("manifest") => {
            let browser = args.get(1).map(String::as_str).unwrap_or("chrome");
            println!("{}", browser_manifest(browser)?);
            Ok(())
        }
        Some("install") => {
            let browser = args.get(1).map(String::as_str).unwrap_or("chrome");
            install_browser_manifest(browser)
        }
        _ => Err("usage: nibpass browser <host|manifest|install> [chrome|firefox]".to_string()),
    }
}

#[cfg(feature = "gui")]
fn cmd_gui(store: &Store) -> Result<()> {
    gui::run(store)
}

#[cfg(not(feature = "gui"))]
fn cmd_gui(_store: &Store) -> Result<()> {
    Err("GTK GUI is available with: cargo run --features gui -- gui".to_string())
}

fn cmd_install_shell(args: &[String]) -> Result<()> {
    let options = ShellSetup::from_args(args)?;
    let shell = options.shell.unwrap_or_else(detect_shell);
    let rc_path = shell_rc_path(&shell)?;
    let bin_dir = options.bin_dir.unwrap_or(current_bin_dir()?);
    let snippet = shell_snippet(&shell, &bin_dir)?;

    if let Some(parent) = rc_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let existing = fs::read_to_string(&rc_path).unwrap_or_default();
    let updated = upsert_managed_block(&existing, &snippet);
    if updated == existing {
        println!("shell setup already present in {}", rc_path.display());
        return Ok(());
    }

    fs::write(&rc_path, updated).map_err(|err| err.to_string())?;
    println!("updated {}", rc_path.display());
    println!("restart your shell or run: source {}", rc_path.display());
    Ok(())
}

fn cmd_shellenv(args: &[String]) -> Result<()> {
    let options = ShellSetup::from_args(args)?;
    let shell = options.shell.unwrap_or_else(detect_shell);
    let bin_dir = options.bin_dir.unwrap_or(current_bin_dir()?);
    print!("{}", shell_snippet(&shell, &bin_dir)?);
    Ok(())
}

fn cmd_completion(args: &[String]) -> Result<()> {
    let shell = args
        .first()
        .ok_or_else(|| "usage: nibpass completion <zsh|bash|fish>".to_string())?;
    match shell.as_str() {
        "zsh" => print_completion_zsh(),
        "bash" => print_completion_bash(),
        "fish" => print_completion_fish(),
        other => return Err(format!("unsupported shell '{other}'")),
    }
    Ok(())
}

fn print_completion_zsh() {
    println!(
        "#compdef nibpass\n_arguments '1:command:(init gen add set edit show find audit ls rm 2fa agent unlock lock status recovery backup sync rotate git import install-shell shellenv completion)' '*:account:->accounts'\ncase $state in\n  accounts) compadd $(nibpass ls 2>/dev/null) ;;\nesac"
    );
}

fn print_completion_bash() {
    println!(
        "_nibpass() {{\n  local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n  local cmds=\"init gen add set edit show find audit ls rm 2fa agent unlock lock status recovery backup sync rotate git import install-shell shellenv completion\"\n  if [[ $COMP_CWORD -eq 1 ]]; then COMPREPLY=( $(compgen -W \"$cmds\" -- \"$cur\") ); else COMPREPLY=( $(compgen -W \"$(nibpass ls 2>/dev/null)\" -- \"$cur\") ); fi\n}}\ncomplete -F _nibpass nibpass"
    );
}

fn print_completion_fish() {
    println!(
        "complete -c nibpass -f -n '__fish_use_subcommand' -a 'init gen add set edit show find audit ls rm 2fa agent unlock lock status recovery backup sync rotate git import install-shell shellenv completion'\ncomplete -c nibpass -f -a '(nibpass ls 2>/dev/null)'"
    );
}

fn cmd_agent(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") => agent_status(store),
        Some("lock") => agent_lock(store),
        Some("unlock") => run_agent(store, &args[1..]),
        _ => run_agent(store, args),
    }
}

fn run_agent(store: &Store, args: &[String]) -> Result<()> {
    use std::net::TcpListener;

    store.ensure_exists()?;
    let ttl = parse_agent_ttl(args)?;
    let mut vault_key = unlock_vault_direct(store)?;
    let control_path = agent_control_path(store)?;
    if let Some(parent) = control_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        set_dir_owner_only_permissions(parent);
    }
    let _ = fs::remove_file(&control_path);
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|err| err.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    let token = hex_encode(&random_array::<32>());
    fs::write(&control_path, format!("{port}\n{token}\n")).map_err(|err| err.to_string())?;
    set_owner_only_permissions(&control_path);

    println!("nibpass agent unlocked for {} seconds", ttl.as_secs());
    let expires_at = Instant::now() + ttl;
    while Instant::now() < expires_at {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = String::new();
                stream
                    .read_to_string(&mut request)
                    .map_err(|err| err.to_string())?;
                let mut lines = request.lines();
                let provided_token = lines.next().unwrap_or_default();
                let command = lines.next().unwrap_or_default();
                if provided_token != token {
                    stream
                        .write_all(b"unauthorized\n")
                        .map_err(|err| err.to_string())?;
                    continue;
                }
                match command {
                    "GET" => {
                        stream
                            .write_all(hex_encode(&vault_key).as_bytes())
                            .map_err(|err| err.to_string())?;
                    }
                    "STATUS" => {
                        let remaining = expires_at
                            .saturating_duration_since(Instant::now())
                            .as_secs();
                        stream
                            .write_all(format!("unlocked {remaining}\n").as_bytes())
                            .map_err(|err| err.to_string())?;
                    }
                    "LOCK" => {
                        stream
                            .write_all(b"locked\n")
                            .map_err(|err| err.to_string())?;
                        break;
                    }
                    _ => {
                        stream
                            .write_all(b"error\n")
                            .map_err(|err| err.to_string())?;
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    vault_key.zeroize();
    let _ = fs::remove_file(control_path);
    Ok(())
}

fn parse_agent_ttl(args: &[String]) -> Result<Duration> {
    let mut ttl = SESSION_AGENT_TTL_SECONDS;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--session" => {
                ttl = SESSION_AGENT_TTL_SECONDS;
                i += 1;
            }
            "--ttl" => {
                ttl = args
                    .get(i + 1)
                    .ok_or_else(|| "--ttl requires seconds".to_string())?
                    .parse::<u64>()
                    .map_err(|_| "--ttl must be a number of seconds".to_string())?;
                i += 2;
            }
            other => return Err(format!("unknown agent option '{other}'")),
        }
    }
    Ok(Duration::from_secs(ttl))
}

fn agent_get_key(store: &Store) -> Result<[u8; 32]> {
    let response = agent_request(store, "GET")?;
    hex_decode_fixed::<32>(response.trim())
}

fn agent_status(store: &Store) -> Result<()> {
    match agent_request(store, "STATUS") {
        Ok(status) => print!("{status}"),
        Err(_) => println!("locked"),
    }
    Ok(())
}

fn agent_lock(store: &Store) -> Result<()> {
    match agent_request(store, "LOCK") {
        Ok(response) => print!("{response}"),
        Err(_) => println!("locked"),
    }
    Ok(())
}

fn agent_request(store: &Store, request: &str) -> Result<String> {
    use std::net::{Shutdown, TcpStream};

    let (port, token) = read_agent_control(store)?;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|err| err.to_string())?;
    stream
        .write_all(format!("{token}\n{request}\n").as_bytes())
        .map_err(|err| err.to_string())?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| err.to_string())?;
    Ok(response)
}

fn read_agent_control(store: &Store) -> Result<(u16, String)> {
    let text = fs::read_to_string(agent_control_path(store)?).map_err(|err| err.to_string())?;
    let mut lines = text.lines();
    let port = lines
        .next()
        .ok_or_else(|| "agent control file missing port".to_string())?
        .parse::<u16>()
        .map_err(|_| "agent control file has invalid port".to_string())?;
    let token = lines
        .next()
        .ok_or_else(|| "agent control file missing token".to_string())?
        .to_string();
    Ok((port, token))
}

fn agent_control_path(store: &Store) -> Result<PathBuf> {
    let root_hash = hex_encode(&sha1(store.root.to_string_lossy().as_bytes()));
    let short_hash = &root_hash[..20];
    let base = env::var_os("NIBPASS_AGENT_DIR")
        .or_else(|| env::var_os("XDG_RUNTIME_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let private_tmp = PathBuf::from("/private/tmp");
            if private_tmp.is_dir() {
                private_tmp
            } else {
                env::temp_dir()
            }
        });
    Ok(base.join(format!("np-{short_hash}.agent")))
}

fn parse_flags(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut flags = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let Some(name) = arg.strip_prefix("--") else {
            return Err(format!("unexpected argument '{arg}'"));
        };
        if matches!(
            name,
            "password-stdin" | "dialog" | "generate" | "dry-run" | "delete-after"
        ) {
            flags.insert(name.to_string(), String::new());
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("--{name} requires a value"))?;
        flags.insert(name.to_string(), value.clone());
        i += 2;
    }
    Ok(flags)
}

fn parse_name_and_flags(args: &[String]) -> Result<(Option<String>, BTreeMap<String, String>)> {
    if args.is_empty() {
        return Ok((None, BTreeMap::new()));
    }
    if args[0].starts_with("--") {
        return Ok((None, parse_flags(args)?));
    }
    Ok((Some(args[0].clone()), parse_flags(&args[1..])?))
}

struct ShellSetup {
    shell: Option<String>,
    bin_dir: Option<PathBuf>,
}

impl ShellSetup {
    fn from_args(args: &[String]) -> Result<Self> {
        let mut shell = None;
        let mut bin_dir = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--shell" => {
                    shell = Some(
                        args.get(i + 1)
                            .ok_or_else(|| "--shell requires a value".to_string())?
                            .to_ascii_lowercase(),
                    );
                    i += 2;
                }
                "--bin-dir" => {
                    bin_dir = Some(PathBuf::from(
                        args.get(i + 1)
                            .ok_or_else(|| "--bin-dir requires a value".to_string())?,
                    ));
                    i += 2;
                }
                other => return Err(format!("unknown option '{other}'")),
            }
        }
        Ok(Self { shell, bin_dir })
    }
}

fn detect_shell() -> String {
    env::var("SHELL")
        .ok()
        .and_then(|shell| {
            Path::new(&shell)
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
        })
        .filter(|shell| shell == "zsh" || shell == "bash" || shell == "fish")
        .unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                "powershell".to_string()
            } else {
                "sh".to_string()
            }
        })
}

fn shell_rc_path(shell: &str) -> Result<PathBuf> {
    let home = home_dir()?;
    match shell {
        "zsh" => Ok(home.join(".zshrc")),
        "bash" => Ok(home.join(".bashrc")),
        "fish" => Ok(home.join(".config/fish/config.fish")),
        "sh" => Ok(home.join(".profile")),
        other => Err(format!(
            "unsupported shell '{other}'. use --shell zsh, --shell bash, or --shell fish"
        )),
    }
}

fn current_bin_dir() -> Result<PathBuf> {
    let exe = env::current_exe().map_err(|err| err.to_string())?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not detect nibpass binary directory".to_string())
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .ok_or_else(|| "could not detect home directory".to_string())
}

fn shell_snippet(shell: &str, bin_dir: &Path) -> Result<String> {
    let bin_dir = bin_dir.display();
    match shell {
        "fish" => Ok(format!(
            "\n# >>> nibpass >>>\n\
if not contains \"{bin_dir}\" $PATH\n\
    fish_add_path \"{bin_dir}\"\n\
end\n\
# <<< nibpass <<<\n"
        )),
        "zsh" | "bash" | "sh" => Ok(format!(
            "\n# >>> nibpass >>>\n\
case \":$PATH:\" in\n\
  *:\"{bin_dir}\":*) ;;\n\
  *) export PATH=\"{bin_dir}:$PATH\" ;;\n\
esac\n\
# <<< nibpass <<<\n"
        )),
        other => Err(format!(
            "unsupported shell '{other}'. use --shell zsh, --shell bash, or --shell fish"
        )),
    }
}

fn upsert_managed_block(existing: &str, snippet: &str) -> String {
    let start = "# >>> nibpass >>>";
    let end = "# <<< nibpass <<<";
    if let Some(start_index) = existing.find(start) {
        if let Some(relative_end) = existing[start_index..].find(end) {
            let end_index = start_index + relative_end + end.len();
            let mut updated = String::new();
            updated.push_str(existing[..start_index].trim_end());
            updated.push_str(snippet);
            updated.push_str(existing[end_index..].trim_start_matches(['\r', '\n']));
            return updated;
        }
    }

    let mut updated = existing.trim_end().to_string();
    updated.push_str(snippet);
    updated
}

fn encode_entry(fields: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in fields {
        let value = value.replace('\n', "\\n");
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&value);
        out.push('\n');
    }
    out
}

fn encode_recovery_file(device_key: &[u8; DEVICE_KEY_LEN]) -> String {
    format!("{RECOVERY_MAGIC}\ndevice_key:{}\n", hex_encode(device_key))
}

fn decode_recovery_file(text: &str) -> Result<[u8; DEVICE_KEY_LEN]> {
    let trimmed = text.trim();
    if !trimmed.starts_with(RECOVERY_MAGIC) {
        return hex_decode_fixed::<DEVICE_KEY_LEN>(trimmed);
    }
    for line in trimmed.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            if key == "device_key" {
                return hex_decode_fixed::<DEVICE_KEY_LEN>(value.trim());
            }
        }
    }
    Err("recovery file missing device_key".to_string())
}

fn decode_entry(text: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(
                key.trim().to_string(),
                value.trim_start().replace("\\n", "\n"),
            );
        }
    }
    fields
}

fn ensure_device_key(store: &Store) -> Result<[u8; DEVICE_KEY_LEN]> {
    let path = store.device_key_path()?;
    if path.is_file() {
        return read_device_key(store);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let key = random_array::<DEVICE_KEY_LEN>();
    fs::write(&path, hex_encode(&key)).map_err(|err| err.to_string())?;
    set_owner_only_permissions(&path);
    eprintln!("created device key at {}", path.display());
    Ok(key)
}

fn read_device_key(store: &Store) -> Result<[u8; DEVICE_KEY_LEN]> {
    let path = store.device_key_path()?;
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read device key {}: {err}", path.display()))?;
    hex_decode_fixed::<DEVICE_KEY_LEN>(text.trim())
}

fn set_owner_only_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

fn set_dir_owner_only_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
}

fn set_executable_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
}

fn unlock_vault(store: &Store, no_agent: bool) -> Result<[u8; 32]> {
    if !no_agent {
        if let Ok(key) = agent_get_key(store) {
            return Ok(key);
        }
    }
    unlock_vault_direct(store)
}

fn unlock_vault_direct(store: &Store) -> Result<[u8; 32]> {
    let vault_file = fs::read_to_string(store.vault_path()).map_err(|err| err.to_string())?;
    let (salt, nonce, encrypted_key) = decode_vault_file(&vault_file)?;
    let device_key = read_device_key(store)?;
    let mut password = read_secret("Master password: ")?;
    let master_key = derive_master_key(password.as_bytes(), &device_key, &salt)?;
    password.zeroize();
    let key_bytes = decrypt_bytes(&master_key, &nonce, b"nibpass:vault-key", &encrypted_key)?;
    key_bytes
        .try_into()
        .map_err(|_| "vault key has invalid length".to_string())
}

fn derive_master_key(password: &[u8], device_key: &[u8], salt: &[u8]) -> Result<[u8; 32]> {
    let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_PASSES, ARGON2_LANES, Some(32))
        .map_err(|err| err.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut material = Vec::with_capacity(password.len() + device_key.len() + 16);
    material.extend_from_slice(b"nibpass-v1:");
    material.extend_from_slice(password);
    material.extend_from_slice(device_key);
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(&material, salt, &mut out)
        .map_err(|err| err.to_string())?;
    material.zeroize();
    Ok(out)
}

fn encrypt_bytes(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(XNonce::from_slice(nonce), Payload { msg: plain, aad })
        .map_err(|_| "encryption failed".to_string())
}

fn decrypt_bytes(
    key: &[u8; 32],
    nonce: &[u8; 24],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| "decrypt failed: wrong master password or corrupted data".to_string())
}

fn random_array<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn generate_password(length: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_=+?";
    random_from_alphabet(CHARS, length)
}

fn generate_words(count: usize) -> String {
    const WORDS: &[&str] = &[
        "amber", "atlas", "cinder", "delta", "ember", "falcon", "glacier", "harbor", "indigo",
        "jupiter", "kernel", "lantern", "matrix", "nickel", "onyx", "prairie", "quartz", "raven",
        "signal", "timber", "umbra", "velvet", "willow", "xenon", "yonder", "zenith",
    ];
    (0..count)
        .map(|_| WORDS[random_index(WORDS.len())])
        .collect::<Vec<_>>()
        .join("-")
}

fn random_from_alphabet(alphabet: &[u8], length: usize) -> String {
    (0..length)
        .map(|_| alphabet[random_index(alphabet.len())] as char)
        .collect()
}

fn random_index(len: usize) -> usize {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    (u64::from_be_bytes(bytes) as usize) % len
}

fn encode_vault_file(salt: &[u8; 16], nonce: &[u8; 24], encrypted_key: &[u8]) -> String {
    format!(
        "{VAULT_MAGIC}\nsalt:{}\nnonce:{}\nkey:{}\n",
        hex_encode(salt),
        hex_encode(nonce),
        hex_encode(encrypted_key)
    )
}

fn write_vault_file(
    store: &Store,
    vault_key: &[u8; 32],
    password: &[u8],
    device_key: &[u8; DEVICE_KEY_LEN],
) -> Result<()> {
    let salt = random_array::<16>();
    let master_key = derive_master_key(password, device_key, &salt)?;
    let nonce = random_array::<24>();
    let encrypted_key = encrypt_bytes(&master_key, &nonce, b"nibpass:vault-key", vault_key)?;
    fs::write(
        store.vault_path(),
        encode_vault_file(&salt, &nonce, &encrypted_key),
    )
    .map_err(|err| err.to_string())
}

fn decode_vault_file(text: &str) -> Result<([u8; 16], [u8; 24], Vec<u8>)> {
    let mut lines = text.lines();
    if lines.next() != Some(VAULT_MAGIC) {
        return Err("not a NibPass vault file".to_string());
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key, value);
        }
    }
    let salt = hex_decode_fixed::<16>(
        fields
            .get("salt")
            .ok_or_else(|| "vault file missing salt".to_string())?,
    )?;
    let nonce = hex_decode_fixed::<24>(
        fields
            .get("nonce")
            .ok_or_else(|| "vault file missing nonce".to_string())?,
    )?;
    let encrypted_key = hex_decode(
        fields
            .get("key")
            .ok_or_else(|| "vault file missing key".to_string())?,
    )?;
    Ok((salt, nonce, encrypted_key))
}

fn encode_entry_file(nonce: &[u8; 24], ciphertext: &[u8]) -> String {
    format!(
        "{ENTRY_MAGIC}\nnonce:{}\ndata:{}\n",
        hex_encode(nonce),
        hex_encode(ciphertext)
    )
}

fn decode_encrypted_file(text: &str, magic: &str) -> Result<([u8; 24], Vec<u8>)> {
    let mut lines = text.lines();
    if lines.next() != Some(magic) {
        return Err("not a NibPass encrypted entry".to_string());
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key, value);
        }
    }
    let nonce = hex_decode_fixed::<24>(
        fields
            .get("nonce")
            .ok_or_else(|| "entry missing nonce".to_string())?,
    )?;
    let ciphertext = hex_decode(
        fields
            .get("data")
            .ok_or_else(|| "entry missing encrypted data".to_string())?,
    )?;
    Ok((nonce, ciphertext))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode_fixed<const N: usize>(value: &str) -> Result<[u8; N]> {
    let bytes = hex_decode(value)?;
    bytes
        .try_into()
        .map_err(|_| format!("expected {N} decoded bytes"))
}

fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("invalid hex length".to_string());
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    for i in (0..value.len()).step_by(2) {
        out.push(u8::from_str_radix(&value[i..i + 2], 16).map_err(|err| err.to_string())?);
    }
    Ok(out)
}

fn read_entry_with_key(
    store: &Store,
    name: &str,
    vault_key: &[u8; 32],
) -> Result<BTreeMap<String, String>> {
    let path = store.entry_path(name)?;
    let plain = decrypt_entry_file(&path, vault_key)?;
    let entry = decode_entry(&plain);
    if entry.is_empty() {
        return Err(format!("{name} is empty or not a NibPass entry"));
    }
    Ok(entry)
}

fn write_entry_with_key(
    store: &Store,
    name: &str,
    entry: &BTreeMap<String, String>,
    vault_key: &[u8; 32],
) -> Result<()> {
    let out_path = store.entry_path(name)?;
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_entry_to_path_with_key(vault_key, entry, &out_path)
}

fn write_entry_to_path_with_key(
    vault_key: &[u8; 32],
    entry: &BTreeMap<String, String>,
    out_path: &Path,
) -> Result<()> {
    let nonce = random_array::<24>();
    let plain = encode_entry(entry);
    let ciphertext = encrypt_bytes(vault_key, &nonce, ENTRY_MAGIC.as_bytes(), plain.as_bytes())?;
    fs::write(out_path, encode_entry_file(&nonce, &ciphertext)).map_err(|err| err.to_string())
}

fn decrypt_entry_file(path: &Path, vault_key: &[u8; 32]) -> Result<String> {
    let data = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let (nonce, ciphertext) = decode_encrypted_file(&data, ENTRY_MAGIC)?;
    let plain = decrypt_bytes(vault_key, &nonce, ENTRY_MAGIC.as_bytes(), &ciphertext)?;
    String::from_utf8(plain).map_err(|err| err.to_string())
}

fn decrypt_gpg_file(path: &Path) -> Result<String> {
    let output = Command::new("gpg")
        .arg("--quiet")
        .arg("--decrypt")
        .arg(path)
        .output()
        .map_err(|err| format!("failed to run gpg: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout).map_err(|err| err.to_string())
}

fn read_secret(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().map_err(|err| err.to_string())?;
    let _ = Command::new("stty").arg("-echo").status();
    let mut secret = String::new();
    let result = io::stdin().read_line(&mut secret);
    let _ = Command::new("stty").arg("echo").status();
    eprintln!();
    result.map_err(|err| err.to_string())?;
    Ok(secret.trim_end_matches(['\r', '\n']).to_string())
}

fn read_required(prompt: &str) -> Result<String> {
    loop {
        let value = read_optional(prompt)?;
        if !value.trim().is_empty() {
            return Ok(value);
        }
        eprintln!("required");
    }
}

fn read_optional(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().map_err(|err| err.to_string())?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|err| err.to_string())?;
    Ok(value.trim_end_matches(['\r', '\n']).to_string())
}

fn yes_no(prompt: &str) -> Result<bool> {
    let answer = read_optional(prompt)?;
    Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn is_weak_password(password: &str) -> bool {
    if password.len() < 12 {
        return true;
    }
    let lower = password.to_ascii_lowercase();
    if ["password", "qwerty", "letmein", "123456", "admin"]
        .iter()
        .any(|bad| lower.contains(bad))
    {
        return true;
    }
    let has_lower = password.chars().any(|ch| ch.is_ascii_lowercase());
    let has_upper = password.chars().any(|ch| ch.is_ascii_uppercase());
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    let has_symbol = password.chars().any(|ch| !ch.is_ascii_alphanumeric());
    [has_lower, has_upper, has_digit, has_symbol]
        .iter()
        .filter(|value| **value)
        .count()
        < 3
}

fn field_label(value: &str) -> String {
    if value == "otp" {
        return "2FA".to_string();
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn field_key(value: &str) -> &str {
    if value.eq_ignore_ascii_case("2fa") {
        "otp"
    } else {
        value
    }
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };

    for (cmd, args) in commands {
        let mut child = match Command::new(cmd).args(*args).stdin(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(_) => continue,
        };
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| err.to_string())?;
        }
        let status = child.wait().map_err(|err| err.to_string())?;
        if status.success() {
            spawn_clipboard_clear(text);
            return Ok(());
        }
    }
    Err("no supported clipboard command found".to_string())
}

fn cmd_clear_clipboard(args: &[String]) -> Result<()> {
    let seconds = args
        .first()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(CLIPBOARD_CLEAR_SECONDS);
    let mut expected = String::new();
    io::stdin()
        .read_to_string(&mut expected)
        .map_err(|err| err.to_string())?;
    std::thread::sleep(Duration::from_secs(seconds));
    if read_clipboard().ok().as_deref() == Some(expected.as_str()) {
        let _ = write_clipboard("");
    }
    expected.zeroize();
    Ok(())
}

fn spawn_clipboard_clear(text: &str) {
    let Ok(exe) = env::current_exe() else {
        return;
    };
    let Ok(mut child) = Command::new(exe)
        .arg("clear-clipboard")
        .arg(CLIPBOARD_CLEAR_SECONDS.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(text.as_bytes());
    }
}

fn write_clipboard(text: &str) -> Result<()> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };

    for (cmd, args) in commands {
        let mut child = match Command::new(cmd).args(*args).stdin(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(_) => continue,
        };
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| err.to_string())?;
        }
        let status = child.wait().map_err(|err| err.to_string())?;
        if status.success() {
            return Ok(());
        }
    }
    Err("no supported clipboard command found".to_string())
}

fn read_clipboard() -> Result<String> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbpaste", &[])]
    } else if cfg!(target_os = "windows") {
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])]
    } else {
        &[
            ("wl-paste", &[]),
            ("xclip", &["-selection", "clipboard", "-out"]),
            ("xsel", &["--clipboard", "--output"]),
        ]
    };

    for (cmd, args) in commands {
        let output = match Command::new(cmd).args(*args).output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        if output.status.success() {
            return String::from_utf8(output.stdout).map_err(|err| err.to_string());
        }
    }
    Err("no supported clipboard read command found".to_string())
}

fn collect_entries(root: &Path, dir: &Path, entries: &mut Vec<String>) -> Result<()> {
    for item in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let item = item.map_err(|err| err.to_string())?;
        let path = item.path();
        if path.file_name() == Some(OsStr::new(".git")) {
            continue;
        }
        if path.is_dir() {
            collect_entries(root, &path, entries)?;
        } else if path.extension() == Some(OsStr::new("nib")) {
            let relative = path.strip_prefix(root).map_err(|err| err.to_string())?;
            let name = relative.with_extension("");
            entries.push(name.to_string_lossy().to_string());
        }
    }
    entries.sort();
    Ok(())
}

fn import_pass_dir(
    source_root: &Path,
    dir: &Path,
    dest_root: &Path,
    imported: &mut usize,
    imported_paths: &mut Vec<PathBuf>,
    vault_key: &[u8; 32],
) -> Result<()> {
    for item in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let item = item.map_err(|err| err.to_string())?;
        let path = item.path();
        if path.file_name() == Some(OsStr::new(".git")) {
            continue;
        }
        if path.is_dir() {
            import_pass_dir(
                source_root,
                &path,
                dest_root,
                imported,
                imported_paths,
                vault_key,
            )?;
        } else if path.extension() == Some(OsStr::new("gpg")) {
            let relative = path
                .strip_prefix(source_root)
                .map_err(|err| err.to_string())?;
            let dest = dest_root.join(relative.with_extension("nib"));
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            let pass_entry = decrypt_gpg_file(&path)?;
            let entry = convert_pass_entry(&pass_entry)?;
            write_entry_to_path_with_key(vault_key, &entry, &dest)?;
            *imported += 1;
            imported_paths.push(relative.with_extension("nib"));
        }
    }
    Ok(())
}

fn import_csv_file(
    store: &Store,
    source: &Path,
    format: &str,
    vault_key: &[u8; 32],
    imported: &mut usize,
    imported_paths: &mut Vec<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    if !is_csv_import_alias(format) && format != "generic" {
        return Err(format!(
            "unsupported csv format '{format}'. use bitwarden, chrome, apple, firefox, 1password, xpass, or generic"
        ));
    }

    let csv = fs::read_to_string(source).map_err(|err| err.to_string())?;
    let rows = parse_csv(&csv)?;
    if rows.is_empty() {
        return Ok(());
    }

    let headers = rows[0]
        .iter()
        .map(|header| normalize_header(header))
        .collect::<Vec<_>>();
    let mut used_names = existing_account_names(store)?;

    for row in rows.iter().skip(1) {
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let record = csv_record(&headers, row);
        let Some(entry) = entry_from_csv_record(&record) else {
            continue;
        };
        let base_name = account_name_from_record(&record);
        let account_name = unique_account_name(&base_name, &mut used_names);
        if dry_run {
            println!("{account_name}");
            *imported += 1;
            continue;
        }
        let relative_path = store.entry_relative_path(&account_name)?;
        let out_path = store.root.join(&relative_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        write_entry_to_path_with_key(vault_key, &entry, &out_path)?;
        imported_paths.push(relative_path);
        *imported += 1;
    }

    Ok(())
}

fn is_csv_import_alias(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "bitwarden" | "chrome" | "apple" | "firefox" | "1password" | "1pass" | "op" | "xpass"
    )
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                row.push(field);
                field = String::new();
            }
            '\n' if !in_quotes => {
                row.push(field);
                field = String::new();
                rows.push(row);
                row = Vec::new();
            }
            '\r' if !in_quotes => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(field);
                field = String::new();
                rows.push(row);
                row = Vec::new();
            }
            _ => field.push(ch),
        }
    }

    if in_quotes {
        return Err("csv has an unterminated quoted field".to_string());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn csv_record(headers: &[String], row: &[String]) -> BTreeMap<String, String> {
    let mut record = BTreeMap::new();
    for (index, header) in headers.iter().enumerate() {
        if let Some(value) = row.get(index) {
            record.insert(header.clone(), value.trim().to_string());
        }
    }
    record
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn entry_from_csv_record(record: &BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    let password = first_record_value(
        record,
        &[
            "password",
            "login_password",
            "current_password",
            "pass",
            "secret",
        ],
    )?;
    if password.is_empty() {
        return None;
    }

    let mut entry = BTreeMap::new();
    entry.insert("password".to_string(), password);

    if let Some(username) = first_record_value(
        record,
        &[
            "username",
            "login_username",
            "user",
            "login",
            "email",
            "account",
        ],
    ) {
        entry.insert("username".to_string(), username);
    }
    if let Some(url) = first_record_value(
        record,
        &[
            "url",
            "login_uri",
            "uri",
            "website",
            "site",
            "origin",
            "hostname",
            "location",
        ],
    ) {
        entry.insert("url".to_string(), url);
    }
    if let Some(secret) = first_record_value(
        record,
        &[
            "2fa",
            "otp",
            "totp",
            "login_totp",
            "otpauth",
            "otp_auth",
            "one_time_password",
        ],
    )
    .and_then(|value| normalize_2fa_secret(&value).ok())
    {
        if base32_decode(&secret).is_ok() {
            entry.insert("otp".to_string(), secret);
        }
    }
    if let Some(notes) = first_record_value(record, &["notes", "note", "comments", "extra"]) {
        entry.insert("notes".to_string(), notes);
    }

    Some(entry)
}

fn account_name_from_record(record: &BTreeMap<String, String>) -> String {
    if let Some(folder) = first_record_value(record, &["folder", "folder_name", "category"]) {
        if let Some(name) = first_record_value(record, &["name", "title", "label"]) {
            return format!("{}/{}", slug_component(&folder), slug_component(&name));
        }
    }
    if let Some(name) = first_record_value(record, &["name", "title", "label"]) {
        return slug_component(&name);
    }
    if let Some(url) = first_record_value(record, &["url", "login_uri", "uri", "website", "site"]) {
        return slug_component(&domain_from_url(&url));
    }
    if let Some(username) = first_record_value(record, &["username", "login_username", "email"]) {
        return slug_component(&username);
    }
    "imported-account".to_string()
}

fn first_record_value(record: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = record.get(*key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn existing_account_names(store: &Store) -> Result<Vec<String>> {
    let mut entries = Vec::new();
    collect_entries(&store.root, &store.root, &mut entries)?;
    Ok(entries)
}

fn unique_account_name(base: &str, used: &mut Vec<String>) -> String {
    let base = if base.is_empty() {
        "imported-account"
    } else {
        base
    };
    if !used.iter().any(|name| name == base) {
        used.push(base.to_string());
        return base.to_string();
    }
    for index in 2.. {
        let candidate = format!("{base}-{index}");
        if !used.iter().any(|name| name == &candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }
    unreachable!()
}

fn normalize_header(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('\u{feff}')
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn slug_component(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if matches!(ch, '.' | '_' | '-') {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches(['-', '.', '_']).to_string();
    if out.is_empty() {
        "imported-account".to_string()
    } else {
        out
    }
}

fn domain_from_url(value: &str) -> String {
    let without_scheme = value
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(value);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .split('@')
        .next_back()
        .unwrap_or(without_scheme)
        .split(':')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

fn convert_pass_entry(text: &str) -> Result<BTreeMap<String, String>> {
    let mut lines = text.lines();
    let password = lines
        .next()
        .ok_or_else(|| "pass entry is empty".to_string())?
        .trim()
        .to_string();
    if password.is_empty() {
        return Err("pass entry password is empty".to_string());
    }

    let mut entry = BTreeMap::new();
    entry.insert("password".to_string(), password);

    let mut notes = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(secret) = parse_otpauth_2fa_secret(trimmed) {
            entry.insert("otp".to_string(), secret);
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user" | "username" | "login" => {
                    entry.insert("username".to_string(), value.to_string());
                }
                "url" | "site" | "website" => {
                    entry.insert("url".to_string(), value.to_string());
                }
                "otp" | "totp" | "secret" => {
                    entry.insert("otp".to_string(), value.to_string());
                }
                _ => notes.push(trimmed.to_string()),
            }
        } else {
            notes.push(trimmed.to_string());
        }
    }

    if !notes.is_empty() {
        entry.insert("notes".to_string(), notes.join("\n"));
    }
    Ok(entry)
}

fn parse_otpauth_2fa_secret(value: &str) -> Option<String> {
    let query = value.strip_prefix("otpauth://")?.split_once('?')?.1;
    for part in query.split('&') {
        if let Some((key, value)) = part.split_once('=') {
            if key.eq_ignore_ascii_case("secret") {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

fn normalize_2fa_secret(value: &str) -> Result<String> {
    let secret = parse_otpauth_2fa_secret(value).unwrap_or_else(|| value.to_string());
    let secret = secret
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    if secret.is_empty() {
        Err("2fa secret cannot be empty".to_string())
    } else {
        Ok(secret)
    }
}

fn is_2fa_alias(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "2fa" | "otp" | "totp")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn run_in<I, S>(dir: &Path, program: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed"))
    }
}

fn ensure_git_repo(store: &Store) {
    if store.is_git_repo() {
        return;
    }
    if let Err(err) = run_in(&store.root, "git", ["init"]) {
        eprintln!("warning: git history disabled: {err}");
    }
}

fn auto_commit_paths(store: &Store, paths: &[PathBuf], message: &str) {
    if paths.is_empty() {
        return;
    }
    ensure_git_repo(store);
    if !store.is_git_repo() {
        return;
    }

    let mut add_args = vec!["add".to_string(), "-A".to_string(), "--".to_string()];
    add_args.extend(paths.iter().map(|path| path.to_string_lossy().to_string()));
    if let Err(err) = git_quiet(&store.root, &add_args) {
        eprintln!("warning: git add failed: {err}");
        return;
    }

    match git_status(&store.root, &["diff", "--cached", "--quiet", "--"]) {
        Ok(0) => return,
        Ok(1) => {}
        Ok(_) => {
            eprintln!("warning: could not inspect staged git changes");
            return;
        }
        Err(err) => {
            eprintln!("warning: could not inspect staged git changes: {err}");
            return;
        }
    }

    let commit_args = vec!["commit".to_string(), "-m".to_string(), message.to_string()];
    if let Err(err) = git_quiet(&store.root, &commit_args) {
        eprintln!("warning: git commit failed: {err}");
    }
}

fn git_status(dir: &Path, args: &[&str]) -> Result<i32> {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| format!("failed to run git: {err}"))?;
    Ok(status.code().unwrap_or(1))
}

fn git_quiet(dir: &Path, args: &[String]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err("git failed".to_string())
        } else {
            Err(stderr)
        }
    }
}

fn git_has_remote(store: &Store, name: &str) -> bool {
    Command::new("git")
        .args(["remote", "get-url", name])
        .current_dir(&store.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_passthrough(dir: &Path, program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed"))
    }
}

fn now_unix() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_secs())
}

fn totp(secret: &str, digits: u32, period: u64) -> Result<String> {
    let key = base32_decode(secret)?;
    let counter = now_unix()? / period;
    let msg = counter.to_be_bytes();
    let hash = hmac_sha1(&key, &msg);
    let offset = (hash[19] & 0x0f) as usize;
    let binary = (((hash[offset] & 0x7f) as u32) << 24)
        | ((hash[offset + 1] as u32) << 16)
        | ((hash[offset + 2] as u32) << 8)
        | (hash[offset + 3] as u32);
    let modulo = 10u32.pow(digits);
    Ok(format!(
        "{:0width$}",
        binary % modulo,
        width = digits as usize
    ))
}

fn base32_decode(input: &str) -> Result<Vec<u8>> {
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut out = Vec::new();
    for ch in input.chars().filter(|ch| !ch.is_whitespace() && *ch != '=') {
        let value = match ch.to_ascii_uppercase() {
            'A'..='Z' => ch.to_ascii_uppercase() as u8 - b'A',
            '2'..='7' => ch as u8 - b'2' + 26,
            _ => return Err("2fa secret is not valid base32".to_string()),
        } as u32;
        bits = (bits << 5) | value;
        bit_count += 5;
        if bit_count >= 8 {
            bit_count -= 8;
            out.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    if out.is_empty() {
        Err("2fa secret is empty".to_string())
    } else {
        Ok(out)
    }
}

fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        key_block[..20].copy_from_slice(&sha1(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer = [0x5c; 64];
    let mut inner = [0x36; 64];
    for i in 0..64 {
        outer[i] ^= key_block[i];
        inner[i] ^= key_block[i];
    }

    let mut inner_data = Vec::with_capacity(64 + message.len());
    inner_data.extend_from_slice(&inner);
    inner_data.extend_from_slice(message);
    let inner_hash = sha1(&inner_data);

    let mut outer_data = Vec::with_capacity(64 + inner_hash.len());
    outer_data.extend_from_slice(&outer);
    outer_data.extend_from_slice(&inner_hash);
    sha1(&outer_data)
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xefcdab89u32;
    let mut h2 = 0x98badcfeu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xc3d2e1f0u32;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let j = i * 4;
            *word = u32::from_be_bytes([chunk[j], chunk[j + 1], chunk[j + 2], chunk[j + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_escape() {
        assert!(clean_entry_name("../secret").is_err());
        assert!(clean_entry_name("/secret").is_err());
        assert!(clean_entry_name("work/github").is_ok());
    }

    #[test]
    fn decodes_base32() {
        assert_eq!(
            base32_decode("JBSWY3DPEHPK3PXP").unwrap(),
            b"Hello!\xde\xad\xbe\xef"
        );
    }

    #[test]
    fn sha1_known_vector() {
        let digest = sha1(b"abc");
        let hex = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn converts_pass_entry() {
        let entry = convert_pass_entry(
            "hunter2\nlogin: octo\nurl: https://github.com\notpauth://totp/GitHub?secret=JBSWY3DPEHPK3PXP&issuer=GitHub\n",
        )
        .unwrap();
        assert_eq!(entry.get("password").unwrap(), "hunter2");
        assert_eq!(entry.get("username").unwrap(), "octo");
        assert_eq!(entry.get("otp").unwrap(), "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn shell_snippet_supports_fish() {
        let snippet = shell_snippet("fish", Path::new("/usr/local/bin")).unwrap();
        assert!(snippet.contains("fish_add_path \"/usr/local/bin\""));
    }

    #[test]
    fn upserts_shell_block() {
        let original = "before\n# >>> nibpass >>>\nold\n# <<< nibpass <<<\nafter\n";
        let updated =
            upsert_managed_block(original, "\n# >>> nibpass >>>\nnew\n# <<< nibpass <<<\n");
        assert_eq!(
            updated,
            "before\n# >>> nibpass >>>\nnew\n# <<< nibpass <<<\nafter\n"
        );
    }

    #[test]
    fn recognizes_2fa_aliases() {
        assert!(is_2fa_alias("2fa"));
        assert!(is_2fa_alias("otp"));
        assert!(is_2fa_alias("totp"));
        assert!(!is_2fa_alias("password"));
    }

    #[test]
    fn normalizes_2fa_secret() {
        let secret = normalize_2fa_secret("jbsw y3dp-ehpk3pxp").unwrap();
        assert_eq!(secret, "JBSWY3DPEHPK3PXP");

        let from_url =
            normalize_2fa_secret("otpauth://totp/GitHub?secret=jbswy3dpehpk3pxp&issuer=GitHub")
                .unwrap();
        assert_eq!(from_url, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn native_crypto_round_trip() {
        let salt = [7u8; 16];
        let nonce = [9u8; 24];
        let device_key = [5u8; DEVICE_KEY_LEN];
        let key = derive_master_key(b"correct horse battery staple", &device_key, &salt).unwrap();
        let ciphertext = encrypt_bytes(&key, &nonce, b"test", b"secret").unwrap();
        let plain = decrypt_bytes(&key, &nonce, b"test", &ciphertext).unwrap();
        assert_eq!(plain, b"secret");
    }

    #[test]
    fn encrypted_entry_file_round_trip() {
        let nonce = [3u8; 24];
        let ciphertext = vec![1, 2, 3, 4];
        let encoded = encode_entry_file(&nonce, &ciphertext);
        let (decoded_nonce, decoded_ciphertext) =
            decode_encrypted_file(&encoded, ENTRY_MAGIC).unwrap();
        assert_eq!(decoded_nonce, nonce);
        assert_eq!(decoded_ciphertext, ciphertext);
    }

    #[test]
    fn parses_quoted_csv() {
        let rows =
            parse_csv("name,url,notes\nGitHub,https://github.com,\"line 1, line 2\"\n").unwrap();
        assert_eq!(rows[1][0], "GitHub");
        assert_eq!(rows[1][2], "line 1, line 2");
    }

    #[test]
    fn maps_bitwarden_csv_record() {
        let rows = parse_csv(
            "folder,name,login_uri,login_username,login_password,login_totp,notes\nwork,GitHub,https://github.com,octo,hunter2,JBSWY3DPEHPK3PXP,hello\n",
        )
        .unwrap();
        let headers = rows[0]
            .iter()
            .map(|header| normalize_header(header))
            .collect::<Vec<_>>();
        let record = csv_record(&headers, &rows[1]);
        let entry = entry_from_csv_record(&record).unwrap();
        assert_eq!(account_name_from_record(&record), "work/github");
        assert_eq!(entry.get("password").unwrap(), "hunter2");
        assert_eq!(entry.get("username").unwrap(), "octo");
        assert_eq!(entry.get("otp").unwrap(), "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn maps_browser_csv_record_without_2fa() {
        let rows =
            parse_csv("name,url,username,password\nGitHub,https://github.com,octo,hunter2\n")
                .unwrap();
        let headers = rows[0]
            .iter()
            .map(|header| normalize_header(header))
            .collect::<Vec<_>>();
        let record = csv_record(&headers, &rows[1]);
        let entry = entry_from_csv_record(&record).unwrap();
        assert_eq!(entry.get("password").unwrap(), "hunter2");
        assert!(!entry.contains_key("otp"));
    }

    #[test]
    fn parses_agent_ttl_modes() {
        assert_eq!(
            parse_agent_ttl(&[]).unwrap().as_secs(),
            SESSION_AGENT_TTL_SECONDS
        );
        assert_eq!(
            parse_agent_ttl(&["--session".to_string()])
                .unwrap()
                .as_secs(),
            SESSION_AGENT_TTL_SECONDS
        );
        assert_eq!(
            parse_agent_ttl(&["--ttl".to_string(), "30".to_string()])
                .unwrap()
                .as_secs(),
            30
        );
    }

    #[test]
    fn recovery_file_round_trip() {
        let key = [42u8; DEVICE_KEY_LEN];
        let encoded = encode_recovery_file(&key);
        assert_eq!(decode_recovery_file(&encoded).unwrap(), key);
        assert_eq!(decode_recovery_file(&hex_encode(&key)).unwrap(), key);
    }
}
