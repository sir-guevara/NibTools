use gio::prelude::*;
use gtk::gdk;
use gtk::pango;
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use sourceview::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_ID: &str = "dev.nibtools.NibNotes";
const ICON_NAME: &str = "nibnotes";
const ICON_SEARCH_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons");
const UNCHECKED: &str = "☐ ";
const CHECKED: &str = "☑ ";

const GRUVBOX_THEME_CSS: &str = r#"
window.nibnotes {
  background: #1d2021;
}

.editor-pane,
scrolledwindow,
textview,
textview text {
  background: #1d2021;
}

textview {
  color: #ebdbb2;
  caret-color: #fe8019;
  font-family: monospace;
  font-size: 14px;
}

textview text selection {
  background: #458588;
  color: #fbf1c7;
}

scrollbar slider {
  min-width: 5px;
  min-height: 34px;
  background: #504945;
}

.panel {
  min-width: 420px;
  padding: 20px 22px;
  border-radius: 8px;
  background: #282828;
  border: 1px solid #504945;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.55);
}

.title {
  color: #fabd2f;
  font-size: 18px;
  font-weight: 700;
}

.body {
  color: #ebdbb2;
  font-family: monospace;
  font-size: 12px;
}

.hint {
  color: #928374;
  font-size: 10px;
}

entry {
  background: #1d2021;
  color: #ebdbb2;
  border: 1px solid #504945;
  border-radius: 6px;
  padding: 8px 10px;
}

listbox {
  background: #282828;
}

listbox row {
  color: #ebdbb2;
  padding: 8px 10px;
}

listbox row:selected {
  background: #458588;
  color: #fbf1c7;
}

.quick-panel {
  min-width: 340px;
  padding: 14px 16px;
}

.quick-entry {
  min-height: 28px;
  padding: 5px 8px;
}

.quick-list row {
  padding: 5px 8px;
}
"#;

const CATPPUCCIN_THEME_CSS: &str = r#"
window.nibnotes {
  background: #1e1e2e;
}

.editor-pane,
scrolledwindow,
textview,
textview text {
  background: #1e1e2e;
}

textview {
  color: #cdd6f4;
  caret-color: #f5c2e7;
  font-family: monospace;
  font-size: 14px;
}

textview text selection {
  background: #45475a;
  color: #f5e0dc;
}

scrollbar slider {
  min-width: 5px;
  min-height: 34px;
  background: #585b70;
}

.panel {
  min-width: 420px;
  padding: 20px 22px;
  border-radius: 8px;
  background: #181825;
  border: 1px solid #45475a;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.55);
}

.title {
  color: #f9e2af;
  font-size: 18px;
  font-weight: 700;
}

.body {
  color: #cdd6f4;
  font-family: monospace;
  font-size: 12px;
}

.hint {
  color: #7f849c;
  font-size: 10px;
}

entry {
  background: #11111b;
  color: #cdd6f4;
  border: 1px solid #45475a;
  border-radius: 6px;
  padding: 8px 10px;
}

listbox {
  background: #181825;
}

listbox row {
  color: #cdd6f4;
  padding: 8px 10px;
}

listbox row:selected {
  background: #313244;
  color: #f5e0dc;
}

.quick-panel {
  min-width: 340px;
  padding: 14px 16px;
}

.quick-entry {
  min-height: 28px;
  padding: 5px 8px;
}

.quick-list row {
  padding: 5px 8px;
}
"#;

const CUSTOM_THEME_TEMPLATE_CSS: &str = r#"/*
NibNotes custom theme.

Set `"theme": "custom"` in config.json to use this file.
This is GTK CSS, so you can override any of the app selectors below.
*/

window.nibnotes {
  background: #1d2021;
}

textview,
textview text {
  background: #1d2021;
  color: #ebdbb2;
  caret-color: #fe8019;
}

textview text selection {
  background: #458588;
  color: #fbf1c7;
}

.panel {
  background: #282828;
  border: 1px solid #504945;
}

.title {
  color: #fabd2f;
}

.body {
  color: #ebdbb2;
}

.hint {
  color: #928374;
}
"#;

const DEFAULT_CONFIG_JSON: &str = r#"{
  "theme": "gruvbox",
  "font_family": "monospace",
  "font_size": 14,
  "decorated": false,
  "window_width": 720,
  "window_height": 760,
  "notes_dir": null
}
"#;

const EXAMPLE_SYNC_CONFIG_JSON: &str = r#"{
  "theme": "catppuccin",
  "font_family": "monospace",
  "font_size": 14,
  "decorated": false,
  "window_width": 720,
  "window_height": 760,
  "notes_dir": "/Users/you/MEGA/Notes/NibNotes"
}
"#;

const SYNC_FOLDERS_TEXT: &str = r#"NibNotes stores notes as plain Markdown files.

Set "notes_dir" in config.json to any synced folder, for example:

macOS:
  "/Users/you/MEGA/Notes/NibNotes"
  "/Users/you/Library/CloudStorage/Dropbox/Notes/NibNotes"
  "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/Notes/NibNotes"

Linux:
  "/home/you/MEGA/Notes/NibNotes"
  "/home/you/Dropbox/Notes/NibNotes"
  "/home/you/Nextcloud/Notes/NibNotes"

Windows:
  "C:\\Users\\you\\OneDrive\\Notes\\NibNotes"
  "C:\\Users\\you\\Dropbox\\Notes\\NibNotes"
"#;

const THEME_HELP_TEXT: &str = r#"NibNotes built-in themes:

  gruvbox
  catppuccin
  custom

Set "theme" in config.json.

Use "custom" to load theme.css from this folder.
"#;

const DEFAULT_KEYS_JSON: &str = r#"{
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
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    notes_dir: PathBuf,
    last_note: Option<PathBuf>,
    font_size: i32,
}

impl Settings {
    fn load() -> Self {
        let path = settings_path();
        let loaded = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok());

