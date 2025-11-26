use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig};
use quinn::crypto::rustls::QuicClientConfig;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio::runtime::{Builder, Runtime};

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn quic_runtime() -> &'static Runtime {
    TOKIO_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("quic-client-runtime")
            .build()
            .expect("Failed to build Tokio runtime")
    })
}

pub async fn run_client(
    server_addr: SocketAddr,
) -> Result<(Endpoint, Connection), Box<dyn Error + Send + Sync + 'static>> {
    println!("Attempting");
    let mut endpoint = Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;

    let rustls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    let mut client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(rustls_config)?));

    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
    client_config.transport_config(Arc::new(transport_config));

    endpoint.set_default_client_config(client_config);
    // connect to server
    let connection = endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync + 'static>)?;
    println!("[client] connected: addr={}", connection.remote_address());
    

    Ok((endpoint, connection))
}

pub async fn open_bi(
    connection: Connection
) -> Result<(SendStream, RecvStream), Box<dyn Error + Send + Sync + 'static>> {
    let (send, recv) = connection
        .open_bi()
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync + 'static>)?;
    Ok((send, recv))
}


pub async fn open_uni(
    connection: Connection
) -> Result<SendStream, Box<dyn Error + Send + Sync + 'static>> {
    let send = connection
        .open_uni()
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync + 'static>)?;
    Ok(send)
}

pub async fn send_data(
    send_stream: &mut SendStream,
    request: &[u8],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    send_stream
        .write_all(request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync + 'static>)?;
    Ok(())
}

pub async fn recieve_data(
    mut recv_stream: RecvStream,
) -> Result<Vec<u8>, Box<dyn Error + Send + Sync + 'static>> {
    let resp = recv_stream
        .read_to_end(usize::MAX)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync + 'static>)?;
    Ok(resp)
}

pub async fn close_client(
    connection: Connection,
    endpoint: Endpoint
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    connection.close(0u32.into(), b"done");
    // Give the server a fair chance to receive the close packet
    endpoint.wait_idle().await;
    Ok(())
}

#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}