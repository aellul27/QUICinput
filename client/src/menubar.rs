use gtk4::prelude::*;
use libadwaita::gio::Menu;
use libadwaita::Application;

pub fn setup(app: &Application) {
	let menubar = Menu::new();
    let connect_menu = Menu::new();
    connect_menu.append(Some("test"), Some("app.test"));
    menubar.append_submenu(Some("Connect"), &connect_menu);
    app.set_menubar(Some(&menubar));
}
