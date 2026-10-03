//! DNS-over-TLS: real DNS framing, native CA trust and strict identity validation.
use crate::{
    diagnosis::{Finding, Severity},
    model::ToolResult,
};
use anyhow::{bail, Context, Result};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{lookup_host, TcpStream},
    time::timeout,
};
use tokio_rustls::{
    rustls::{self, pki_types::ServerName, RootCertStore},
    TlsConnector,
};

#[derive(Debug)]
pub struct DotFailure {
    pub stage: &'static str,
    pub detail: String,
}
impl std::fmt::Display for DotFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage, self.detail)
    }
}
impl std::error::Error for DotFailure {}
fn failure(stage: &'static str, detail: impl ToString) -> DotFailure {
    DotFailure {
        stage,
        detail: detail.to_string(),
    }
}

pub struct DotSample {
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub query_ms: f64,
    pub rcode: u8,
    pub answers: u16,
    pub protocol: String,
}

fn packet(name: &str, id: u16) -> Result<Vec<u8>> {
    let name = name.trim_end_matches('.');
    if name.is_empty() || name.len() > 253 {
        bail!("Enter a DNS name of at most 253 ASCII characters");
    }
    let mut out = vec![(id >> 8) as u8, id as u8, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("DNS labels must contain 1–63 ASCII letters, digits, hyphens or underscores (use punycode for IDNs)");
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.extend_from_slice(&[0, 0, 1, 0, 1]); // A, IN
    Ok(out)
}

fn question_name(data: &[u8], start: usize) -> Result<(String, usize)> {
    let mut pos = start;
    let mut end = None;
    let mut labels = Vec::new();
    for _ in 0..128 {
        let length = *data.get(pos).context("Truncated DNS question")?;
        if length & 0xc0 == 0xc0 {
            let low = *data.get(pos + 1).context("Truncated DNS pointer")?;
            end.get_or_insert(pos + 2);
            pos = (((length & 0x3f) as usize) << 8) | low as usize;
            continue;
        }
        if length & 0xc0 != 0 {
            bail!("Invalid DNS label encoding");
        }
        pos += 1;
        if length == 0 {
            return Ok((labels.join("."), end.unwrap_or(pos)));
        }
        let label = data
            .get(pos..pos + length as usize)
            .context("Truncated DNS label")?;
        labels.push(std::str::from_utf8(label)?.to_ascii_lowercase());
        pos += length as usize;
        if labels.iter().map(String::len).sum::<usize>() + labels.len() > 254 {
            bail!("DNS name exceeds wire limit");
        }
    }
    bail!("DNS compression loop or excessive name length")
}

fn response(data: &[u8], id: u16, name: &str) -> Result<(u8, u16)> {
    if data.len() < 12 {
        bail!("DNS response shorter than its header");
    }
    if u16::from_be_bytes([data[0], data[1]]) != id {
        bail!("DNS transaction ID mismatch");
    }
    if data[2] & 0x80 == 0 || data[2] & 0x78 != 0 {
        bail!("Unexpected DNS response flags/opcode");
    }
    if data[2] & 2 != 0 {
        bail!("Truncated DNS response over TLS");
    }
    if data[4..6] != [0, 1] {
        bail!("DNS response must echo one question");
    }
    let (question, end) = question_name(data, 12)?;
    if question != name.trim_end_matches('.').to_ascii_lowercase()
        || data.get(end..end + 4) != Some(&[0, 1, 0, 1])
    {
        bail!("DNS response question mismatch");
    }
    let answers = u16::from_be_bytes([data[6], data[7]]);
    // Validate the complete record section before trusting its counts.
    let count = answers as usize
        + u16::from_be_bytes([data[8], data[9]]) as usize
        + u16::from_be_bytes([data[10], data[11]]) as usize;
    let mut pos = end + 4;
    for _ in 0..count {
        let (_, next) = question_name(data, pos)?;
        let header = data
            .get(next..next + 10)
            .context("Truncated DNS resource record")?;
        let length = u16::from_be_bytes([header[8], header[9]]) as usize;
        pos = next + 10 + length;
        if pos > data.len() {
            bail!("Truncated DNS resource data");
        }
    }
    if pos != data.len() {
        bail!("Unexpected trailing DNS response data");
    }
    Ok((data[3] & 15, answers))
}

pub async fn probe(
    server: &str,
    identity: &str,
    query: &str,
    port: u16,
    roots: RootCertStore,
) -> std::result::Result<DotSample, DotFailure> {
    let id = 0x4e58; // One query on a fresh authenticated TLS connection.
    let request = packet(query, id).map_err(|e| failure("Input", e))?;
    let tls_name = ServerName::try_from(identity.to_owned()).map_err(|e| failure("Input", e))?;
    let addresses = timeout(Duration::from_secs(4), lookup_host((server, port)))
        .await
        .map_err(|_| failure("Resolve", "Resolver address lookup timed out"))?
        .map_err(|e| failure("Resolve", e))?
        .take(8)
        .collect::<Vec<_>>();
    let start = Instant::now();
    let connect = async {
        let mut last = None;
        for address in addresses {
            match TcpStream::connect(address).await {
                Ok(stream) => return Ok(stream),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::other("No resolver addresses")))
    };
    let socket = timeout(Duration::from_secs(5), connect)
        .await
        .map_err(|_| failure("TCP", "Connection timed out; port may be filtered"))?
        .map_err(|e| failure("TCP", e))?;
    let connect_ms = start.elapsed().as_secs_f64() * 1000.0;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| failure("Trust", e))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let start = Instant::now();
    let mut stream = timeout(Duration::from_secs(5), connector.connect(tls_name, socket))
        .await
        .map_err(|_| failure("TLS", "Handshake timed out"))?
        .map_err(|e| failure("TLS", e))?;
    let tls_ms = start.elapsed().as_secs_f64() * 1000.0;
    let protocol = stream
        .get_ref()
        .1
        .protocol_version()
        .map(|p| format!("{p:?}"))
        .unwrap_or_default();
    let start = Instant::now();
    let work = async {
        stream.write_u16(request.len() as u16).await?;
        stream.write_all(&request).await?;
        stream.flush().await?;
        let length = stream.read_u16().await? as usize;
        if !(12..=65535).contains(&length) {
            return Err(std::io::Error::other("Invalid DNS frame length"));
        }
        let mut data = vec![0; length];
        stream.read_exact(&mut data).await?;
        Ok::<_, std::io::Error>(data)
    };
    let data = timeout(Duration::from_secs(5), work)
        .await
        .map_err(|_| failure("DNS", "TLS connected but the DNS query timed out"))?
        .map_err(|e| failure("DNS", e))?;
    let query_ms = start.elapsed().as_secs_f64() * 1000.0;
    let (rcode, answers) = response(&data, id, query).map_err(|e| failure("Protocol", e))?;
    Ok(DotSample {
        connect_ms,
        tls_ms,
        query_ms,
        rcode,
        answers,
        protocol,
    })
}

