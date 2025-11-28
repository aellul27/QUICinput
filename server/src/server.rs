use std::{
    error::Error,
    net::SocketAddr,
    sync::Arc,
    thread,
};

use quinn::{Endpoint, Incoming, ServerConfig};
use rdev::EventType;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use shared::MouseMove;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::{
    mousemove::do_mouse_move,
    simulator::EventSimulator,
};

#[cfg(target_os = "linux")]
use std::sync::Mutex;

#[cfg(target_os = "linux")]
pub(crate) fn ensure_uinput_available() {
    use std::process::Command;

    let output = match Command::new("lsmod").output() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("[server] failed to run lsmod: {error}");
            std::process::exit(1);
        }
    };

    let modules = String::from_utf8_lossy(&output.stdout);
    let has_uinput = modules
        .lines()
        .any(|line| line.split_whitespace().next() == Some("uinput"));

    if !has_uinput {
        eprintln!(
            "[server] kernel module 'uinput' is not loaded. Please enable it (e.g., 'sudo modprobe uinput') and ensure this program has permission to access /dev/uinput."
        );
        std::process::exit(1);
    }
}

pub(crate) type Simulators = Arc<[EventSimulator; 2]>;

#[cfg(target_os = "linux")]
pub(crate) type DeviceInput = Arc<Mutex<Option<uinput::Device>>>;
#[cfg(not(target_os = "linux"))]
pub(crate) type DeviceInput = ();

pub(crate) async fn run_server(
    addr: SocketAddr,
    max_connections: u8,
    simulators: Simulators,
    device_input: DeviceInput,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let (endpoint, _server_cert) = make_server_endpoint(addr)?;
    println!(
        "[server] listening on {} with max {} connections",
        addr, max_connections
    );

    let connection_limit = Arc::new(Semaphore::new(max_connections.into()));

    while let Some(incoming) = endpoint.accept().await {
        let permit = match Arc::clone(&connection_limit).acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                eprintln!("[server] semaphore closed; shutting down accept loop");
                break;
            }
        };

        let simulators_for_connection = Arc::clone(&simulators);
        let device_for_connection = device_input.clone();
        tokio::spawn(async move {
            handle_connection(
                incoming,
                permit,
                simulators_for_connection,
                device_for_connection,
            )
            .await;
        });
    }

    Ok(())
}

fn make_server_endpoint(
    bind_addr: SocketAddr,
) -> Result<(Endpoint, CertificateDer<'static>), Box<dyn Error + Send + Sync + 'static>> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

fn configure_server() -> Result<
    (ServerConfig, CertificateDer<'static>),
    Box<dyn Error + Send + Sync + 'static>,
> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());

    let server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;

    Ok((server_config, cert_der))
}

const MAX_STREAM_DATA: usize = 64 * 1024;

async fn handle_connection(
    incoming: Incoming,
    permit: OwnedSemaphorePermit,
    simulators: Simulators,
    device_input: DeviceInput,
) {
    match incoming.await {
        Ok(connection) => {
            println!(
                "[server] connection accepted: addr={}",
                connection.remote_address()
            );

            let bi_task = tokio::spawn(listen_bi_streams(connection.clone()));
            let uni_task = tokio::spawn(listen_uni_streams(
                connection.clone(),
                Arc::clone(&simulators),
                device_input,
            ));
            let close_task = tokio::spawn(async move {
                let reason = connection.closed().await;
                match reason {
                    quinn::ConnectionError::ApplicationClosed { .. } => {
                        println!("[server] connection closed by peer");
                    }
                    quinn::ConnectionError::LocallyClosed => {
                        println!("[server] connection closed locally");
                    }
                    err => {
                        eprintln!("[server] connection closed with error: {err}");
                    }
                }
            });

            if let Err(err) = bi_task.await {
                eprintln!("[server] bi stream task failed: {err}");
            }

            if let Err(err) = uni_task.await {
                eprintln!("[server] uni stream task failed: {err}");
            }

            if let Err(err) = close_task.await {
                eprintln!("[server] connection close task failed: {err}");
            }
        }
        Err(err) => {
            eprintln!("[server] failed to establish connection: {err}");
        }
    }

    drop(permit);
}

