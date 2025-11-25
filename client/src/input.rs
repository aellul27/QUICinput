use gtk4::prelude::*;
use gtk4::{Box, GestureClick, Label, Orientation};
use rdev::{grab, Event, EventType, Key};
#[cfg(target_os = "macos")]
use rdev::set_is_main_thread;
use std::process;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

const OUTER_MARGIN: i32 = 32;
const INNER_SPACING: i32 = 18;

pub fn build() -> Box {
	let container = Box::new(Orientation::Vertical, INNER_SPACING);
	container.set_margin_top(OUTER_MARGIN);
	container.set_margin_bottom(OUTER_MARGIN);
	container.set_margin_start(OUTER_MARGIN);
	container.set_margin_end(OUTER_MARGIN);
	container.set_hexpand(true);
	container.set_vexpand(true);
	container.set_focusable(true);
	container.set_can_focus(true);

	let title = Label::new(Some("Event monitor"));
	title.add_css_class("title-3");
	title.set_xalign(0.0);
	container.append(&title);

	let info = Label::new(Some("Click here to start key capture."));
	info.set_xalign(0.0);
	info.set_wrap(true);
	let clicker = GestureClick::new();
	clicker.connect_pressed(|_, _, _, _| {
		start_global_key_monitor();
	});
	info.add_controller(clicker);
	container.append(&info);

	container
}

static KEY_MONITOR_ONCE: OnceLock<()> = OnceLock::new();

pub fn start_global_key_monitor() {
	KEY_MONITOR_ONCE.get_or_init(|| {
		thread::spawn(run_key_monitor);
	});
}

fn run_key_monitor() {
    #[cfg(target_os = "macos")]
    set_is_main_thread(false);

	let modifiers = Arc::new(Mutex::new(ModifierState::default()));
	let modifier_handle = Arc::clone(&modifiers);

	let callback = move |event: Event| -> Option<Event> {
		match event.event_type {
			EventType::KeyPress(key) => {
				println!("Key press   {:?} | text {:?}", key, event.name.as_deref());
				let mut state = modifier_handle
					.lock()
					.expect("modifier mutex poisoned");
				state.update(key, true);

				if state.ctrl_alt_active() && matches!(key, Key::Num0 | Key::Kp0) {
					println!("Detected Ctrl+Alt+0. Exiting.");
					process::exit(0);
				}
			}
			EventType::KeyRelease(key) => {
				println!("Key release {:?} | text {:?}", key, event.name.as_deref());
				modifier_handle
					.lock()
					.expect("modifier mutex poisoned")
					.update(key, false);
			}
			_ => {}
		}

		None
	};

	if let Err(error) = grab(callback) {
		eprintln!("Failed to grab input events: {error:?}");
	}
}

#[derive(Default)]
struct ModifierState {
	ctrl_left: bool,
	alt_left: bool,
}

impl ModifierState {
	fn update(&mut self, key: Key, pressed: bool) {
		match key {
			Key::ControlLeft => self.ctrl_left = pressed,
			Key::Alt => self.alt_left = pressed,
			_ => {}
		}
	}

	fn ctrl_alt_active(&self) -> bool {
		self.ctrl_left && self.alt_left
	}
}
