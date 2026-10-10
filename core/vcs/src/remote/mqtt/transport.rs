// SPDX-License-Identifier: MIT
//! The connection to a broker: TCP (`std::net`), and TLS with rustls
//! (crypto: ring), the broker's certificate checked by rustls-webpki
//! against the system's root certificates (rustls-native-certs) and those
//! of a certificate authority file the user names.
//!
//! Blocking sockets with timeouts: a read that waits longer than the read
//! timeout gives [`io::ErrorKind::WouldBlock`] or `TimedOut` (Windows), and
//! the caller reads again; TLS keeps what it has read of a record.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use mitcad_model::file::settings::BrokerAddress;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use super::{LiveError, LiveErrorClass};

/// A connection's bytes, plain or in TLS.
pub enum Transport {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

/// The root certificates of the system, read once per process.
fn system_roots() -> &'static (RootCertStore, Vec<String>) {
    static ROOTS: OnceLock<(RootCertStore, Vec<String>)> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let found = rustls_native_certs::load_native_certs();
        let mut roots = RootCertStore::empty();
        roots.add_parsable_certificates(found.certs);
        let errors = found.errors.iter().map(ToString::to_string).collect();
        (roots, errors)
    })
}

/// The certificates of a PEM file of certificate authorities (a broker
/// with a certificate authority of its own).
pub fn read_authorities(path: &Path) -> Result<Vec<CertificateDer<'static>>, LiveError> {
    let bad = |why: String| {
        LiveError::new(
            LiveErrorClass::Invalid,
            format!(
                "cannot read the certificate authorities in {}: {why}",
                path.display()
            ),
        )
    };
    let metadata = std::fs::metadata(path).map_err(|e| bad(e.to_string()))?;
    if metadata.len() > 1 << 20 {
        return Err(bad("the file is larger than 1 MiB".to_owned()));
    }
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|e| bad(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| bad(e.to_string()))?;
    if certificates.is_empty() {
        return Err(bad("no certificates in it".to_owned()));
    }
    Ok(certificates)
}