        loaded.unwrap_or_else(|| Settings {
            notes_dir: default_notes_dir(),
            last_note: None,
            font_size: 14,
        })
    }

    fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct DotConfig {
    theme: String,
    font_family: String,
    font_size: i32,
    decorated: bool,
    window_width: i32,
    window_height: i32,
    notes_dir: Option<PathBuf>,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            theme: "gruvbox".to_string(),
            font_family: "monospace".to_string(),
            font_size: 14,
            decorated: false,
            window_width: 720,
            window_height: 760,
            notes_dir: None,
        }
    }
}

impl DotConfig {
    fn load() -> Self {
        ensure_dotfiles();
        fs::read_to_string(config_path())
            .ok()
            .and_then(|text| serde_json::from_str::<DotConfig>(&text).ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Action {
    NewNote,
    QuickOpen,
    ChooseNotesDir,
    Save,
    SaveQuit,
    ShowHelp,
    InsertCheckbox,
    ToggleCheckbox,
    IncreaseFont,
    DecreaseFont,
    ResetFont,
    TrashNote,
}

#[derive(Debug, Clone)]
struct KeyBinding {
    ctrl: bool,
    cmd: bool,
    shift: bool,
    alt: bool,
    key: String,
}

impl KeyBinding {
    fn parse(input: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut cmd = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = None;

        for part in input.split('+') {
            let normalized = part.trim().to_lowercase();
            match normalized.as_str() {
                "ctrl" | "control" | "cmdorctrl" | "primary" => {
                    if cfg!(target_os = "macos") {
                        cmd = true;
                    } else {
                        ctrl = true;
                    }
                }
                "cmd" | "command" | "meta" | "super" => cmd = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "" => {}
                value => key = Some(value.to_string()),
            }
        }

        key.map(|key| Self {
            ctrl,
            cmd,
            shift,
            alt,
            key,
        })
    }

    fn matches(&self, key: gdk::Key, modifier: gdk::ModifierType) -> bool {
        self.ctrl == modifier.contains(gdk::ModifierType::CONTROL_MASK)
            && self.cmd == has_command_modifier(modifier)
            && self.shift == modifier.contains(gdk::ModifierType::SHIFT_MASK)
            && self.alt == modifier.contains(gdk::ModifierType::ALT_MASK)
            && self.key == normalize_key(key)
    }
}

#[derive(Debug, Clone)]
struct KeyMap {
    bindings: Vec<(Action, KeyBinding)>,
}

impl KeyMap {
    fn load() -> Self {
        ensure_dotfiles();
        let raw = fs::read_to_string(keys_path())
            .ok()
            .and_then(|text| serde_json::from_str::<HashMap<String, String>>(&text).ok())
            .unwrap_or_default();

        let bindings = default_key_specs()
            .into_iter()
            .filter_map(|(name, action, fallback)| {
                let spec = raw.get(name).map(String::as_str).unwrap_or(fallback);
                KeyBinding::parse(spec).map(|binding| (action, binding))
            })
            .collect();

        Self { bindings }
    }

    fn action_for(&self, key: gdk::Key, modifier: gdk::ModifierType) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(_, binding)| binding.matches(key, modifier))
            .map(|(action, _)| action.clone())
    }
}

struct AppState {
    window: gtk::ApplicationWindow,
    buffer: sourceview::Buffer,
    view: sourceview::View,
    help_panel: gtk::Box,
    quick_panel: gtk::Box,
    quick_entry: gtk::SearchEntry,
    quick_list: gtk::ListBox,
    font_provider: gtk::CssProvider,
    settings: RefCell<Settings>,
    dot_config: DotConfig,
    keymap: KeyMap,
    current_note: RefCell<Option<PathBuf>>,
    last_saved: RefCell<String>,
    disk_mtime: Cell<u128>,
    loading: Cell<bool>,
    save_timer: RefCell<Option<glib::SourceId>>,
    style_timer: RefCell<Option<glib::SourceId>>,
    render_tags: RenderTags,
}

#[derive(Clone)]
struct RenderTags {
    marker: gtk::TextTag,
    heading1: gtk::TextTag,
    heading2: gtk::TextTag,
    heading3: gtk::TextTag,
    bold: gtk::TextTag,
    italic: gtk::TextTag,
    underline: gtk::TextTag,
    strike: gtk::TextTag,
    link: gtk::TextTag,
    quote: gtk::TextTag,
    inline_code: gtk::TextTag,
    code_block: gtk::TextTag,
    code_keyword: gtk::TextTag,
    code_string: gtk::TextTag,
    code_comment: gtk::TextTag,
    code_number: gtk::TextTag,
    code_function: gtk::TextTag,
    completed: gtk::TextTag,
    checkbox: gtk::TextTag,
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn register_app_icon() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_search_path(ICON_SEARCH_PATH);
    gtk::Window::set_default_icon_name(ICON_NAME);
}

fn build_render_tags(buffer: &sourceview::Buffer) -> RenderTags {
    let tags = RenderTags {
        marker: gtk::TextTag::builder()
            .name("md-marker")
            .invisible(true)
            .build(),
        heading1: gtk::TextTag::builder()
            .name("heading1")
            .foreground("#fabd2f")
            .weight(700)
            .build(),
        heading2: gtk::TextTag::builder()
            .name("heading2")
            .foreground("#fe8019")
            .weight(700)
            .build(),
        heading3: gtk::TextTag::builder()
            .name("heading3")
            .foreground("#83a598")
            .weight(700)
            .build(),
        bold: gtk::TextTag::builder().name("bold").weight(700).build(),
        italic: gtk::TextTag::builder()
            .name("italic")
            .style(pango::Style::Italic)
            .build(),
        underline: gtk::TextTag::builder()
            .name("underline")
            .underline(pango::Underline::Single)
            .build(),
        strike: gtk::TextTag::builder()
            .name("strike")
            .strikethrough(true)
            .build(),
        link: gtk::TextTag::builder()
            .name("link")
            .foreground("#83a598")
            .underline(pango::Underline::Single)
            .build(),
        quote: gtk::TextTag::builder()
            .name("quote")
            .style(pango::Style::Italic)
            .left_margin(18)
            .indent(0)
            .build(),
        inline_code: gtk::TextTag::builder()
            .name("inline-code")
            .foreground("#8ec07c")
            .background("#282828")
            .build(),
        code_block: gtk::TextTag::builder()
            .name("code-block")
            .foreground("#ebdbb2")
            .background("#282828")
            .build(),
        code_keyword: gtk::TextTag::builder()
            .name("code-keyword")
            .foreground("#fb4934")
            .weight(700)
            .build(),
        code_string: gtk::TextTag::builder()
            .name("code-string")
            .foreground("#b8bb26")
            .build(),
        code_comment: gtk::TextTag::builder()
            .name("code-comment")
            .foreground("#928374")
            .style(pango::Style::Italic)
            .build(),
        code_number: gtk::TextTag::builder()
            .name("code-number")
            .foreground("#d3869b")
            .build(),
        code_function: gtk::TextTag::builder()
            .name("code-function")
            .foreground("#83a598")
            .build(),
        completed: gtk::TextTag::builder()
            .name("completed-task")
            .strikethrough(true)
            .build(),
        checkbox: gtk::TextTag::builder()
            .name("checkbox")
            .foreground("#fabd2f")
            .weight(700)
            .scale(1.22)
            .build(),
    };

    for tag in tags.all() {
        buffer.tag_table().add(tag);
    }

    tags
}

impl RenderTags {
    fn all(&self) -> [&gtk::TextTag; 19] {
        [
            &self.marker,
            &self.heading1,
            &self.heading2,
            &self.heading3,
            &self.bold,
            &self.italic,
            &self.underline,
            &self.strike,
            &self.link,
            &self.quote,
            &self.inline_code,
            &self.code_block,
            &self.code_keyword,
            &self.code_string,
            &self.code_comment,
            &self.code_number,
            &self.code_function,
            &self.completed,
            &self.checkbox,
        ]
    }
}

fn build_ui(app: &gtk::Application) {
    ensure_dotfiles();
    let dot_config = DotConfig::load();
    let keymap = KeyMap::load();
    register_app_icon();

    let provider = gtk::CssProvider::new();
    provider.load_from_data(&load_theme_css(&dot_config.theme));
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("GTK display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let font_provider = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("GTK display"),
        &font_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );

    let mut settings = Settings::load();
    if settings.font_size == 14 {
        settings.font_size = dot_config.font_size;
    }
    if let Some(notes_dir) = dot_config.notes_dir.clone() {
        settings.notes_dir = notes_dir;
    }
    let _ = fs::create_dir_all(&settings.notes_dir);

    let buffer = sourceview::Buffer::new(None::<&gtk::TextTagTable>);
    if let Some(language) = sourceview::LanguageManager::new().language("markdown") {
        buffer.set_language(Some(&language));
    }
    buffer.set_highlight_syntax(true);

    let render_tags = build_render_tags(&buffer);

    let view = sourceview::View::with_buffer(&buffer);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_monospace(true);
    view.set_left_margin(28);
    view.set_right_margin(28);
    view.set_top_margin(24);
    view.set_bottom_margin(28);
    view.set_pixels_below_lines(5);
    view.set_show_line_numbers(false);
    view.set_tab_width(2);

    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();
    scroller.add_css_class("editor-pane");

    let help_panel = build_help_panel();
    help_panel.set_visible(false);

    let (quick_panel, quick_entry, quick_list) = build_quick_panel();
    quick_panel.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroller));
    overlay.add_overlay(&help_panel);
    overlay.add_overlay(&quick_panel);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("NibNotes")
        .default_width(dot_config.window_width)
        .default_height(dot_config.window_height)
        .decorated(dot_config.decorated)
        .child(&overlay)
        .build();
    window.add_css_class("nibnotes");
    window.set_icon_name(Some(ICON_NAME));

    let state = Rc::new(AppState {
        window,
        buffer,
        view,
        help_panel,
        quick_panel,
        quick_entry,
        quick_list,
        font_provider,
        settings: RefCell::new(settings),
        dot_config,
        keymap,
        current_note: RefCell::new(None),
        last_saved: RefCell::new(String::new()),
        disk_mtime: Cell::new(0),
        loading: Cell::new(false),
        save_timer: RefCell::new(None),
        style_timer: RefCell::new(None),
        render_tags,
    });

    apply_font_size(&state);
    connect_handlers(&state);
    prepare_notes(&state);
    open_startup_note(&state);
    refresh_markdown_rendering(&state);

    state.window.present();
    state.view.grab_focus();

    let external_state = state.clone();
    glib::timeout_add_seconds_local(3, move || {
        check_external_change(&external_state);
        glib::ControlFlow::Continue
    });
}

