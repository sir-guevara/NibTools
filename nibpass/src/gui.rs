use crate::{copy_to_clipboard, read_entry_with_key, unlock_vault, Result, Store};
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label, ListBox,
    Orientation, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "dev.nibtools.NibPass";

const GRUVBOX_CSS: &str = r#"
window.nibpass {
  background: #1d2021;
}

.nibpass-root {
  background: #1d2021;
}

.nibpass-search {
  min-height: 34px;
  padding: 6px 10px;
  background: #282828;
  color: #ebdbb2;
  border: 1px solid #504945;
  border-radius: 6px;
  box-shadow: none;
}

.nibpass-search text {
  color: #ebdbb2;
  background: #282828;
}

.nibpass-list {
  background: #1d2021;
  color: #ebdbb2;
}

.nibpass-list row {
  min-height: 34px;
  padding: 0;
  background: #1d2021;
  color: #ebdbb2;
  border-radius: 4px;
}

.nibpass-list row:hover {
  background: #282828;
}

.nibpass-list row:selected {
  background: #458588;
  color: #fbf1c7;
}

.nibpass-list row:selected .nibpass-account-label {
  color: #fbf1c7;
}

.nibpass-account-label {
  color: #ebdbb2;
  padding: 8px 10px;
  font-size: 13px;
}

.nibpass-button {
  min-height: 34px;
  padding: 6px 12px;
  background: #3c3836;
  color: #ebdbb2;
  border: 1px solid #665c54;
  border-radius: 6px;
  box-shadow: none;
}

.nibpass-button:hover {
  background: #504945;
}

.nibpass-button:active {
  background: #458588;
  color: #fbf1c7;
}
"#;

pub fn run(store: &Store) -> Result<()> {
    let store_root = store.root.clone();
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        let store = Store {
            root: store_root.clone(),
        };
        build_window(app, store);
    });

    app.run();
    Ok(())
}

fn build_window(app: &Application, store: Store) {
    install_gruvbox_css();
    let selected = Rc::new(RefCell::new(String::new()));
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.add_css_class("nibpass-root");
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let search = Entry::builder().placeholder_text("Search accounts").build();
    search.add_css_class("nibpass-search");
    let list = ListBox::new();
    list.add_css_class("nibpass-list");
    populate_list(&store, &list, "", selected.clone());

    let actions = GtkBox::new(Orientation::Horizontal, 8);
    let copy_password = Button::with_label("Copy Password");
    let copy_2fa = Button::with_label("Copy 2FA");
    copy_password.add_css_class("nibpass-button");
    copy_2fa.add_css_class("nibpass-button");
    actions.append(&copy_password);
    actions.append(&copy_2fa);

    {
        let list = list.clone();
        let store = Store {
            root: store.root.clone(),
        };
        let selected = selected.clone();
        search.connect_changed(move |entry| {
            populate_list(&store, &list, &entry.text(), selected.clone());
        });
    }

    {
        let store = Store {
            root: store.root.clone(),
        };
        let selected = selected.clone();
        copy_password.connect_clicked(move |_| {
            copy_selected_field(&store, &selected.borrow(), "password");
        });
    }

    {
        let store = Store {
            root: store.root.clone(),
        };
        let selected = selected.clone();
        copy_2fa.connect_clicked(move |_| {
            copy_selected_field(&store, &selected.borrow(), "otp");
        });
    }

    root.append(&search);
    root.append(&list);
    root.append(&actions);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("NibPass")
        .default_width(420)
        .default_height(560)
        .child(&root)
        .build();
    window.add_css_class("nibpass");
    window.present();
}

fn install_gruvbox_css() {
    let provider = CssProvider::new();
    provider.load_from_data(GRUVBOX_CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn populate_list(store: &Store, list: &ListBox, query: &str, selected: Rc<RefCell<String>>) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    let mut accounts = Vec::new();
    if crate::collect_entries(&store.root, &store.root, &mut accounts).is_err() {
        return;
    }
    let query = query.to_ascii_lowercase();
    for account in accounts
        .into_iter()
        .filter(|account| account.to_ascii_lowercase().contains(&query))
    {
        let label = Label::new(Some(&account));
        label.set_xalign(0.0);
        label.add_css_class("nibpass-account-label");
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&label));
        {
            let selected = selected.clone();
            let account = account.clone();
            row.connect_activate(move |_| {
                *selected.borrow_mut() = account.clone();
            });
        }
        list.append(&row);
    }
}

fn copy_selected_field(store: &Store, account: &str, field: &str) {
    if account.is_empty() {
        return;
    }
    if let Ok(vault_key) = unlock_vault(store, false) {
        if let Ok(entry) = read_entry_with_key(store, account, &vault_key) {
            if let Some(value) = entry.get(field) {
                let _ = copy_to_clipboard(value);
            }
        }
    }
}
