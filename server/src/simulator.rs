use rdev::{simulate, EventType};
use std::sync::mpsc::{self, Sender};
use std::thread;

pub struct EventSimulator {
    sender: Sender<EventType>,
}

impl EventSimulator {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<EventType>();

        thread::Builder::new()
            .name("event-simulator".into())
            .spawn(move || {
                for event in receiver {
                    if let Err(error) = simulate(&event) {
                        eprintln!("[server] failed to simulate event: {error:?}");
                    }
                }
            })
            .expect("failed to spawn event simulator thread");

        Self { sender }
    }

    pub fn enqueue(&self, event: EventType) {
        if let Err(error) = self.sender.send(event) {
            eprintln!("[server] failed to enqueue event for simulation: {error}");
        }
    }
}