fn build_help_panel() -> gtk::Box {
    let primary = primary_modifier_label();
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 9);
    panel.add_css_class("panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);

    let title = gtk::Label::builder()
        .label("NibNotes keys")
        .xalign(0.0)
        .build();
    title.add_css_class("title");
    panel.append(&title);

    let manual = gtk::Label::builder()
        .xalign(0.0)
        .yalign(0.0)
        .use_markup(true)
        .label(format!(
            "<span foreground=\"#fe8019\" weight=\"bold\">NOTES</span>\n\
{primary} O          Open note list\n\
{primary} Shift O    Choose notes folder\n\
{primary} N          New note\n\
{primary} S          Save\n\
{primary} Shift Del  Move note to trash\n\
{primary} Q          Save and close\n\n\
<span foreground=\"#fe8019\" weight=\"bold\">WRITING</span>\n\
{primary} + / -      Increase / decrease font\n\
{primary} 0          Reset font size\n\
{primary} T          Insert checkbox\n\
{primary} Enter      Toggle checkbox\n\n\
<span foreground=\"#fe8019\" weight=\"bold\">MARKDOWN</span>\n\
#  ##  ###      Headings\n\
**text**        Bold\n\
*text*          Italic\n\
`text`          Inline code\n\
```lang         Multiline code block\n\
&gt; text          Quote\n\
- [ ] / - [x]   Checklist",
        ))
        .build();
    manual.add_css_class("body");
    panel.append(&manual);

    let hint = gtk::Label::builder()
        .label(format!("{primary} M or Esc to close"))
        .xalign(0.0)
        .build();
    hint.add_css_class("hint");
    panel.append(&hint);
    panel
}

