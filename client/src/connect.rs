use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Button, Entry, Image, Label, Orientation, Spinner};
use quinn::{Connection, Endpoint};
use std::cell::{Cell, RefCell};
use std::net::{IpAddr, SocketAddr};
use std::rc::Rc;

use crate::quic::{quic_runtime, run_client};

const OUTER_MARGIN: i32 = 24;
const COLUMN_SPACING: i32 = 16;
const INPUT_ROW_SPACING: i32 = 12;
const STATUS_ROW_SPACING: i32 = 8;

type ConnectHandler = dyn Fn(String, u16, Endpoint, Connection);

#[derive(Clone)]
pub struct ConnectView {
    root: Box,
    ip_entry: Entry,
    port_entry: Entry,
    enter_button: Button,
    status_row: Box,
    status_label: Label,
    spinner_row: Box,
    spinner: Spinner,
    session_id: Rc<Cell<u64>>,
    on_success: Rc<RefCell<Option<Rc<ConnectHandler>>>>,
}

impl ConnectView {
    pub fn new() -> Self {
        let root = build_container();
        root.append(&build_prompt());

        let (input_row, ip_entry, port_entry, enter_button) = build_input_row();
        root.append(&input_row);

        let (spinner_row, spinner) = build_spinner_row();
        root.append(&spinner_row);

        let (status_row, status_label) = build_status_row();
        root.append(&status_row);

        let view = Self {
            root,
            ip_entry,
            port_entry,
            enter_button,
            status_row,
            status_label,
            spinner_row,
            spinner,
            session_id: Rc::new(Cell::new(0)),
            on_success: Rc::new(RefCell::new(None)),
        };

        view.wire_enter_button();

        view
    }

    pub fn widget(&self) -> Box {
        self.root.clone()
    }

    pub fn set_on_connect<F>(&self, handler: F)
    where
        F: Fn(String, u16, Endpoint, Connection) + 'static,
    {
        let handler: Rc<ConnectHandler> = Rc::new(handler);
        self.on_success.borrow_mut().replace(handler);
    }

    pub fn reset(&self) {
        self.bump_session();
        self.hide_status();
        self.hide_spinner();
        self.enter_button.set_sensitive(true);
        self.ip_entry.set_sensitive(true);
        self.port_entry.set_sensitive(true);
        self.ip_entry.set_text("");
        self.port_entry.set_text("");
        self.ip_entry.grab_focus();
    }

    pub fn focus(&self) {
        self.ip_entry.grab_focus();
    }

    fn wire_enter_button(&self) {
        let button_for_ip = self.enter_button.clone();
        self.ip_entry.connect_activate(move |_entry| {
            button_for_ip.emit_clicked();
        });

        let button_for_port = self.enter_button.clone();
        self.port_entry.connect_activate(move |_entry| {
            button_for_port.emit_clicked();
        });

        let ip_entry = self.ip_entry.clone();
        let port_entry = self.port_entry.clone();
        let status_row = self.status_row.clone();
        let status_label = self.status_label.clone();
        let spinner_row = self.spinner_row.clone();
        let spinner = self.spinner.clone();
        let session_id = self.session_id.clone();
        let on_success = self.on_success.clone();

        self.enter_button.connect_clicked(move |button| {
            hide_status(&status_row, &status_label);

            let ip_value = ip_entry.text();
            let ip = ip_value.trim().to_string();
            if ip.is_empty() {
                show_status(&status_row, &status_label, "IP address is required");
                return;
            }

            let port_value = port_entry.text();
            let port = port_value.trim().to_string();
            if port.is_empty() {
                show_status(&status_row, &status_label, "Port is required");
                return;
            }

            let portnum = match port.parse::<u16>() {
                Ok(n) => n,
                Err(_) => {
                    show_status(&status_row, &status_label, "Invalid port number");
                    return;
                }
            };

            let ip_addr = match ip.parse::<IpAddr>() {
                Ok(a) => a,
                Err(_) => {
                    show_status(&status_row, &status_label, "Invalid IP address");
                    return;
                }
            };
            let server_addr = SocketAddr::new(ip_addr, portnum);

            show_spinner(&spinner_row, &spinner);
            button.set_sensitive(false);
            ip_entry.set_sensitive(false);
            port_entry.set_sensitive(false);

            let runtime_handle = quic_runtime().handle().clone();
            let status_row_async = status_row.clone();
            let status_label_async = status_label.clone();
            let spinner_row_async = spinner_row.clone();
            let spinner_async = spinner.clone();
            let ip_entry_async = ip_entry.clone();
            let port_entry_async = port_entry.clone();
            let button_async = button.clone();
            let handler_option = on_success.borrow().clone();
            let ip_for_callback = ip.clone();
            let session_marker = session_id.get();
            let session_id_async = session_id.clone();

            glib::MainContext::default().spawn_local(async move {
                let result = runtime_handle
                    .spawn(async move { run_client(server_addr).await })
                    .await;

                if session_id_async.get() != session_marker {
                    return;
                }

                hide_spinner(&spinner_row_async, &spinner_async);
                button_async.set_sensitive(true);
                ip_entry_async.set_sensitive(true);
                port_entry_async.set_sensitive(true);

                match result {
                    Ok(Ok((endpoint, connection))) => {
                        hide_status(&status_row_async, &status_label_async);
                        if let Some(handler) = handler_option {
                            handler(ip_for_callback, portnum, endpoint, connection);
                        }
                    }
                    Ok(Err(err)) => {
                        let message = format!("Failed to connect: {err}");
                        show_status(&status_row_async, &status_label_async, &message);
                        println!("{message}");
                    }
                    Err(join_err) => {
                        let message = format!("Failed to connect: {join_err}");
                        show_status(&status_row_async, &status_label_async, &message);
                        println!("{message}");
                    }
                }
            });
        });
    }

