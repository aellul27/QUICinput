use glib::SendWeakRef;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, GestureClick, Label, Orientation};
use quinn::{Connection, Endpoint};
use std::cell::RefCell;
use std::rc::Rc;

use crate::key_monitor::start_global_key_monitor;

const OUTER_MARGIN: i32 = 32;
const INNER_SPACING: i32 = 18;
const INFO_DEFAULT: &str = "Click here to start capture.";
const INFO_CAPTURE_ACTIVE: &str = "Type CTRL-ALT-0 to ungrab and stop capture.";

#[derive(Clone)]
pub struct InputView {
	inner: Rc<InputViewInner>,
}

struct InputViewInner {
	container: Box,
	info_label: Label,
	connection: RefCell<Option<(Endpoint, Connection)>>,
}

impl InputView {
	pub fn new() -> Self {
		let container = Box::new(Orientation::Vertical, INNER_SPACING);
		container.set_margin_top(OUTER_MARGIN);
		container.set_margin_bottom(OUTER_MARGIN);
		container.set_margin_start(OUTER_MARGIN);
		container.set_margin_end(OUTER_MARGIN);
		container.set_hexpand(true);
		container.set_vexpand(true);
		container.set_focusable(true);
		container.set_can_focus(true);

		let header_row = Box::new(Orientation::Horizontal, INNER_SPACING);
		header_row.set_hexpand(true);

		let title = Label::new(Some("Event monitor"));
		title.add_css_class("title-3");
		title.set_xalign(0.0);
		title.set_hexpand(true);
		title.set_halign(Align::Start);
		header_row.append(&title);

		let disconnect_button = Button::with_label("Disconnect");
		disconnect_button.set_halign(Align::End);
		disconnect_button.connect_clicked(|button| {
			let _ = button.activate_action("app.reset", None);
		});
		header_row.append(&disconnect_button);

		container.append(&header_row);

		let info_label = Label::new(Some(INFO_DEFAULT));
		info_label.set_xalign(0.0);
		info_label.set_wrap(true);

		let inner = Rc::new(InputViewInner {
			container: container.clone(),
			info_label: info_label.clone(),
			connection: RefCell::new(None),
		});

		let clicker = GestureClick::new();
		let inner_for_click = Rc::clone(&inner);
		clicker.connect_pressed(move |_, _, _, _| {
			inner_for_click.start_capture();
		});
		container.add_controller(clicker);
		container.append(&info_label);

		Self { inner }
	}

	pub fn widget(&self) -> Box {
		self.inner.container.clone()
	}

	pub fn set_connection(&self, endpoint: Endpoint, connection: Connection) {
		self.inner
			.connection
			.borrow_mut()
			.replace((endpoint, connection));
		self.focus();
	}

	pub fn take_connection(&self) -> Option<(Endpoint, Connection)> {
		self.inner.connection.borrow_mut().take()
	}

	pub fn reset(&self) {
		self.inner.connection.borrow_mut().take();
		self.inner.mark_ungrabbed();
	}

	pub fn focus(&self) {
		self.inner.container.grab_focus();
	}
}

impl InputViewInner {
	fn start_capture(self: &Rc<Self>) {
		let maybe_connection = self.connection.borrow().clone();
		let Some((endpoint, connection)) = maybe_connection else {
			return;
		};

		self.mark_grabbed();
		let container_weak: SendWeakRef<Box> = self.container.downgrade().into();
		let label_weak: SendWeakRef<Label> = self.info_label.downgrade().into();
		let started = start_global_key_monitor(endpoint, connection, move || {
			if let Some(container) = container_weak.upgrade() {
				container.set_cursor_from_name(None);
			}
			if let Some(label) = label_weak.upgrade() {
				label.set_label(INFO_DEFAULT);
			}
		});
		if !started {
			self.mark_ungrabbed();
		}
	}

	fn mark_grabbed(&self) {
		self.container.set_cursor_from_name(Some("none"));
		self.info_label.set_label(INFO_CAPTURE_ACTIVE);
	}

	fn mark_ungrabbed(&self) {
		self.container.set_cursor_from_name(None);
		self.info_label.set_label(INFO_DEFAULT);
	}
}
