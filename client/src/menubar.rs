use gtk4::{prelude::*, Align, MenuButton};
use libadwaita::{gio::Menu, Application};

/// Builds a MenuButton-backed menu so it stays accessible across platforms
/// while still registering with the application (macOS picks it up globally).
pub fn build(app: &Application) -> MenuButton {
    let menubar = Menu::new();
    let connect_menu = Menu::new();
    connect_menu.append(Some("Back to Connect"), Some("app.reset"));
    menubar.append_submenu(Some("Connect"), &connect_menu);

    app.set_menubar(Some(&menubar));

    let button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .valign(Align::Center)
        .build();
    button.set_menu_model(Some(&menubar));

    button
}