fn build_quick_panel() -> (gtk::Box, gtk::SearchEntry, gtk::ListBox) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    panel.add_css_class("panel");
    panel.add_css_class("quick-panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);

    let title = gtk::Label::builder().label("Open note").xalign(0.0).build();
    title.add_css_class("title");
    panel.append(&title);

    let entry = gtk::SearchEntry::new();
    entry.add_css_class("quick-entry");
    panel.append(&entry);

    let list = gtk::ListBox::new();
    list.add_css_class("quick-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    panel.append(&list);

    let hint = gtk::Label::builder()
        .label("Enter to open, Esc to close")
        .xalign(0.0)
        .build();
    hint.add_css_class("hint");
    panel.append(&hint);

    (panel, entry, list)
}

fn connect_handlers(state: &Rc<AppState>) {
    let changed_state = state.clone();
    state.buffer.connect_changed(move |_| {
        if changed_state.loading.get() {
            return;
        }

        if let Some(id) = changed_state.style_timer.borrow_mut().take() {
            id.remove();
        }
        let style_state = changed_state.clone();
        *changed_state.style_timer.borrow_mut() = Some(glib::timeout_add_local(
            std::time::Duration::from_millis(40),
            move || {
                refresh_markdown_rendering(&style_state);
                *style_state.style_timer.borrow_mut() = None;
                glib::ControlFlow::Break
            },
        ));

        if let Some(id) = changed_state.save_timer.borrow_mut().take() {
            id.remove();
        }
        let save_state = changed_state.clone();
        *changed_state.save_timer.borrow_mut() = Some(glib::timeout_add_local(
            std::time::Duration::from_millis(450),
            move || {
                save_note(&save_state);
                *save_state.save_timer.borrow_mut() = None;
                glib::ControlFlow::Break
            },
        ));
    });

    let keys = gtk::EventControllerKey::new();
    let key_state = state.clone();
    keys.connect_key_pressed(move |_, key, _, modifier| {
        handle_key(&key_state, key, modifier).into()
    });
    state.view.add_controller(keys);

    let quick_key = gtk::EventControllerKey::new();
    let quick_key_state = state.clone();
    quick_key.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            close_quick_open(&quick_key_state);
            return true.into();
        }
        if key == gdk::Key::Return || key == gdk::Key::KP_Enter {
            open_selected_quick_note(&quick_key_state);
            return true.into();
        }
        false.into()
    });
    state.quick_entry.add_controller(quick_key);

    let search_state = state.clone();
    state.quick_entry.connect_search_changed(move |_| {
        populate_quick_open(&search_state);
    });

    let activate_state = state.clone();
    state.quick_list.connect_row_activated(move |_, row| {
        if let Some(path) = row_path(row) {
            load_note(&activate_state, path);
            close_quick_open(&activate_state);
        }
    });
}

fn handle_key(state: &Rc<AppState>, key: gdk::Key, modifier: gdk::ModifierType) -> bool {
    if key == gdk::Key::Escape {
        if state.help_panel.is_visible() {
            state.help_panel.set_visible(false);
            return true;
        }
        if state.quick_panel.is_visible() {
            close_quick_open(state);
            return true;
        }
    }

    if let Some(action) = state.keymap.action_for(key, modifier) {
        run_action(state, action);
        return true;
    }

    if !modifier.contains(gdk::ModifierType::CONTROL_MASK)
        && !has_command_modifier(modifier)
        && !modifier.contains(gdk::ModifierType::ALT_MASK)
        && !modifier.contains(gdk::ModifierType::SHIFT_MASK)
        && (key == gdk::Key::Return || key == gdk::Key::KP_Enter)
    {
        return continue_checkbox_line(state);
    }

    false
}

fn run_action(state: &Rc<AppState>, action: Action) {
    match action {
        Action::NewNote => new_note(state),
        Action::QuickOpen => open_quick_open(state),
        Action::ChooseNotesDir => choose_notes_dir(state),
        Action::Save => save_note(state),
        Action::SaveQuit => {
            save_note(state);
            state.window.close();
        }
        Action::ShowHelp => state.help_panel.set_visible(!state.help_panel.is_visible()),
        Action::InsertCheckbox => insert_checkbox(state),
        Action::ToggleCheckbox => toggle_current_line(state),
        Action::IncreaseFont => change_font_size(state, 1),
        Action::DecreaseFont => change_font_size(state, -1),
        Action::ResetFont => {
            state.settings.borrow_mut().font_size = state.dot_config.font_size;
            apply_font_size(state);
            state.settings.borrow().save();
        }
        Action::TrashNote => trash_current_note(state),
    }
}

fn prepare_notes(state: &Rc<AppState>) {
    let dir = state.settings.borrow().notes_dir.clone();
    let _ = fs::create_dir_all(&dir);
    if notes(state).is_empty() {
        let _ = fs::write(dir.join("untitled.md"), "");
    }
}

fn open_startup_note(state: &Rc<AppState>) {
    let candidate = state.settings.borrow().last_note.clone();
    if let Some(path) = candidate {
        if path.exists() {
            load_note(state, path);
            return;
        }
    }
    if let Some(path) = notes(state).first().cloned() {
        load_note(state, path);
    }
}

fn notes(state: &Rc<AppState>) -> Vec<PathBuf> {
    let dir = state.settings.borrow().notes_dir.clone();
    let mut files = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect::<Vec<_>>();

    files.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|mtime| mtime.duration_since(UNIX_EPOCH).ok())
            .map(|duration| std::cmp::Reverse(duration.as_nanos()))
    });
    files
}

fn load_note(state: &Rc<AppState>, path: PathBuf) {
    state.loading.set(true);
    let text = fs::read_to_string(&path).unwrap_or_default();
    state.buffer.set_text(&text);
    let end = state.buffer.end_iter();
    state.buffer.place_cursor(&end);
    *state.current_note.borrow_mut() = Some(path.clone());
    *state.last_saved.borrow_mut() = text;
    state.disk_mtime.set(mtime_ns(&path));
    {
        let mut settings = state.settings.borrow_mut();
        settings.last_note = Some(path);
        settings.save();
    }
    state.loading.set(false);
    refresh_markdown_rendering(state);
}

