# NibPass

NibPass is a small terminal-first password manager for people who want local encrypted files, plain git history, and 2FA support without a service account.

The vault design keeps normal use native and local:

- `nibpass init` creates one master-password-protected vault
- the master password and local device key are stretched with Argon2id
- a random vault key encrypts account entries with XChaCha20-Poly1305
- encrypted entries are regular `.nib` files
- git is ordinary git inside the store
- 2FA codes are generated locally from each account entry
- copied secrets are cleared from the clipboard after 30 seconds when possible

## Requirements

- no crypto service or cloud account
- `git` for history support
- one clipboard command for `-c`: `pbcopy`, `wl-copy`, `xclip`, `xsel`, or Windows `clip`
- `gpg` only if importing an existing Linux `pass` store

## Install

Install the latest macOS or Linux build from GitHub Releases:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash
```

`latest` resolves to the newest GitHub Release whose tag starts with `nibpass-v`.

Install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash -s -- --version nibpass-v0.1.0
```

By default the installer puts `nibpass` in `~/.local/bin`. You can change that:

```bash
curl -fsSL https://raw.githubusercontent.com/nibtools/nibtools/main/nibpass/install.sh | bash -s -- --bin-dir /usr/local/bin
```

If the repository name changes, set `NIBPASS_REPO=owner/repo` before running the installer.

## Quick Start

```bash
nibpass init
nibpass add github --generate
nibpass github add 2fa
nibpass ls
nibpass -c github
nibpass 2fa -c github
```

Session unlock:

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
nibpass agent
nibpass ls
```

## Build

```bash
cargo build --release
```

## GitHub Releases

NibPass releases are published from tags that start with `nibpass-v`:

```bash
git tag nibpass-v0.1.0
git push origin nibpass-v0.1.0
```

The release workflow builds:

```text
nibpass-macos-universal.tar.gz
nibpass-linux-x86_64.tar.gz
```

Each tarball includes the `nibpass` binary, README, installer script, and shell completions for zsh, bash, and fish.

## Shell Setup

After installing the binary, add NibPass to the current shell startup file:

```bash
nibpass install-shell
```

That detects `$SHELL` and updates one of:

```text
~/.zshrc
~/.bashrc
~/.config/fish/config.fish
```

You can be explicit:

```bash
nibpass install-shell --shell zsh
nibpass install-shell --shell bash
nibpass install-shell --shell fish
```

Installers can print the snippet instead of writing files:

```bash
nibpass shellenv --shell fish --bin-dir /usr/local/bin
```

## Store Location

Default store:

```text
~/.local/share/nibpass
```

Override per command:

```bash
NIBPASS_STORE=/path/to/store nibpass ls
```

## Commands

```bash
nibpass init
# Create master password:
# Confirm master password:

nibpass add
nibpass add github --generate
nibpass add github --dialog
nibpass add github --username octo --url https://github.com --2fa JBSWY3DPEHPK3PXP
nibpass set github username octo@example.com
nibpass github set url https://github.com
nibpass edit github
nibpass find git
nibpass audit
nibpass gen
nibpass gen --words 5
nibpass github add 2fa
nibpass github add 2fa JBSWY3DPEHPK3PXP
nibpass show github
nibpass -c github
nibpass 2fa github
nibpass 2fa -c github
nibpass ls
nibpass rm github
nibpass agent
nibpass agent --ttl 900
nibpass agent status
nibpass agent lock
nibpass recovery export ~/nibpass-recovery.key
nibpass recovery import ~/nibpass-recovery.key
nibpass recovery verify ~/nibpass-recovery.key
nibpass recovery status
nibpass sync
nibpass sync init <private-git-repo>
nibpass sync status
nibpass rotate master
nibpass rotate device
nibpass completion zsh
nibpass browser install chrome
nibpass browser install firefox
nibpass browser manifest chrome
nibpass gui
nibpass install-shell
```

`nibpass add` prompts for account name, password, username, URL, and notes. It does not prompt for 2FA because not every account has it. `nibpass <account> add 2fa` adds or replaces the 2FA secret for that account when needed. It accepts a raw Base32 secret or a full `otpauth://` URL. `otp` and `totp` are accepted as aliases, but NibPass documentation and prompts use `2fa`.