pub async fn inspect(server: &str, identity: &str, query: &str, port: u16) -> Result<ToolResult> {
    crate::tools::validate_host(server)?;
    crate::tools::validate_host(identity)?;
    packet(query, 0)?;
    if port == 0 {
        bail!("Resolver port must be 1–65535");
    }
    let mut result = ToolResult {
        title: "DNS-over-TLS inspector".into(),
        at: chrono::Utc::now(),
        columns: vec![
            "Status".into(),
            "Finding".into(),
            "Evidence".into(),
            "Next step".into(),
        ],
        ..Default::default()
    };
    result
        .metrics
        .insert("Resolver".into(), format!("{server}:{port}"));
    result
        .metrics
        .insert("TLS identity".into(), identity.into());
    let certs = tokio::task::spawn_blocking(rustls_native_certs::load_native_certs).await?;
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(certs.certs);
    let outcome = if roots.is_empty() {
        Err(failure("Trust", "No trusted CA certificates loaded; inspect SSL_CERT_FILE / SSL_CERT_DIR or the system trust store"))
    } else {
        probe(server, identity, query, port, roots).await
    };
    match outcome {
        Ok(sample) => {
            for (key, value) in [
                ("TCP", sample.connect_ms),
                ("TLS", sample.tls_ms),
                ("DNS query", sample.query_ms),
            ] {
                result.metrics.insert(key.into(), format!("{value:.2} ms"));
            }
            result.metrics.insert("Protocol".into(), sample.protocol);
            result
                .metrics
                .insert("Answers".into(), sample.answers.to_string());
            result.findings.push(Finding::new(
                Severity::Pass,
                "DoT certificate and hostname verified",
                format!("Authenticated TLS connection to {identity}"),
                "Certificate checks succeeded for this endpoint.",
            ));
            let status = match sample.rcode {
                0 => "NOERROR",
                1 => "FORMERR",
                2 => "SERVFAIL",
                3 => "NXDOMAIN",
                4 => "NOTIMP",
                5 => "REFUSED",
                _ => "Other DNS error",
            };
            let level = if sample.rcode != 0 {
                Severity::Error
            } else if sample.answers == 0 {
                Severity::Warning
            } else {
                Severity::Pass
            };
            result.findings.push(Finding::new(
                level,
                format!("DoT DNS response · {status}"),
                format!("{query}: {} answer records", sample.answers),
                if sample.rcode == 2 {
                    "Inspect resolver health, upstream reachability and DNSSEC validation."
                } else if sample.rcode == 3 {
                    "Check the queried name; the resolver reports it does not exist."
                } else if sample.answers == 0 {
                    "No A answer was returned; check AAAA or the domain's DNS records."
                } else {
                    "The encrypted resolver answered the A query."
                },
            ));
        }
        Err(error) => {
            let (title, next, level) = match error.stage {
                "TLS" => ("DoT TLS handshake / verification failed", "Check the TLS hostname, certificate expiry, system clock and trusted CA store.", Severity::Error),
                "TCP" => ("DoT resolver connection failed", "Check resolver address, port 853, VPN and firewall rules.", Severity::Error),
                "Resolve" => ("DoT resolver bootstrap DNS failed", "Try the resolver's IP address while preserving its TLS hostname.", Severity::Error),
                "Trust" => ("DoT CA trust unavailable", "Inspect the native CA store and SSL_CERT_FILE / SSL_CERT_DIR.", Severity::Unknown),
                "Protocol" => ("DoT invalid DNS response", "Check the endpoint is a DNS-over-TLS service and retry another resolver.", Severity::Error),
                _ => ("DoT DNS query failed", "TLS may have connected; inspect resolver health and retry the query.", Severity::Error),
            };
            result
                .findings
                .push(Finding::new(level, title, error.to_string(), next));
        }
    }
    result.rows = result.findings.iter().map(Finding::row).collect();
    result.notes.push("One A query over authenticated TLS; native CA trust and hostname validation are required. This tests the selected endpoint, not proof that the operating system is using DoT. No plaintext fallback or certificate bypass.".into());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dns_response_is_bound_to_the_request() {
        let mut data = packet("example.com", 5).unwrap();
        data[2] = 0x81;
        data[3] = 0x83;
        assert_eq!(response(&data, 5, "EXAMPLE.COM.").unwrap(), (3, 0));
        assert!(response(&data, 6, "example.com").is_err());
        assert!(response(&data, 5, "other.example").is_err());
        data[7] = 1;
        assert!(response(&data, 5, "example.com").is_err());
    }
    #[test]
    fn invalid_frames_and_compression_loops_are_rejected() {
        assert!(packet("a..b", 1).is_err());
        assert!(packet("example.com;id", 1).is_err());
        assert!(question_name(&[0xc0, 0], 0).is_err());
        assert!(response(&[0; 11], 1, "example.com").is_err());
    }
}
