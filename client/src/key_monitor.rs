use rdev::{grab, Event, EventType, Key};
#[cfg(target_os = "macos")]
use rdev::set_is_main_thread;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn start_global_key_monitor() {
    let already_running = MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err();
    if already_running {
        println!("Global key monitor already running");
        return;
    }

    thread::spawn(|| {
        let result = panic::catch_unwind(run_key_monitor);
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

struct MonitorStop;

fn run_key_monitor() {
    #[cfg(target_os = "macos")]
    set_is_main_thread(false);

    let modifiers = Arc::new(Mutex::new(ModifierState::default()));
    let modifier_handle = Arc::clone(&modifiers);

    let callback = move |event: Event| -> Option<Event> {
        println!("Event: {:?} | text {:?}", event.event_type, event.name.as_deref());
        println!("{:?}", rmp_serde::to_vec(&event.event_type));
        match event.event_type {
            EventType::KeyPress(key) => {
                println!("Key press   {:?} | text {:?}", key, event.name.as_deref());
                let mut state = modifier_handle
                    .lock()
                    .expect("modifier mutex poisoned");
                state.update(key, true);

                if state.ctrl_alt_active() && matches!(key, Key::Num0 | Key::Kp0) {
                    println!("Detected Ctrl+Alt+0. Stopping key monitor.");
                    request_monitor_stop();
                    return None;
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

fn request_monitor_stop() {
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