fn save_note(state: &Rc<AppState>) {
    let Some(path) = state.current_note.borrow().clone() else {
        return;
    };
    let text = buffer_text(&state.buffer);
    let temporary = path.with_extension("tmp");
    let result = fs::File::create(&temporary)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|_| fs::rename(&temporary, &path));

    if result.is_ok() {
        let final_path = rename_from_title(state, &path, &text).unwrap_or(path);
        *state.current_note.borrow_mut() = Some(final_path.clone());
        *state.last_saved.borrow_mut() = text;
        state.disk_mtime.set(mtime_ns(&final_path));
        let mut settings = state.settings.borrow_mut();
        settings.last_note = Some(final_path);
        settings.save();
    } else {
        let _ = fs::remove_file(temporary);
    }
}

fn rename_from_title(state: &Rc<AppState>, path: &Path, text: &str) -> Option<PathBuf> {
    let slug = note_slug(text)?;
    let stem = path.file_stem()?.to_string_lossy();
    if stem == slug || stem.starts_with(&format!("{slug}-")) {
        return None;
    }

    let dir = state.settings.borrow().notes_dir.clone();
    let mut destination = dir.join(format!("{slug}.md"));
    let mut counter = 2;
    while destination.exists() {
        destination = dir.join(format!("{slug}-{counter}.md"));
        counter += 1;
    }

    fs::rename(path, &destination).ok()?;
    Some(destination)
}

fn new_note(state: &Rc<AppState>) {
    save_if_dirty(state);
    let dir = state.settings.borrow().notes_dir.clone();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let mut path = dir.join(format!("note-{stamp}.md"));
    let mut counter = 2;
    while path.exists() {
        path = dir.join(format!("note-{stamp}-{counter}.md"));
        counter += 1;
    }
    let _ = fs::write(&path, "");
    load_note(state, path);
    state.view.grab_focus();
}

fn trash_current_note(state: &Rc<AppState>) {
    let Some(path) = state.current_note.borrow().clone() else {
        return;
    };
    let dir = state.settings.borrow().notes_dir.clone();
    let trash = dir.join(".trash");
    let _ = fs::create_dir_all(&trash);
    let Some(file_name) = path.file_name() else {
        return;
    };
    let mut target = trash.join(file_name);
    if target.exists() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        target = trash.join(format!("{stamp}-{}", file_name.to_string_lossy()));
    }
    if fs::rename(&path, target).is_ok() {
        *state.current_note.borrow_mut() = None;
        prepare_notes(state);
        if let Some(next) = notes(state).first().cloned() {
            load_note(state, next);
        }
    }
}

fn choose_notes_dir(state: &Rc<AppState>) {
    save_if_dirty(state);
    let initial_folder = gio::File::for_path(state.settings.borrow().notes_dir.clone());
    let dialog = gtk::FileDialog::builder()
        .title("Choose notes folder")
        .modal(true)
        .accept_label("Choose")
        .initial_folder(&initial_folder)
        .build();

    let chooser_state = state.clone();
    dialog.select_folder(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    {
                        let mut settings = chooser_state.settings.borrow_mut();
                        settings.notes_dir = path;
                        settings.last_note = None;
                        settings.save();
                    }
                    prepare_notes(&chooser_state);
                    if let Some(note) = notes(&chooser_state).first().cloned() {
                        load_note(&chooser_state, note);
                    }
                }
            }
        },
    );
}

fn open_quick_open(state: &Rc<AppState>) {
    save_if_dirty(state);
    state.quick_entry.set_text("");
    populate_quick_open(state);
    state.quick_panel.set_visible(true);
    state.quick_entry.grab_focus();
}

fn close_quick_open(state: &Rc<AppState>) {
    state.quick_panel.set_visible(false);
    state.view.grab_focus();
}

fn populate_quick_open(state: &Rc<AppState>) {
    while let Some(child) = state.quick_list.first_child() {
        state.quick_list.remove(&child);
    }

    let query = state.quick_entry.text().to_string().to_lowercase();
    for path in notes(state)
        .into_iter()
        .filter(|path| note_title(path).to_lowercase().contains(&query))
        .take(12)
    {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(true);
        row.set_activatable(true);
        row.set_child(Some(
            &gtk::Label::builder()
                .label(note_title(&path))
                .xalign(0.0)
                .build(),
        ));
        unsafe {
            row.set_data("path", path);
        }
        state.quick_list.append(&row);
    }

    if let Some(row) = state.quick_list.row_at_index(0) {
        state.quick_list.select_row(Some(&row));
    }
}

fn open_selected_quick_note(state: &Rc<AppState>) {
    if let Some(row) = state.quick_list.selected_row() {
        if let Some(path) = row_path(&row) {
            load_note(state, path);
            close_quick_open(state);
        }
    }
}

fn row_path(row: &gtk::ListBoxRow) -> Option<PathBuf> {
    unsafe {
        row.data::<PathBuf>("path")
            .map(|data| data.as_ref().clone())
    }
}

fn insert_checkbox(state: &Rc<AppState>) {
    let (start, end) = line_bounds(&state.buffer);
    let text = state.buffer.text(&start, &end, true).to_string();
    if text.starts_with(UNCHECKED) || text.starts_with(CHECKED) {
        return;
    }

    let cursor = state.buffer.iter_at_mark(&state.buffer.get_insert());
    if cursor.line_offset() == 0 {
        state.buffer.insert_at_cursor(UNCHECKED);
    } else {
        state.buffer.insert_at_cursor(&format!("\n{UNCHECKED}"));
    }
}

fn toggle_current_line(state: &Rc<AppState>) {
    let (start, end) = line_bounds(&state.buffer);
    toggle_line(&state.buffer, &start, &end);
}

