mod connect;
mod input;
mod key_monitor;
mod menubar;
mod windowresolution;
mod quic;

use libadwaita::gio::SimpleAction;
use libadwaita::prelude::*;
use libadwaita::{glib, Application, ApplicationWindow, HeaderBar};
use gtk4::{Box, Orientation, Stack, StackTransitionType};
use rustls::crypto::aws_lc_rs;
use rustls::crypto::CryptoProvider;


const APP_ID: &str = "com.aellul27.quicinput.client";

fn main() -> glib::ExitCode {
    // Create a new application
    let app = Application::builder().application_id(APP_ID).build();
    CryptoProvider::install_default(aws_lc_rs::default_provider())
            .expect("Failed to install default crypto provider");

    app.connect_activate(build_ui);

    // Run the application
    app.run()
}

fn build_ui(app: &Application) {

    // Combine the content in a box
    let content = Box::new(Orientation::Vertical, 0);
    // Adwaita's ApplicationWindow does not include a HeaderBar
    let header = HeaderBar::new();
    header.pack_end(&menubar::build(app));
    content.append(&header);

    let stack = build_stack();
    content.append(&stack);

    if app.lookup_action("test").is_none() {
        let connect_action = SimpleAction::new("test", None);
        connect_action.connect_activate(|_, _| {
            println!("Connect > test triggered");
        });
        app.add_action(&connect_action);
    }

    let (window_height, window_width) = windowresolution::find_window_size();

    // Create a window, set the title, and size it relative to the primary display
    let window = ApplicationWindow::builder()
        .application(app)
        .title("QUICinput")
        .default_height(window_height as i32)
        .default_width(window_width as i32)
        .content(&content)
        .build();
    
    // Present window
    window.present();
}

fn build_stack() -> Stack {
    let stack = Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .transition_type(StackTransitionType::SlideLeft)
        .build();

    let input_view = input::build();
    stack.add_named(&input_view, Some("input"));

    let stack_for_connect = stack.clone();
    let input_view_for_connect = input_view.clone();
    let connect_view = connect::build(move |ip, port| {
        println!("Connecting to {}:{}", ip, port);
        stack_for_connect.set_visible_child_name("input");
        input_view_for_connect.grab_focus();
    });
    stack.add_named(&connect_view, Some("connect"));

    stack.set_visible_child_name("connect");
    stack
}