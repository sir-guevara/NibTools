# NibNotes

NibNotes is a fast, local-first Markdown note app built with Rust, GTK4, and GtkSourceView.

This is the native desktop port of the original Linux GroovyNote prototype.

## Features

- Single-window distraction-free editor
- Markdown syntax highlighting through GtkSourceView
- Rendered-in-editor Markdown markers for headings, emphasis, links, quotes, code, and checkboxes
- Plain `.md` files
- User-chosen notes directory
- Autosave
- Filename slugged from the first meaningful line
- Quick Open search
- Keyboard shortcut help
- Checkbox insert/toggle
- Links, underline, strikethrough, quotes, inline code, and fenced code block styling
- Checked items shown with strikethrough styling
- Built-in Gruvbox and Catppuccin themes
- Adjustable editor font size
- Dotfile configuration for fonts, themes, and keybindings
- Last-opened note restoration

## Native Dependencies

### macOS

```bash
brew install gtk4 gtksourceview5
```

If `pkg-config` cannot find GTK after installing with Homebrew, export the pkg-config path:

```bash
export PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig:$(brew --prefix)/share/pkgconfig"
```

### Fedora

```bash
sudo dnf install gtk4-devel gtksourceview5-devel
```

### Ubuntu/Debian

```bash
sudo apt install libgtk-4-dev libgtksourceview-5-dev
```

## Run

```bash
cargo run
```

## Build Packages

macOS local build:

```bash
cargo build --release
cargo bundle --release
```

Outputs:

```text
target/release/nibnotes
target/release/bundle/osx/NibNotes.app
target/release/bundle/dmg/NibNotes.dmg
```

All-platform release builds are handled by GitHub Actions:

```text
../.github/workflows/release.yml
```

Run the workflow manually, or push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds:

- macOS `.dmg` and `.app.zip`
- Linux `tar.gz`
- Windows `.zip` containing `nibnotes.exe` and GTK runtime DLLs

## Icon

The app icon lives at:

```text
assets/icons/hicolor/1024x1024/apps/nibnotes.png
```

GTK uses this icon during development. Platform packages should derive their `.icns`, `.ico`, and smaller PNG icon sizes from this 1024x1024 PNG.

## Dotfile Configuration

NibNotes creates one editable user config file on first launch.

Linux and macOS:

```text
~/.config/nibnotes/config.toml
```

Windows:

```text
%APPDATA%\NibNotes\config.toml
```

`config.toml` controls app defaults, keybindings, synced note path, and custom theme CSS:

```toml
[app]
theme = "gruvbox"
font_family = "sans-serif"
code_font_family = "monospace"
font_size = 14
decorated = false
window_width = 720
window_height = 760
show_empty_hint = true
# notes_dir = "/Users/you/MEGA/Notes/NibNotes"

[colors]
# Optional overrides. Leave unset to use the selected theme defaults.
# text_color = "#ebdbb2"
# h1 = "#fabd2f"
# h2 = "#fe8019"
# h3 = "#83a598"
# link = "#83a598"
# code_bg = "#3c3836"
# code_keyword = "#fb4934"

[keys]
new_note = "Primary+N"
quick_open = "Primary+O"
choose_notes_dir = "Primary+Shift+O"
save = "Primary+S"
save_quit = "Primary+Q"
show_help = "Primary+M"
insert_checkbox = "Primary+T"
toggle_checkbox = "Primary+Enter"
increase_font = "Primary+Plus"
decrease_font = "Primary+Minus"
reset_font = "Primary+0"
trash_note = "Primary+Shift+D"
```

`notes_dir` can point at any synced folder, such as MEGA, Dropbox, Nextcloud, iCloud Drive, or OneDrive:

```toml
[app]
notes_dir = "/Users/you/MEGA/Notes/NibNotes"
```

If `notes_dir` is `null`, NibNotes uses the last folder chosen inside the app, or falls back to `~/Documents/NibNotes`.

Built-in theme names:

```text
gruvbox
catppuccin
custom
```

Set `theme = "custom"` and put GTK CSS in `custom_css`:

```toml
[app]
theme = "custom"
custom_css = """
window.nibnotes { background: #1d2021; }
textview, textview text { background: #1d2021; color: #ebdbb2; }
"""
```

Code uses `code_font_family`. `font_family` changes normal note text, headings, bold, italic, lists, quotes, and the app panels.

`Primary` means `Cmd` on macOS and `Ctrl` on Linux/Windows. `Cmd`, `Command`, `Ctrl`, and `CmdOrCtrl` are also accepted in custom bindings.

NibNotes also creates one example file in the same config folder:

```text
config.example.toml
```

## Shortcuts

Use `Cmd` on macOS and `Ctrl` on Linux/Windows.

- `Primary+N` new note
- `Primary+O` quick open
- `Primary+Shift+O` choose notes folder
- `Primary+S` save
- `Primary+Q` save and quit
- `Primary+M` show keyboard help
- `Primary+T` insert checkbox
- `Primary+Enter` toggle checkbox
- `Primary++` increase font size
- `Primary+-` decrease font size
- `Primary+0` reset font size
- `Primary+Shift+D` move note to trash

## Markdown Display

NibNotes keeps real Markdown on disk, but visually hides common Markdown markers while editing. For example, `# Heading`, `**bold**`, `~~done~~`, `[site](https://example.com)`, and `<u>underlined</u>` display closer to rendered text in the editor while saving the original Markdown source.

Underline is supported with either:

```markdown
<u>underlined</u>
++underlined++
```