async fn listen_bi_streams(connection: quinn::Connection) {
    loop {
        match connection.accept_bi().await {
            Ok((send, recv)) => {
                let handle = tokio::runtime::Handle::current();
                thread::spawn(move || {
                    handle.block_on(async move {
                        handle_bi_stream(send, recv).await;
                    });
                });
            }
            Err(quinn::ConnectionError::ApplicationClosed { .. })
            | Err(quinn::ConnectionError::LocallyClosed) => {
                break;
            }
            Err(err) => {
                eprintln!("[server] bi stream error: {err}");
                break;
            }
        }
    }
}

async fn listen_uni_streams(
    connection: quinn::Connection,
    simulators: Simulators,
    device_input: DeviceInput,
) {
    loop {
        match connection.accept_uni().await {
            Ok(recv) => {
                let handle = tokio::runtime::Handle::current();
                let simulators = Arc::clone(&simulators);
                let device_input = device_input.clone();
                thread::spawn(move || {
                    handle.block_on(async move {
                        handle_uni_stream(recv, simulators, device_input).await;
                    });
                });
            }
            Err(quinn::ConnectionError::ApplicationClosed { .. })
            | Err(quinn::ConnectionError::LocallyClosed) => {
                break;
            }
            Err(err) => {
                eprintln!("[server] uni stream error: {err}");
                break;
            }
        }
    }
}

async fn handle_bi_stream(mut send: quinn::SendStream, mut recv: quinn::RecvStream) {
    let mut total = 0usize;

    loop {
        match recv.read_chunk(MAX_STREAM_DATA, true).await {
            Ok(Some(chunk)) => {
                total += chunk.bytes.len();
                let message = String::from_utf8_lossy(&chunk.bytes);
                println!(
                    "[server] bi stream chunk ({} bytes): {message}",
                    chunk.bytes.len()
                );
            }
            Ok(None) => {
                println!("[server] bi stream closed after {total} bytes");
                break;
            }
            Err(err) => {
                eprintln!("[server] failed to read bi stream: {err}");
                return;
            }
        }
    }

    if let Err(err) = send_bi_data(&mut send, b"ack").await {
        eprintln!("[server] failed to reply on bi stream: {err}");
    }
}

async fn handle_uni_stream(
    mut recv: quinn::RecvStream,
    simulators: Simulators,
    device_input: DeviceInput,
) {
    let mut total = 0usize;

    loop {
        match recv.read_chunk(MAX_STREAM_DATA, true).await {
            Ok(Some(chunk)) => {
                total += chunk.bytes.len();
                if let Ok(mouse_move) = rmp_serde::from_slice::<MouseMove>(&chunk.bytes) {
                    #[cfg(target_os = "linux")]
                    {
                        match device_input.lock() {
                            Ok(mut maybe_device) => {
                                if let Some(device) = maybe_device.as_mut() {
                                    if let Err(err) = do_mouse_move(device, mouse_move) {
                                        eprintln!("[server] failed to emit mouse move: {err}");
                                    }
                                } else {
                                    eprintln!("[server] virtual mouse not available; dropping MouseMove");
                                }
                            }
                            Err(poisoned) => {
                                eprintln!("[server] virtual mouse mutex poisoned: {poisoned}");
                            }
                        }
                    }

                    #[cfg(not(target_os = "linux"))]
                    {
                        let _ = device_input;
                        do_mouse_move(&simulators[1], mouse_move);
                    }
                } else if let Ok(event_type) = rmp_serde::from_slice::<EventType>(&chunk.bytes) {
                    match event_type {
                        EventType::ButtonPress(..)
                        | EventType::ButtonRelease(..)
                        | EventType::Wheel { .. } => {
                            simulators[1].enqueue(event_type);
                        }
                        _other => {
                            simulators[0].enqueue(event_type);
                        }
                    }
                } else {
                    println!(
                        "[server] uni stream unknown payload ({} bytes)",
                        chunk.bytes.len()
                    );
                }
            }
            Ok(None) => {
                println!("[server] uni stream closed after {total} bytes");
                break;
            }
            Err(err) => {
                eprintln!("[server] failed to read uni stream: {err}");
                break;
            }
        }
    }
    simulators[0].enqueue(EventType::KeyRelease(rdev::Key::ControlLeft));
    simulators[0].enqueue(EventType::KeyRelease(rdev::Key::Alt));
    simulators[0].enqueue(EventType::KeyRelease(rdev::Key::Num0));
}

async fn send_bi_data(
    send: &mut quinn::SendStream,
    payload: &[u8],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    send.write_all(payload).await?;
    send.finish()?;
    Ok(())
}
