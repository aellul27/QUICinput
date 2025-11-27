use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    thread,
};

use quinn::{Endpoint, ServerConfig};
use rdev::EventType;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use shared::MouseMove;

mod simulator;

use simulator::EventSimulator;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    // server and client are running on the same thread asynchronously
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 4433);
    const MAX_CONNECTIONS: usize = 16;
    let simulator = Arc::new(EventSimulator::new());
    run_server(addr, MAX_CONNECTIONS, simulator).await
}

async fn run_server(
    addr: SocketAddr,
    max_connections: usize,
    simulator: Arc<EventSimulator>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let (endpoint, _server_cert) = make_server_endpoint(addr)?;
    println!("[server] listening on {} with max {} connections", addr, max_connections);

    let connection_limit = Arc::new(Semaphore::new(max_connections));

    while let Some(incoming) = endpoint.accept().await {
        let permit = match Arc::clone(&connection_limit).acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                eprintln!("[server] semaphore closed; shutting down accept loop");
                break;
            }
        };

        let simulator_for_connection = Arc::clone(&simulator);
        tokio::spawn(async move {
            handle_connection(incoming, permit, simulator_for_connection).await;
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

fn configure_server()
-> Result<(ServerConfig, CertificateDer<'static>), Box<dyn Error + Send + Sync + 'static>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());

    let server_config =
        ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;
    // let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();

    Ok((server_config, cert_der))
}

const MAX_STREAM_DATA: usize = 64 * 1024;

async fn handle_connection(
    incoming: quinn::Incoming,
    permit: OwnedSemaphorePermit,
    simulator: Arc<EventSimulator>,
) {
    match incoming.await {
        Ok(connection) => {
            println!(
                "[server] connection accepted: addr={}",
                connection.remote_address()
            );

            let bi_task = tokio::spawn(listen_bi_streams(connection.clone()));
            let uni_task = tokio::spawn(listen_uni_streams(connection.clone(), Arc::clone(&simulator)));
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

async fn listen_uni_streams(connection: quinn::Connection, simulator: Arc<EventSimulator>) {
    loop {
        match connection.accept_uni().await {
            Ok(recv) => {
                let handle = tokio::runtime::Handle::current();
                let simulator = Arc::clone(&simulator);
                thread::spawn(move || {
                    handle.block_on(async move {
                        handle_uni_stream(recv, simulator).await;
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
                // dropping `chunk` here returns capacity to flow control
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

async fn handle_uni_stream(mut recv: quinn::RecvStream, simulator: Arc<EventSimulator>) {
    let mut total = 0usize;

    loop {
        match recv.read_chunk(MAX_STREAM_DATA, true).await {
            Ok(Some(chunk)) => {
                total += chunk.bytes.len();
                if let Ok(mouse_move) = rmp_serde::from_slice::<MouseMove>(&chunk.bytes) {
                    println!(
                        "[server] uni stream mouse move: dx={:.3}, dy={:.3}",
                        mouse_move.dx,
                        mouse_move.dy
                    );
                } else if let Ok(event_type) = rmp_serde::from_slice::<EventType>(&chunk.bytes) {
                    match event_type {
                        EventType::MouseMove { .. } => {
                            println!("[server] uni stream event (mouse move)");
                        }
                        other => {
                            println!("[server] uni stream event: {:?}", other);
                            simulator.enqueue(other);
                        }
                    }
                } else {
                    println!(
                        "[server] uni stream unknown payload ({} bytes)",
                        chunk.bytes.len()
                    );
                }
                // chunk dropped here; grants window credit back to the peer
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
}

async fn send_bi_data(
    send: &mut quinn::SendStream,
    payload: &[u8],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    send.write_all(payload).await?;
    send.finish()?;
    Ok(())
}