Git helper:

```bash
nibpass git status
nibpass git log
nibpass git undo
nibpass sync
```

NibPass initializes git automatically and commits successful add, 2FA, remove, and import operations. `nibpass git ...` is still available for status, log, remote setup, sync, and revert workflows.

## Backup And Recovery

NibPass intentionally separates backup pieces:

```text
private git repo:
  encrypted vault files

recovery key file:
  local device key

your memory:
  master password
```

Set up backup:

```bash
nibpass git remote add origin <private-git-repo>
nibpass sync
nibpass recovery export ~/nibpass-recovery.key
nibpass recovery status
```

Restore on another computer:

```bash
git clone <private-git-repo> ~/.local/share/nibpass
nibpass recovery import ~/nibpass-recovery.key
nibpass agent
nibpass ls
```

`nibpass sync` runs `git pull --rebase` and `git push`. Keep the recovery key somewhere separate from the git repo.

## Encryption

NibPass uses one master password for the vault:

```text
something you know: master password
something you have: local device key
  -> Argon2id
  -> decrypts random vault key
  -> vault key encrypts/decrypts each account entry
```

Files on disk:

```text
~/.local/share/nibpass/.nibpass/vault
~/.local/share/nibpass/github.nib
~/.config/nibpass/device-<vault-id>.key
```

Git tracks only encrypted vault data and encrypted account files. The device key is stored outside the git-tracked vault by default.

Unlock options:

```bash
nibpass agent              # unlock for the shell/work session
nibpass agent --ttl 900    # unlock for 15 minutes
nibpass --no-agent -c github
```

While the agent is running, decrypting commands use its local authenticated loopback agent instead of prompting for the master password. The vault key stays in the agent process memory and expires automatically. `--ttl` gives a shorter timed unlock. `--no-agent` forces a command to ask for the master password even when an agent is running. Use `nibpass agent lock` to end the session early.

Biometrics are not implemented directly in the CLI yet. The intended path is OS-backed biometric unlock through the future native GUI/keychain integration, where available.

Import an existing Linux `pass` store:

```bash
nibpass import pass ~/.password-store
```

`import pass` copies `.gpg` files into the NibPass store and preserves directory structure.
It decrypts each `pass` entry, converts common fields such as `login`, `url`, and `otpauth://` 2FA secrets, then re-encrypts it in the NibPass format.

Import CSV exports:

```bash
nibpass import csv ~/Downloads/passwords.csv
nibpass import csv ~/Downloads/passwords.csv --format bitwarden
nibpass import bitwarden ~/Downloads/bitwarden.csv
nibpass import chrome ~/Downloads/chrome-passwords.csv
nibpass import apple ~/Downloads/passwords.csv
nibpass import firefox ~/Downloads/firefox-passwords.csv
nibpass import 1password ~/Downloads/1password.csv
nibpass import xpass ~/Downloads/xpass.csv
nibpass import bitwarden ~/Downloads/bitwarden.csv --dry-run
nibpass import bitwarden ~/Downloads/bitwarden.csv --delete-after
```

CSV import maps common fields such as name/title, URL, username, password, notes, and 2FA/TOTP columns. Imported entries are written as native encrypted `.nib` files.

Export requires an explicit plaintext flag:

```bash
nibpass export csv ~/nibpass-export.csv --plaintext
```

Delete or re-encrypt plaintext exports immediately.

Browser autofill:

```bash
nibpass browser install chrome
nibpass browser install firefox
nibpass browser manifest chrome
nibpass browser host
```

`browser install` writes a native messaging host wrapper and manifest. The browser extension side can ask the host for account lists or specific fields through native messaging. The host expects the NibPass agent to be unlocked for low-friction use.

GTK GUI:

```bash
cargo run --features gui -- gui
```

The GUI is optional so the default terminal binary stays small. It provides account search plus copy password / copy 2FA actions using the same vault and agent.

## Entry Format

Entries decrypt to a tiny line-based format:

```text
password: correct horse battery staple
username: octo
url: https://github.com
2fa: JBSWY3DPEHPK3PXP
notes: recovery codes stored offline
```

The first phase focuses on a fast CLI. Planned later pieces are a small browser autofill helper and a GTK Rust GUI that uses the same store.