/// The TLS settings for a connection: the system's roots and `extra` ones.
fn tls_config(extra: &[CertificateDer<'static>]) -> Result<Arc<ClientConfig>, LiveError> {
    let (system, errors) = system_roots();
    let mut roots = system.clone();
    let (added, _) = roots.add_parsable_certificates(extra.iter().cloned());
    if extra.len() > added {
        return Err(LiveError::new(
            LiveErrorClass::Invalid,
            "a certificate authority given is not a usable certificate",
        ));
    }
    if roots.is_empty() {
        let mut message = "no root certificates were found on this computer".to_owned();
        if let Some(error) = errors.first() {
            message.push_str(&format!(" ({error})"));
        }
        return Err(LiveError::new(LiveErrorClass::Certificate, message));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| LiveError::new(LiveErrorClass::Tls, e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Random bytes from the operating system (ring's generator).
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    if rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut bytes)
        .is_err()
    {
        // Without the system's generator (it does not fail on the systems
        // Mitcad runs on), the time: ids stay distinct, not secret.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = (nanos >> ((i % 16) * 8)) as u8;
        }
    }
    bytes
}

/// Lowercase hexadecimal of bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Transport {
    /// Connects to `broker` within `timeout` per address (and as long again
    /// for the TLS handshake). `authorities`: certificate authorities
    /// trusted besides the system's.
    pub fn open(
        broker: &BrokerAddress,
        authorities: &[CertificateDer<'static>],
        timeout: Duration,
    ) -> Result<Transport, LiveError> {
        let config = broker.tls.then(|| tls_config(authorities)).transpose()?;
        let addresses = (broker.host.as_str(), broker.port)
            .to_socket_addrs()
            .map_err(|e| {
                LiveError::new(
                    LiveErrorClass::Network,
                    format!("cannot find the broker {}: {e}", broker.host),
                )
            })?;
        let mut last = None;
        let mut socket = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => {
                    socket = Some(stream);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let socket = socket.ok_or_else(|| match last {
            Some(e) => io_error(&format!("cannot connect to {broker}"), &e),
            None => LiveError::new(
                LiveErrorClass::Network,
                format!("the broker {} has no address", broker.host),
            ),
        })?;
        socket
            .set_nodelay(true)
            .and_then(|()| socket.set_read_timeout(Some(timeout)))
            .and_then(|()| socket.set_write_timeout(Some(timeout)))
            .map_err(|e| io_error("cannot set up the connection", &e))?;
        let Some(config) = config else {
            return Ok(Transport::Plain(socket));
        };
        let name = ServerName::try_from(broker.host.clone()).map_err(|_| {
            LiveError::new(
                LiveErrorClass::Invalid,
                format!("{} is not a host name for TLS", broker.host),
            )
        })?;
        let connection = ClientConnection::new(config, name)
            .map_err(|e| LiveError::new(LiveErrorClass::Tls, e.to_string()))?;
        let mut stream = StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream
                .conn
                .complete_io(&mut stream.sock)
                .map_err(|e| io_error(&format!("the TLS handshake with {broker} failed"), &e))?;
        }
        Ok(Transport::Tls(Box::new(stream)))
    }

    pub fn is_tls(&self) -> bool {
        matches!(self, Transport::Tls(_))
    }

    fn socket(&self) -> &TcpStream {
        match self {
            Transport::Plain(socket) => socket,
            Transport::Tls(stream) => &stream.sock,
        }
    }

    pub fn set_read_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.socket().set_read_timeout(Some(timeout))
    }

    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(socket) => socket.read(buffer),
            Transport::Tls(stream) => stream.read(buffer),
        }
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            Transport::Plain(socket) => socket.write_all(data),
            Transport::Tls(stream) => {
                stream.write_all(data)?;
                stream.flush()
            }
        }
    }

    /// Ends the connection politely: TLS's close_notify, the end of the
    /// sending side, then what still comes is read until the broker closes
    /// its side (at most two seconds). A socket closed with unread data
    /// would end with a reset, which can make the broker lose the last
    /// packets (the DISCONNECT: it would publish the will).
    pub fn close(&mut self) {
        if let Transport::Tls(stream) = self {
            stream.conn.send_close_notify();
            let _ = stream.flush();
        }
        let mut socket = self.socket();
        let _ = socket.shutdown(Shutdown::Write);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match socket.read(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => break,
            }
        }
    }

    /// Ends the socket at once, as if the process had died (tests: the
    /// broker then publishes the will).
    #[cfg(test)]
    pub fn kill(&mut self) {
        let _ = self.socket().shutdown(Shutdown::Both);
    }
}

/// Whether a read only waited longer than the timeout.
pub fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// A failure of the socket or of TLS, classified.
pub fn io_error(what: &str, error: &io::Error) -> LiveError {
    if let Some(tls) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        return tls_error(what, tls);
    }
    let class = match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => LiveErrorClass::TimedOut,
        _ => LiveErrorClass::Network,
    };
    LiveError::new(class, format!("{what}: {error}"))
}

fn tls_error(what: &str, error: &rustls::Error) -> LiveError {
    use rustls::CertificateError as C;
    let (class, why) = match error {
        rustls::Error::InvalidCertificate(certificate) => (
            LiveErrorClass::Certificate,
            match certificate {
                C::UnknownIssuer => {
                    "the broker's certificate is not signed by an authority this computer trusts"
                        .to_owned()
                }
                C::NotValidForName | C::NotValidForNameContext { .. } => {
                    "the broker's certificate is for another host name".to_owned()
                }
                C::Expired | C::ExpiredContext { .. } => {
                    "the broker's certificate has expired".to_owned()
                }
                C::NotValidYet | C::NotValidYetContext { .. } => {
                    "the broker's certificate is not valid yet".to_owned()
                }
                C::Revoked => "the broker's certificate was revoked".to_owned(),
                other => format!("the broker's certificate is not accepted ({other:?})"),
            },
        ),
        other => (LiveErrorClass::Tls, other.to_string()),
    };
    LiveError::new(class, format!("{what}: {why}"))
}
