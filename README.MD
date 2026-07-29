# NibTools

Terminal-first, self-managed tools written in Rust.

The main project here is NibPass: a small password manager for people who want local encrypted files, git history, 2FA codes, and simple command-line workflows without a hosted account.

## Install NibPass

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash
nibpass install-shell
```

Install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash -s -- --version nibpass-v0.1.0
```

Install somewhere else:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash -s -- --bin-dir /usr/local/bin
```

Release downloads are published on tags named `nibpass-v*` and include:

```text
nibpass-macos-universal.tar.gz
nibpass-linux-x86_64.tar.gz
```

## NibPass Workflow

Create a vault:

```bash
nibpass init
```

Add accounts:

```bash
nibpass add github --generate
nibpass add github --username octo --url https://github.com
nibpass github add 2fa
```

Use secrets:

```bash
nibpass ls
nibpass show github
nibpass -c github
nibpass 2fa -c github
```

Unlock once for the session:

```bash
nibpass agent
nibpass agent --ttl 900
nibpass agent lock
```

Backup and restore:

```bash
nibpass sync init <private-git-repo>
nibpass recovery export ~/nibpass-recovery.key

git clone <private-git-repo> ~/.local/share/nibpass
nibpass recovery import ~/nibpass-recovery.key
```

Import from other stores:

```bash
nibpass import pass ~/.password-store
nibpass import bitwarden ~/Downloads/bitwarden.csv
nibpass import chrome ~/Downloads/chrome-passwords.csv
nibpass import apple ~/Downloads/passwords.csv
nibpass import firefox ~/Downloads/firefox-passwords.csv
nibpass import 1password ~/Downloads/1password.csv
```

## Security Model

NibPass uses:

- something you know: your master password
- something you have: a local device key stored outside the git vault
- Argon2id to unlock a random vault key
- XChaCha20-Poly1305 to encrypt account entries
- git for encrypted history and sync

Git stores encrypted vault files only. The recovery key is backed up separately, and the master password stays in your head.

## Build From Source

```bash
cd nibpass
cargo build --release
```

Optional GTK GUI:

```bash
cargo run --features gui -- gui
```

## Release NibPass

```bash
git tag nibpass-v0.1.0
git push origin nibpass-v0.1.0
```

The GitHub Actions workflow builds macOS and Linux tarballs and attaches them to the release.

## Other Tools

- NibNotes: minimal notes
- NibClip: lightweight clipboard manager
- NibMail: planned minimal email client
