mod connect;
mod input;
mod key_monitor;
mod menubar;
mod windowresolution;
mod quic;
mod quic_helper_thread;

use std::rc::Rc;

use libadwaita::gio::SimpleAction;
use libadwaita::prelude::*;
use libadwaita::{glib, Application, ApplicationWindow, HeaderBar, ToolbarView};
use gtk4::{Stack, StackTransitionType};
use rustls::crypto::aws_lc_rs;
use rustls::crypto::CryptoProvider;
use quinn::{Connection, Endpoint};


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
    let toolbar_view = ToolbarView::new();

    // Header bar sits in the toolbar view so Adwaita can manage window chrome
    let header = HeaderBar::new();
    header.pack_end(&menubar::build(app));
    toolbar_view.add_top_bar(&header);

    let controller = AppController::new();
    toolbar_view.set_content(Some(&controller.stack()));

    if app.lookup_action("reset").is_none() {
        let controller_for_action = controller.clone();
        let reset_action = SimpleAction::new("reset", None);
        reset_action.connect_activate(move |_, _| {
            controller_for_action.reset();
        });
        app.add_action(&reset_action);
    }

    if app.lookup_action("quit").is_none() {
        let controller_for_quit = controller.clone();
        let app_for_quit = app.clone();
        let quit_action = SimpleAction::new("quit", None);
        quit_action.connect_activate(move |_, _| {
            controller_for_quit.shutdown();
            app_for_quit.quit();
        });
        app.add_action(&quit_action);
        app.set_accels_for_action("app.quit", &["<Primary>q"]);
    }

    {
        let controller_for_shutdown = controller.clone();
        app.connect_shutdown(move |_app| {
            controller_for_shutdown.shutdown();
        });
    }

    let (window_height, window_width) = windowresolution::find_window_size();

    // Create a window, set the title, and size it relative to the primary display
    let window = ApplicationWindow::builder()
        .application(app)
        .title("QUICinput")
        .default_height(window_height as i32)
        .default_width(window_width as i32)
        .content(&toolbar_view)
        .build();

    {
        let controller_for_close = controller.clone();
        let app_for_close = app.clone();
        window.connect_close_request(move |_window| {
            controller_for_close.shutdown();
            app_for_close.quit();
            glib::Propagation::Proceed
        });
    }
    
    // Present window
    window.present();
}

struct AppController {
    stack: Stack,
    connect_view: connect::ConnectView,
    input_view: input::InputView,
}

impl AppController {
    fn new() -> Rc<Self> {
        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(StackTransitionType::SlideLeft)
            .build();

        let input_view = input::InputView::new();
        let connect_view = connect::ConnectView::new();

        let controller = Rc::new(Self {
            stack,
            connect_view,
            input_view,
        });

        controller.initialize();

        controller
    }

    fn initialize(self: &Rc<Self>) {
        self.stack
            .add_named(&self.connect_view.widget(), Some("connect"));
        self.stack
            .add_named(&self.input_view.widget(), Some("input"));
        self.stack.set_visible_child_name("connect");

        self.connect_view.set_on_connect({
            let controller = Rc::clone(self);
            move |ip, port, endpoint, connection| {
                controller.handle_connected(ip, port, endpoint, connection);
            }
        });

        self.connect_view.focus();
    }

    fn stack(&self) -> Stack {
        self.stack.clone()
    }

    fn handle_connected(&self, ip: String, port: u16, endpoint: Endpoint, connection: Connection) {
        println!("Connected to {}:{}", ip, port);
        self.input_view.set_connection(endpoint, connection);
        self.show_input();
    }

    fn show_input(&self) {
        self.stack.set_visible_child_name("input");
        self.input_view.focus();
    }

    fn reset(&self) {
        self.shutdown();
        self.stack.set_visible_child_name("connect");
        self.connect_view.focus();
    }

    fn shutdown(&self) {
        self.shutdown_connection();
        self.input_view.reset();
        self.connect_view.reset();
    }

    fn shutdown_connection(&self) {
        if let Some((endpoint, connection)) = self.input_view.take_connection() {
            quic::quic_runtime().spawn(async move {
                if let Err(error) = quic::close_client(connection, endpoint).await {
                    eprintln!("failed to close client cleanly: {error}");
                }
            });
        }
    }
}