fn toggle_line(buffer: &sourceview::Buffer, start: &gtk::TextIter, end: &gtk::TextIter) {
    let text = buffer.text(start, end, true).to_string();
    let replacement = if let Some(rest) = text.strip_prefix(UNCHECKED) {
        format!("{CHECKED}{rest}")
    } else if let Some(rest) = text.strip_prefix(CHECKED) {
        format!("{UNCHECKED}{rest}")
    } else if let Some(rest) = text.strip_prefix("- [ ] ") {
        format!("{CHECKED}{rest}")
    } else if text.to_lowercase().starts_with("- [x] ") {
        format!("{UNCHECKED}{}", &text[6..])
    } else {
        format!("{UNCHECKED}{text}")
    };

    let offset = start.offset();
    let mut delete_start = *start;
    let mut delete_end = *end;
    buffer.delete(&mut delete_start, &mut delete_end);
    let mut insert_at = buffer.iter_at_offset(offset);
    buffer.insert(&mut insert_at, &replacement);
    buffer.place_cursor(&buffer.iter_at_offset(offset + replacement.chars().count() as i32));
}

fn continue_checkbox_line(state: &Rc<AppState>) -> bool {
    let (start, end) = line_bounds(&state.buffer);
    let text = state.buffer.text(&start, &end, true).to_string();
    if let Some(rest) = text.strip_prefix("- [ ] ") {
        replace_line(&state.buffer, &start, &end, &format!("{UNCHECKED}{rest}"));
        state.buffer.insert_at_cursor(&format!("\n{UNCHECKED}"));
        return true;
    }
    if text.to_lowercase().starts_with("- [x] ") {
        replace_line(
            &state.buffer,
            &start,
            &end,
            &format!("{CHECKED}{}", &text[6..]),
        );
        state.buffer.insert_at_cursor(&format!("\n{UNCHECKED}"));
        return true;
    }
    if text.starts_with(UNCHECKED) || text.starts_with(CHECKED) {
        state.buffer.insert_at_cursor(&format!("\n{UNCHECKED}"));
        return true;
    }
    false
}

fn replace_line(
    buffer: &sourceview::Buffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    replacement: &str,
) {
    let offset = start.offset();
    let mut delete_start = *start;
    let mut delete_end = *end;
    buffer.delete(&mut delete_start, &mut delete_end);
    let mut insert_at = buffer.iter_at_offset(offset);
    buffer.insert(&mut insert_at, replacement);
    buffer.place_cursor(&buffer.iter_at_offset(offset + replacement.chars().count() as i32));
}

fn refresh_markdown_rendering(state: &Rc<AppState>) {
    let start = state.buffer.start_iter();
    let end = state.buffer.end_iter();
    for tag in state.render_tags.all() {
        state.buffer.remove_tag(tag, &start, &end);
    }

    let mut line = state.buffer.start_iter();
    let mut in_code_block = false;
    loop {
        let line_start = line;
        let mut line_end = line_start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let text = state.buffer.text(&line_start, &line_end, true).to_string();

        if text.trim_start().starts_with("```") {
            state
                .buffer
                .apply_tag(&state.render_tags.marker, &line_start, &line_end);
            in_code_block = !in_code_block;
        } else if in_code_block {
            state
                .buffer
                .apply_tag(&state.render_tags.code_block, &line_start, &line_end);
            render_code_line(state, &text, &line_start);
        } else {
            render_markdown_line(state, &text, &line_start, &line_end);
        }

        if line_end.is_end() {
            break;
        }
        line = line_end;
        if !line.forward_line() {
            break;
        }
    }
}

fn render_markdown_line(
    state: &Rc<AppState>,
    text: &str,
    line_start: &gtk::TextIter,
    line_end: &gtk::TextIter,
) {
    if let Some(marker_len) = heading_marker_len(text, "# ") {
        state
            .buffer
            .apply_tag(&state.render_tags.heading1, line_start, line_end);
        apply_char_range(state, &state.render_tags.marker, line_start, 0, marker_len);
    } else if let Some(marker_len) = heading_marker_len(text, "## ") {
        state
            .buffer
            .apply_tag(&state.render_tags.heading2, line_start, line_end);
        apply_char_range(state, &state.render_tags.marker, line_start, 0, marker_len);
    } else if let Some(marker_len) = heading_marker_len(text, "### ") {
        state
            .buffer
            .apply_tag(&state.render_tags.heading3, line_start, line_end);
        apply_char_range(state, &state.render_tags.marker, line_start, 0, marker_len);
    }

    if text.starts_with("> ") {
        state
            .buffer
            .apply_tag(&state.render_tags.quote, line_start, line_end);
        apply_char_range(state, &state.render_tags.marker, line_start, 0, 2);
    }

    render_checkboxes(state, text, line_start);
    render_inline_pattern(
        state,
        text,
        line_start,
        r"\*\*([^*\n]+?)\*\*",
        &state.render_tags.bold,
        2,
        2,
    );
    render_inline_pattern(
        state,
        text,
        line_start,
        r"(?:^|[^*])\*([^*\n]+?)\*(?:[^*]|$)",
        &state.render_tags.italic,
        1,
        1,
    );
    render_inline_pattern(
        state,
        text,
        line_start,
        r"__([^_\n]+?)__",
        &state.render_tags.bold,
        2,
        2,
    );
    render_inline_pattern(
        state,
        text,
        line_start,
        r"(?:^|[^_])_([^_\n]+?)_(?:[^_]|$)",
        &state.render_tags.italic,
        1,
        1,
    );
    render_inline_pattern(
        state,
        text,
        line_start,
        r"~~([^~\n]+?)~~",
        &state.render_tags.strike,
        2,
        2,
    );
    render_inline_pattern(
        state,
        text,
        line_start,
        r"\+\+([^+\n]+?)\+\+",
        &state.render_tags.underline,
        2,
        2,
    );
    render_html_underline(state, text, line_start);
    render_inline_pattern(
        state,
        text,
        line_start,
        r"`([^`\n]+?)`",
        &state.render_tags.inline_code,
        1,
        1,
    );
    render_links(state, text, line_start);
}

fn heading_marker_len(text: &str, marker: &str) -> Option<i32> {
    text.starts_with(marker)
        .then(|| marker.chars().count() as i32)
}

