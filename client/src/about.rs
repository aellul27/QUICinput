use libadwaita::prelude::*;
use libadwaita::{AboutDialog, ApplicationWindow};

pub fn show_about(parent: Option<&ApplicationWindow>) {
    let dialog = AboutDialog::new();

    dialog.set_application_name("QUICinput");
    dialog.set_developer_name("Alex Ellul");
    dialog.set_version("1.0.0");
    dialog.set_website("https://github.com/aellul27/quicinput");
    dialog.set_comments("A small tool for sending QUIC input data.");

    #[cfg(not(target_os = "macos"))]
    dialog.set_application_icon("Icon");


    dialog.present(parent);
}