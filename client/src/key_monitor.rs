use rdev::{grab, simulate, Event, EventType, Key};
#[cfg(target_os = "macos")]
use rdev::set_is_main_thread;
use shared::MouseMove;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self};
use quinn::{Connection, Endpoint};
use crate::quic_helper_thread::{spawn_quic_helper, QuicCommand, QuicSender};

static IGNORE_MOUSE: AtomicBool = AtomicBool::new(false);


use crate::input::{input_ungrabbed};
use crate::windowresolution::{find_window_size};

static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn start_global_key_monitor(endpoint: Endpoint, connection: Connection) {
    let already_running = MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err();
    if already_running {
        println!("Global key monitor already running");
        return;
    }

    thread::spawn(move || {
        let endpoint_for_run = endpoint.clone();
        let connection_for_run = connection.clone();
        let result = panic::catch_unwind(AssertUnwindSafe(move || {
            run_key_monitor(endpoint_for_run, connection_for_run);
        }));
        MONITOR_RUNNING.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => println!("Global key monitor stopped"),
            Err(err) => {
                if err.downcast_ref::<MonitorStop>().is_some() {
                    println!("Global key monitor stopped");
                } else {
                    panic::resume_unwind(err);
                }
            }
        }
    });
}

fn send_data(quic_sender: &mut Option<QuicSender>, command: QuicCommand) {
    let send_result = quic_sender
        .as_ref()
        .map(|sender| sender.send(command));
    if matches!(send_result, Some(Err(_))) {
        *quic_sender = None;
    }
}

struct MonitorStop;

fn run_key_monitor(_endpoint: Endpoint, connection: Connection) {
    #[cfg(target_os = "macos")]
    set_is_main_thread(false);

    let mut quic_sender = Some(spawn_quic_helper(connection));

    let (middle_y, middle_x) = find_window_size();
    let _ = simulate(&EventType::MouseMove { x: middle_x, y: middle_y});

    let modifiers = Arc::new(Mutex::new(ModifierState::default()));
    let modifier_handle = Arc::clone(&modifiers);

    let callback = move |event: Event| -> Option<Event> {
        match event.event_type {
            EventType::KeyPress(key) => {
                let buf = rmp_serde::to_vec(&event.event_type).expect("failed to serialise");
                send_data(&mut quic_sender, QuicCommand::Keyboard(buf));
                let mut state = modifier_handle
                    .lock()
                    .expect("modifier mutex poisoned");
                state.update(key, true);

                if state.ctrl_alt_active() && matches!(key, Key::Num0 | Key::Kp0) {
                    println!("Detected Ctrl+Alt+0. Stopping key monitor.");
                    if let Some(sender) = quic_sender.take() {
                        let _ = sender.send(QuicCommand::Shutdown);
                    }
                    request_monitor_stop();
                    return None;
                }
                return None
            }
            EventType::KeyRelease(key) => {
                let buf = rmp_serde::to_vec(&event.event_type).expect("failed to serialise");
                send_data(&mut quic_sender, QuicCommand::Keyboard(buf));
                modifier_handle
                    .lock()
                    .expect("modifier mutex poisoned")
                    .update(key, false);
                return None
            }
            EventType::MouseMove { x, y } => {
                // Ignore the event triggered by simulate()
                if IGNORE_MOUSE.swap(false, Ordering::SeqCst) {
                    return None; // Swallow simulated event
                }

                let data = MouseMove {dx: (x - middle_x), dy: (y - middle_y) };
                let buf = rmp_serde::to_vec(&data).expect("failed to serialise");
                send_data(&mut quic_sender, QuicCommand::Mouse(buf));

                // Mark next mouse event as simulated
                IGNORE_MOUSE.store(true, Ordering::SeqCst);

                let _ = simulate(&EventType::MouseMove { x: middle_x, y: middle_y });
            }
            EventType::ButtonPress(..) | EventType::ButtonRelease(..) => {
                let buf = rmp_serde::to_vec(&event.event_type).expect("failed to serialise");
                send_data(&mut quic_sender, QuicCommand::Mouse(buf));
            }
            EventType::Wheel { delta_x, delta_y } => {
                if delta_x != 0 || delta_y != 0 {
                    let buf = rmp_serde::to_vec(&event.event_type).expect("failed to serialise");
                    send_data(&mut quic_sender, QuicCommand::Mouse(buf));
                }
            }
        }

        Some(event)
    };

    if let Err(error) = grab(callback) {
        eprintln!("Failed to grab input events: {error:?}");
    }
}

fn request_monitor_stop() {
    glib::MainContext::default().invoke(|| {
        input_ungrabbed();
    });
    #[cfg(target_os = "macos")]
    macos_run_loop::stop_current();

    #[cfg(not(target_os = "macos"))]
    panic::panic_any(MonitorStop);
}

#[cfg(target_os = "macos")]
mod macos_run_loop {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopStop(run_loop: *mut c_void);
    }

    pub fn stop_current() {
        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            if !run_loop.is_null() {
                CFRunLoopStop(run_loop);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_run_loop {}

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