fn render_checkboxes(state: &Rc<AppState>, text: &str, line_start: &gtk::TextIter) {
    if text.starts_with(UNCHECKED) || text.starts_with(CHECKED) {
        apply_char_range(state, &state.render_tags.checkbox, line_start, 0, 1);
        if text.starts_with(CHECKED) {
            apply_char_range(
                state,
                &state.render_tags.completed,
                line_start,
                2,
                text.chars().count() as i32,
            );
        }
        return;
    }

    if text.starts_with("- [ ] ") || text.to_lowercase().starts_with("- [x] ") {
        apply_char_range(state, &state.render_tags.marker, line_start, 0, 2);
        apply_char_range(state, &state.render_tags.checkbox, line_start, 2, 5);
        apply_char_range(state, &state.render_tags.marker, line_start, 5, 6);
        if text.to_lowercase().starts_with("- [x] ") {
            apply_char_range(
                state,
                &state.render_tags.completed,
                line_start,
                6,
                text.chars().count() as i32,
            );
        }
    }
}

fn render_inline_pattern(
    state: &Rc<AppState>,
    text: &str,
    line_start: &gtk::TextIter,
    pattern: &str,
    tag: &gtk::TextTag,
    open_len: usize,
    close_len: usize,
) {
    let Ok(regex) = regex::Regex::new(pattern) else {
        return;
    };
    for item in regex
        .captures_iter(text)
        .filter_map(|capture| capture.get(1))
    {
        let start = char_offset(text, item.start()) - open_len as i32;
        let end = char_offset(text, item.end()) + close_len as i32;
        let open_end = start + open_len as i32;
        let close_start = end - close_len as i32;

        apply_char_range(
            state,
            &state.render_tags.marker,
            line_start,
            start,
            open_end,
        );
        apply_char_range(
            state,
            &state.render_tags.marker,
            line_start,
            close_start,
            end,
        );
        apply_char_range(state, tag, line_start, open_end, close_start);
    }
}

fn render_html_underline(state: &Rc<AppState>, text: &str, line_start: &gtk::TextIter) {
    let regex = regex::Regex::new(r"(?i)<u>(.+?)</u>").expect("valid underline regex");
    for item in regex.find_iter(text) {
        let start = char_offset(text, item.start());
        let end = char_offset(text, item.end());
        let open_end = start + 3;
        let close_start = end - 4;
        apply_char_range(
            state,
            &state.render_tags.marker,
            line_start,
            start,
            open_end,
        );
        apply_char_range(
            state,
            &state.render_tags.marker,
            line_start,
            close_start,
            end,
        );
        apply_char_range(
            state,
            &state.render_tags.underline,
            line_start,
            open_end,
            close_start,
        );
    }
}

fn render_links(state: &Rc<AppState>, text: &str, line_start: &gtk::TextIter) {
    let regex = regex::Regex::new(r"\[([^\]\n]+?)\]\(([^)\n]+?)\)").expect("valid link regex");
    for capture in regex.captures_iter(text) {
        let whole = capture.get(0).expect("whole link match");
        let label = capture.get(1).expect("link label match");
        let url = capture.get(2).expect("link url match");

        apply_byte_range(
            state,
            &state.render_tags.marker,
            line_start,
            whole.start(),
            label.start(),
            text,
        );
        apply_byte_range(
            state,
            &state.render_tags.link,
            line_start,
            label.start(),
            label.end(),
            text,
        );
        apply_byte_range(
            state,
            &state.render_tags.marker,
            line_start,
            label.end(),
            url.end() + 1,
            text,
        );
    }
}

fn render_code_line(state: &Rc<AppState>, text: &str, line_start: &gtk::TextIter) {
    let patterns = [
        (
            &state.render_tags.code_string,
            r#""(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'"#,
        ),
        (&state.render_tags.code_comment, r"(?:#|//).*?$"),
        (
            &state.render_tags.code_number,
            r"\b(?:0x[0-9a-fA-F]+|\d+(?:\.\d+)?)\b",
        ),
        (
            &state.render_tags.code_keyword,
            r"\b(?:and|as|async|await|break|case|catch|class|const|continue|def|do|else|elif|enum|export|false|finally|fn|for|from|function|if|impl|import|in|interface|let|match|mod|new|none|null|or|pass|pub|raise|return|self|static|struct|switch|throw|trait|true|try|use|var|while|with|yield)\b",
        ),
    ];

    for (tag, pattern) in patterns {
        let Ok(regex) = regex::Regex::new(pattern) else {
            continue;
        };
        for item in regex.find_iter(text) {
            apply_byte_range(state, tag, line_start, item.start(), item.end(), text);
        }
    }

    if let Ok(function_regex) = regex::Regex::new(r"\b([A-Za-z_]\w*)\s*\(") {
        for capture in function_regex.captures_iter(text) {
            if let Some(name) = capture.get(1) {
                apply_byte_range(
                    state,
                    &state.render_tags.code_function,
                    line_start,
                    name.start(),
                    name.end(),
                    text,
                );
            }
        }
    }
}

fn apply_byte_range(
    state: &Rc<AppState>,
    tag: &gtk::TextTag,
    line_start: &gtk::TextIter,
    byte_start: usize,
    byte_end: usize,
    text: &str,
) {
    apply_char_range(
        state,
        tag,
        line_start,
        char_offset(text, byte_start),
        char_offset(text, byte_end),
    );
}

fn apply_char_range(
    state: &Rc<AppState>,
    tag: &gtk::TextTag,
    line_start: &gtk::TextIter,
    start_offset: i32,
    end_offset: i32,
) {
    if end_offset <= start_offset {
        return;
    }
    let mut start = *line_start;
    let mut end = *line_start;
    start.forward_chars(start_offset);
    end.forward_chars(end_offset);
    state.buffer.apply_tag(tag, &start, &end);
}

fn char_offset(text: &str, byte_offset: usize) -> i32 {
    text[..byte_offset].chars().count() as i32
}

fn check_external_change(state: &Rc<AppState>) {
    let Some(path) = state.current_note.borrow().clone() else {
        return;
    };
    let changed = mtime_ns(&path) != state.disk_mtime.get();
    if changed && buffer_text(&state.buffer) == *state.last_saved.borrow() {
        load_note(state, path);
    }
}

fn save_if_dirty(state: &Rc<AppState>) {
    if buffer_text(&state.buffer) != *state.last_saved.borrow() {
        save_note(state);
    }
}

