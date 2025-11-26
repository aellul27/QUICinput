use gtk4::prelude::*;
use gtk4::{Box, Button, Entry, Image, Label, Orientation};
use std::net::IpAddr;
use std::rc::Rc;

const OUTER_MARGIN: i32 = 24;
const COLUMN_SPACING: i32 = 16;
const INPUT_ROW_SPACING: i32 = 12;
const STATUS_ROW_SPACING: i32 = 8;

pub fn build<F>(on_success: F) -> Box
where
    F: Fn(String, u16) + 'static,
{
    let container = build_container();
    container.append(&build_prompt());

    let (input_row, ip_entry, port_entry, enter_button) = build_input_row();
    container.append(&input_row);

    let (status_row, status_label) = build_status_row();
    container.append(&status_row);

    let on_success = Rc::new(on_success);
    wire_enter_button(
        &enter_button,
        &ip_entry,
        &port_entry,
        &status_row,
        &status_label,
        on_success,
    );

    container
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

fn wire_enter_button(
    enter_button: &Button,
    ip_entry: &Entry,
    port_entry: &Entry,
    status_row: &Box,
    status_label: &Label,
    on_success: Rc<dyn Fn(String, u16)>,
) {
    let button_for_ip = enter_button.clone();
    ip_entry.connect_activate(move |_entry| {
        button_for_ip.emit_clicked();
    });

    let button_for_port = enter_button.clone();
    port_entry.connect_activate(move |_entry| {
        button_for_port.emit_clicked();
    });

    let ip_entry = ip_entry.clone();
    let port_entry = port_entry.clone();
    let status_row = status_row.clone();
    let status_label = status_label.clone();
    let on_success = on_success.clone();

    enter_button.connect_clicked(move |_btn: &Button| {
        hide_status(&status_row, &status_label);

        let ip_value = ip_entry.text();
        let ip = ip_value.trim();
        if ip.is_empty() {
            show_status(&status_row, &status_label, "IP address is required");
            return;
        }

        let port_value = port_entry.text();
        let port = port_value.trim();
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

        if ip.parse::<IpAddr>().is_err() {
            show_status(&status_row, &status_label, "Invalid IP address");
            return;
        }

        on_success(ip.to_string(), portnum);
    });
}

fn hide_status(row: &Box, label: &Label) {
    label.set_text("");
    row.set_visible(false);
}

fn show_status(row: &Box, label: &Label, message: &str) {
    label.set_text(message);
    row.set_visible(true);
}