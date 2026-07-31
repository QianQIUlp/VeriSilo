#[allow(dead_code)]
#[path = "../src/https_server.rs"]
mod https_server;

use std::{
    io::{Read as _, Write as _},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use https_server::{
    serve_https_listener, ApplicationResponse, ConnectionAdmission, JsonRequestHandler,
};
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, StreamOwned,
};

const TEST_CA_PEM: &[u8] = include_bytes!("fixtures/transport-test-ca.pem");
const TEST_LEAF_PEM: &[u8] = include_bytes!("fixtures/transport-test-leaf.pem");
const TEST_LEAF_KEY_PEM: &[u8] = include_bytes!("fixtures/transport-test-leaf-key.pem");

struct RecordingHandler {
    handled: mpsc::Sender<()>,
    maintenance_ticks: Arc<AtomicUsize>,
}

impl JsonRequestHandler for RecordingHandler {
    fn handle_json(&mut self, bearer: Option<&str>, body: &[u8]) -> ApplicationResponse {
        assert!(bearer.is_none());
        assert_eq!(body, b"{}");
        self.handled.send(()).unwrap();
        ApplicationResponse::new(200, b"{}".to_vec())
    }

    fn maintenance_tick(&mut self) {
        self.maintenance_ticks.fetch_add(1, Ordering::AcqRel);
    }
}

fn server_tls_configuration() -> Arc<ServerConfig> {
    let certificates = CertificateDer::pem_slice_iter(TEST_LEAF_PEM)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let private_key = PrivateKeyDer::from_pem_slice(TEST_LEAF_KEY_PEM).unwrap();
    let mut configuration = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .unwrap();
    configuration.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(configuration)
}

fn client_tls_configuration() -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(TEST_CA_PEM) {
        roots.add(certificate.unwrap()).unwrap();
    }
    let mut configuration = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    configuration.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(configuration)
}

#[test]
fn connection_admission_is_bounded_and_released_by_raii() {
    let admission = Arc::new(ConnectionAdmission::new(2));
    let first = admission.try_acquire().unwrap();
    let second = admission.try_acquire().unwrap();
    assert_eq!(admission.active(), 2);
    assert!(admission.try_acquire().is_none());

    drop(first);
    let replacement = admission.try_acquire().unwrap();
    assert_eq!(admission.active(), 2);
    drop(second);
    drop(replacement);
    assert_eq!(admission.active(), 0);
}

#[test]
fn slow_tls_peer_does_not_block_accept_maintenance_or_ready_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let (handled_sender, handled_receiver) = mpsc::channel();
    let maintenance_ticks = Arc::new(AtomicUsize::new(0));
    let server_maintenance_ticks = maintenance_ticks.clone();
    let server = thread::spawn(move || {
        let mut handler = RecordingHandler {
            handled: handled_sender,
            maintenance_ticks: server_maintenance_ticks,
        };
        serve_https_listener(listener, server_tls_configuration(), &mut handler, || {
            server_stop.load(Ordering::Acquire)
        })
    });

    // This peer completes TCP connect but sends no TLS bytes. In the old
    // serial accept loop it occupied the only server thread for 15 seconds.
    let slow_peer = TcpStream::connect(address).unwrap();
    thread::sleep(Duration::from_millis(250));

    let fast_socket = TcpStream::connect(address).unwrap();
    fast_socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    fast_socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let server_name = ServerName::try_from("127.0.0.1").unwrap();
    let connection = ClientConnection::new(client_tls_configuration(), server_name).unwrap();
    let mut fast_peer = StreamOwned::new(connection, fast_socket);
    fast_peer
        .write_all(
            b"POST / HTTP/1.1\r\nHost: agent.example\r\nContent-Type: application/json\r\nX-VeriSilo-Protocol: 1\r\nContent-Length: 2\r\n\r\n{}",
        )
        .unwrap();
    fast_peer.flush().unwrap();

    handled_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("ready request was blocked behind the slow unauthenticated peer");
    assert!(
        maintenance_ticks.load(Ordering::Acquire) > 0,
        "maintenance did not run while a slow connection was in flight"
    );
    let mut response = Vec::new();
    fast_peer.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));

    drop(fast_peer);
    drop(slow_peer);
    stop.store(true, Ordering::Release);
    server.join().unwrap().unwrap();
}