    fn hide_status(&self) {
        hide_status(&self.status_row, &self.status_label);
    }

    fn hide_spinner(&self) {
        hide_spinner(&self.spinner_row, &self.spinner);
    }

    fn bump_session(&self) {
        let next = self.session_id.get().wrapping_add(1);
        self.session_id.set(next);
    }
}

fn build_container() -> Box {
    let container = Box::new(Orientation::Vertical, COLUMN_SPACING);
    container.set_margin_top(OUTER_MARGIN);
    container.set_margin_bottom(OUTER_MARGIN);
    container.set_margin_start(OUTER_MARGIN);
    container.set_margin_end(OUTER_MARGIN);
    container
}

fn build_prompt() -> Label {
    let prompt = Label::new(Some("Input IP and Port"));
    prompt.set_xalign(0.0);
    prompt.add_css_class("title-4");
    prompt
}

fn build_input_row() -> (Box, Entry, Entry, Button) {
    let row = Box::new(Orientation::Horizontal, INPUT_ROW_SPACING);
    row.set_hexpand(true);

    let ip_entry = Entry::new();
    ip_entry.set_placeholder_text(Some("IP address"));
    ip_entry.set_hexpand(true);

    let port_entry = Entry::new();
    port_entry.set_placeholder_text(Some("Port"));
    port_entry.set_width_chars(6);

    let enter_button = Button::with_label("Enter");
    enter_button.add_css_class("suggested-action");

    row.append(&ip_entry);
    row.append(&port_entry);
    row.append(&enter_button);

    (row, ip_entry, port_entry, enter_button)
}

fn build_status_row() -> (Box, Label) {
    let row = Box::new(Orientation::Horizontal, STATUS_ROW_SPACING);
    row.set_visible(false);
    row.add_css_class("error");

    let status_icon = Image::from_icon_name("dialog-error-symbolic");
    row.append(&status_icon);

    let label = Label::new(None);
    label.set_xalign(0.0);
    row.append(&label);

    (row, label)
}

fn build_spinner_row() -> (Box, Spinner) {
    let row = Box::new(Orientation::Horizontal, STATUS_ROW_SPACING);
    row.set_visible(false);

    let spinner = Spinner::new();
    spinner.set_spinning(false);
    row.append(&spinner);

    let label = Label::new(Some("Connecting…"));
    label.set_xalign(0.0);
    row.append(&label);

    (row, spinner)
}

fn hide_status(row: &Box, label: &Label) {
    label.set_text("");
    row.set_visible(false);
}

fn show_status(row: &Box, label: &Label, message: &str) {
    label.set_text(message);
    row.set_visible(true);
}

fn show_spinner(row: &Box, spinner: &Spinner) {
    row.set_visible(true);
    spinner.start();
}

fn hide_spinner(row: &Box, spinner: &Spinner) {
    spinner.stop();
    row.set_visible(false);
}