fn change_font_size(state: &Rc<AppState>, amount: i32) {
    {
        let mut settings = state.settings.borrow_mut();
        settings.font_size = (settings.font_size + amount).clamp(9, 30);
        settings.save();
    }
    apply_font_size(state);
}

fn apply_font_size(state: &Rc<AppState>) {
    let size = state.settings.borrow().font_size;
    state.font_provider.load_from_data(&format!(
        "textview {{ font-family: {}; font-size: {size}px; }} .body {{ font-family: {}; }}",
        css_font_family(&state.dot_config.font_family),
        css_font_family(&state.dot_config.font_family)
    ));
}

fn line_bounds(buffer: &sourceview::Buffer) -> (gtk::TextIter, gtk::TextIter) {
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let mut start = cursor;
    start.set_line_offset(0);
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    (start, end)
}

fn buffer_text(buffer: &sourceview::Buffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn note_title(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        let title = clean_title(line);
        if !title.is_empty() {
            return title.chars().take(32).collect();
        }
    }
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn note_slug(text: &str) -> Option<String> {
    for line in text.lines() {
        let title = clean_title(line);
        if title.is_empty() {
            continue;
        }
        let ascii = title
            .chars()
            .filter_map(|character| {
                if character.is_ascii_alphanumeric() {
                    Some(character.to_ascii_lowercase())
                } else if character.is_whitespace() || "-_./".contains(character) {
                    Some('-')
                } else {
                    None
                }
            })
            .collect::<String>();
        let slug = regex::Regex::new("-+")
            .expect("valid slug regex")
            .replace_all(ascii.trim_matches('-'), "-")
            .to_string();
        if !slug.is_empty() {
            return Some(
                slug.chars()
                    .take(64)
                    .collect::<String>()
                    .trim_matches('-')
                    .to_string(),
            );
        }
    }
    None
}

fn clean_title(line: &str) -> String {
    let mut title = line.trim().to_string();
    for prefix in [
        "###### ", "##### ", "#### ", "### ", "## ", "# ", UNCHECKED, CHECKED, "- [ ] ", "- [x] ",
        "- [X] ",
    ] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.to_string();
            break;
        }
    }
    title.replace(['*', '_', '`'], "").trim().to_string()
}

fn mtime_ns(path: &Path) -> u128 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn default_notes_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("NibNotes")
}

fn settings_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().unwrap_or_else(|| PathBuf::from(".")))
            .join("NibNotes")
            .join("settings.json")
    } else if cfg!(target_os = "macos") {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join("NibNotes")
            .join("settings.json")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("nibnotes")
            .join("settings.json")
    }
}

fn config_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().unwrap_or_else(|| PathBuf::from(".")))
            .join("NibNotes")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("nibnotes")
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn keys_path() -> PathBuf {
    config_dir().join("keys.json")
}

fn theme_path() -> PathBuf {
    config_dir().join("theme.css")
}

fn ensure_dotfiles() {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    create_file_if_missing(&config_path(), DEFAULT_CONFIG_JSON);
    create_file_if_missing(&keys_path(), DEFAULT_KEYS_JSON);
    create_file_if_missing(&theme_path(), CUSTOM_THEME_TEMPLATE_CSS);
    create_file_if_missing(
        &config_dir().join("config.synced-folder.example.json"),
        EXAMPLE_SYNC_CONFIG_JSON,
    );
    create_file_if_missing(&config_dir().join("themes.txt"), THEME_HELP_TEXT);
    create_file_if_missing(&config_dir().join("synced-folders.txt"), SYNC_FOLDERS_TEXT);
}

fn create_file_if_missing(path: &Path, contents: &str) {
    if !path.exists() {
        let _ = fs::write(path, contents);
    }
}

fn load_theme_css(theme: &str) -> String {
    match theme.trim().to_lowercase().as_str() {
        "catppuccin" | "catppuccin-mocha" | "mocha" => CATPPUCCIN_THEME_CSS.to_string(),
        "custom" => fs::read_to_string(theme_path())
            .unwrap_or_else(|_| CUSTOM_THEME_TEMPLATE_CSS.to_string()),
        _ => GRUVBOX_THEME_CSS.to_string(),
    }
}

fn default_key_specs() -> Vec<(&'static str, Action, &'static str)> {
    vec![
        ("new_note", Action::NewNote, "Primary+N"),
        ("quick_open", Action::QuickOpen, "Primary+O"),
        (
            "choose_notes_dir",
            Action::ChooseNotesDir,
            "Primary+Shift+O",
        ),
        ("save", Action::Save, "Primary+S"),
        ("save_quit", Action::SaveQuit, "Primary+Q"),
        ("show_help", Action::ShowHelp, "Primary+M"),
        ("insert_checkbox", Action::InsertCheckbox, "Primary+T"),
        ("toggle_checkbox", Action::ToggleCheckbox, "Primary+Enter"),
        ("increase_font", Action::IncreaseFont, "Primary+Plus"),
        ("decrease_font", Action::DecreaseFont, "Primary+Minus"),
        ("reset_font", Action::ResetFont, "Primary+0"),
        ("trash_note", Action::TrashNote, "Primary+Shift+Delete"),
    ]
}

fn primary_modifier_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

fn has_command_modifier(modifier: gdk::ModifierType) -> bool {
    modifier.contains(gdk::ModifierType::META_MASK)
        || modifier.contains(gdk::ModifierType::SUPER_MASK)
}

fn normalize_key(key: gdk::Key) -> String {
    match key {
        gdk::Key::Return | gdk::Key::KP_Enter => "enter".to_string(),
        gdk::Key::Delete | gdk::Key::KP_Delete => "delete".to_string(),
        gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => "plus".to_string(),
        gdk::Key::minus | gdk::Key::KP_Subtract => "minus".to_string(),
        gdk::Key::_0 | gdk::Key::KP_0 => "0".to_string(),
        other => other
            .name()
            .map(|name| name.to_string().to_lowercase())
            .unwrap_or_default(),
    }
}

fn css_font_family(font_family: &str) -> String {
    font_family
        .split(',')
        .map(|font| {
            let font = font.trim();
            if font == "monospace" || font == "serif" || font == "sans-serif" {
                font.to_string()
            } else {
                format!("\"{}\"", font.replace('"', ""))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}
