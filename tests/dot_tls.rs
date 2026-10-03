//! Local TLS servers exercise real certificate verification and DNS framing.
use nexus_net::dot;
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
        RootCertStore,
    },
    TlsAcceptor,
};

struct Fixture {
    dir: PathBuf,
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    ca: CertificateDer<'static>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "nexus-dot-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let run = |args: &[&str]| {
            let output = std::process::Command::new("openssl")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "2",
            "-subj",
            "/CN=Nexus Test CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
        ]);
        run(&[
            "req",
            "-new",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
            "-subj",
            "/CN=dot.test",
        ]);
        std::fs::write(dir.join("extensions"),"subjectAltName=DNS:dot.test\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n").unwrap();
        run(&[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-out",
            "server.pem",
            "-days",
            "2",
            "-extfile",
            "extensions",
        ]);
        let cert = CertificateDer::from_pem_file(dir.join("server.pem")).unwrap();
        let key = PrivateKeyDer::from_pem_file(dir.join("server.key")).unwrap();
        let ca = CertificateDer::from_pem_file(dir.join("ca.pem")).unwrap();
        Self { dir, cert, key, ca }
    }
    fn roots(&self) -> RootCertStore {
        let mut roots = RootCertStore::empty();
        roots.add(self.ca.clone()).unwrap();
        roots
    }
    async fn server(&self, rcode: u8) -> (u16, tokio::task::JoinHandle<()>) {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![self.cert.clone()], self.key.clone_key())
        .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let Ok(mut stream) = acceptor.accept(socket).await else {
                return;
            };
            let Ok(n) = stream.read_u16().await else {
                return;
            };
            let mut query = vec![0; n as usize];
            stream.read_exact(&mut query).await.unwrap();
            query[2] = 0x81;
            query[3] = 0x80 | rcode;
            if rcode == 0 {
                query[7] = 1;
                query.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1]);
            }
            stream.write_u16(query.len() as u16).await.unwrap();
            stream.write_all(&query).await.unwrap();
            stream.flush().await.unwrap();
        });
        (port, task)
    }
}

#[tokio::test]
async fn dot_verifies_tls_and_receives_a_real_dns_reply() {
    if !nexus_net::command::available("openssl") {
        return;
    }
    let f = Fixture::new();
    let (port, server) = f.server(0).await;
    let sample = dot::probe("127.0.0.1", "dot.test", "example.test", port, f.roots())
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!((sample.rcode, sample.answers), (0, 1));
    assert!(sample.tls_ms > 0.0);
}

#[tokio::test]
async fn dot_distinguishes_dns_servfail_from_tls_failure() {
    if !nexus_net::command::available("openssl") {
        return;
    }
    let f = Fixture::new();
    let (port, server) = f.server(2).await;
    let sample = dot::probe("127.0.0.1", "dot.test", "example.test", port, f.roots())
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(sample.rcode, 2);
    let (port, server) = f.server(0).await;
    let failure = dot::probe("127.0.0.1", "wrong.test", "example.test", port, f.roots())
        .await
        .err()
        .unwrap();
    server.await.unwrap();
    assert_eq!(failure.stage, "TLS");
    let (port, server) = f.server(0).await;
    let failure = dot::probe(
        "127.0.0.1",
        "dot.test",
        "example.test",
        port,
        RootCertStore::empty(),
    )
    .await
    .err()
    .unwrap();
    server.await.unwrap();
    assert_eq!(failure.stage, "TLS");
}
