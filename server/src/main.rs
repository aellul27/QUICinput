use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    // server and client are running on the same thread asynchronously
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4433);
    const MAX_CONNECTIONS: usize = 16;
    run_server(addr, MAX_CONNECTIONS).await
}

async fn run_server(
    addr: SocketAddr,
    max_connections: usize,
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

        tokio::spawn(async move {
            handle_connection(incoming, permit).await;
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

async fn handle_connection(incoming: quinn::Incoming, permit: OwnedSemaphorePermit) {
    match incoming.await {
        Ok(connection) => {
            println!(
                "[server] connection accepted: addr={}",
                connection.remote_address()
            );

            let bi_task = tokio::spawn(listen_bi_streams(connection.clone()));
            let uni_task = tokio::spawn(listen_uni_streams(connection.clone()));

            if let Err(err) = bi_task.await {
                eprintln!("[server] bi stream task failed: {err}");
            }

            if let Err(err) = uni_task.await {
                eprintln!("[server] uni stream task failed: {err}");
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
                tokio::spawn(async move {
                    handle_bi_stream(send, recv).await;
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

async fn listen_uni_streams(connection: quinn::Connection) {
    loop {
        match connection.accept_uni().await {
            Ok(recv) => {
                tokio::spawn(async move {
                    handle_uni_stream(recv).await;
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
    match recv.read_to_end(MAX_STREAM_DATA).await {
        Ok(data) => {
            let message = String::from_utf8_lossy(&data);
            println!("[server] bi stream received: {message}");

            if let Err(err) = send.write_all(b"ack").await {
                eprintln!("[server] failed to send ack: {err}");
            }

            if let Err(err) = send.finish() {
                eprintln!("[server] failed to finish bi stream: {err}");
            }
        }
        Err(err) => {
            eprintln!("[server] failed to read bi stream: {err}");
        }
    }
}

async fn handle_uni_stream(mut recv: quinn::RecvStream) {
    match recv.read_to_end(MAX_STREAM_DATA).await {
        Ok(data) => {
            let message = String::from_utf8_lossy(&data);
            println!("[server] uni stream received: {message}");
        }
        Err(err) => {
            eprintln!("[server] failed to read uni stream: {err}");
        }
    }
}
