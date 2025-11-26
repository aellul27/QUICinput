use std::thread;

use quinn::{Connection, SendStream};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::quic::{open_uni, quic_runtime, send_data as send_quic_bytes};

pub enum QuicCommand {
    Mouse(Vec<u8>),
    Keyboard(Vec<u8>),
    Shutdown,
}

pub type QuicSender = UnboundedSender<QuicCommand>;

pub fn spawn_quic_helper(connection: Connection) -> QuicSender {
    let (tx, rx) = mpsc::unbounded_channel();
    // Run QUIC networking on a dedicated worker thread to avoid blocking the input grab callback.
    let _ = thread::spawn(move || run_quic_worker(connection, rx));
    tx
}

fn run_quic_worker(connection: Connection, mut rx: UnboundedReceiver<QuicCommand>) {
    quic_runtime().block_on(async move {
        let mut mouse_stream = match open_uni(connection.clone()).await {
            Ok(stream) => Some(stream),
            Err(error) => {
                eprintln!("failed to open mouse send stream: {error:?}");
                return;
            }
        };

        let mut keyboard_stream = match open_uni(connection).await {
            Ok(stream) => Some(stream),
            Err(error) => {
                eprintln!("failed to open keyboard send stream: {error:?}");
                return;
            }
        };

        while let Some(command) = rx.recv().await {
            match command {
                QuicCommand::Mouse(buf) => {
                    if let Some(stream) = mouse_stream.as_mut() {
                        if let Err(error) = send_quic_bytes(stream, &buf).await {
                            eprintln!("failed to send mouse data: {error:?}");
                            mouse_stream = None;
                        }
                    }
                }
                QuicCommand::Keyboard(buf) => {
                    if let Some(stream) = keyboard_stream.as_mut() {
                        if let Err(error) = send_quic_bytes(stream, &buf).await {
                            eprintln!("failed to send keyboard data: {error:?}");
                            keyboard_stream = None;
                        }
                    }
                }
                QuicCommand::Shutdown => {
                    finish_stream(mouse_stream.take());
                    finish_stream(keyboard_stream.take());
                    break;
                }
            }
        }

        finish_stream(mouse_stream.take());
        finish_stream(keyboard_stream.take());
    });
}

fn finish_stream(stream: Option<SendStream>) {
    if let Some(mut stream) = stream {
        let _ = stream.finish();
    }
}
