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

## Icon

The app icon lives at:

```text
assets/icons/hicolor/scalable/apps/nibnotes.svg
```

GTK uses this icon during development. Platform packages should derive their `.icns`, `.ico`, and PNG icon sizes from this SVG.

## Dotfile Configuration

NibNotes creates editable user config files on first launch.

Linux and macOS:

```text
~/.config/nibnotes/config.json
~/.config/nibnotes/keys.json
~/.config/nibnotes/theme.css
```

Windows:

```text
%APPDATA%\NibNotes\config.json
%APPDATA%\NibNotes\keys.json
%APPDATA%\NibNotes\theme.css
```

`config.json` controls defaults:

```json
{
  "theme": "gruvbox",
  "font_family": "monospace",
  "font_size": 14,
  "decorated": false,
  "window_width": 720,
  "window_height": 760,
  "notes_dir": null
}
```

`notes_dir` can point at any synced folder, such as MEGA, Dropbox, Nextcloud, iCloud Drive, or OneDrive:

```json
{
  "notes_dir": "/Users/you/MEGA/Notes/NibNotes"
}
```

If `notes_dir` is `null`, NibNotes uses the last folder chosen inside the app, or falls back to `~/Documents/NibNotes`.

Built-in theme names:

```text
gruvbox
catppuccin
custom
```

Set `"theme": "custom"` to load `theme.css`.

`keys.json` controls rebindable actions:

```json
{
  "new_note": "Primary+N",
  "quick_open": "Primary+O",
  "choose_notes_dir": "Primary+Shift+O",
  "save": "Primary+S",
  "save_quit": "Primary+Q",
  "show_help": "Primary+M",
  "insert_checkbox": "Primary+T",
  "toggle_checkbox": "Primary+Enter",
  "increase_font": "Primary+Plus",
  "decrease_font": "Primary+Minus",
  "reset_font": "Primary+0",
  "trash_note": "Primary+Shift+Delete"
}
```

`Primary` means `Cmd` on macOS and `Ctrl` on Linux/Windows. `Cmd`, `Command`, `Ctrl`, and `CmdOrCtrl` are also accepted in custom bindings.

`theme.css` is regular GTK CSS. Edit it to make custom themes. It is only active when `config.json` uses `"theme": "custom"`.

NibNotes also creates helper files in the same config folder:

```text
config.synced-folder.example.json
themes.txt
synced-folders.txt
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
- `Primary+Shift+Delete` move note to trash

## Markdown Display

NibNotes keeps real Markdown on disk, but visually hides common Markdown markers while editing. For example, `# Heading`, `**bold**`, `~~done~~`, `[site](https://example.com)`, and `<u>underlined</u>` display closer to rendered text in the editor while saving the original Markdown source.

Underline is supported with either:

```markdown
<u>underlined</u>
++underlined++
```
