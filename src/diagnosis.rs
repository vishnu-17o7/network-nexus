//! Evidence-based findings shared by tools, the dashboard and exports.
use crate::model::Snapshot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Unknown,
    Info,
    Pass,
}
impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "FAIL",
            Self::Warning => "WARN",
            Self::Unknown => "UNKNOWN",
            Self::Info => "INFO",
            Self::Pass => "PASS",
        }
    }
    pub fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Unknown => 2,
            Self::Info => 3,
            Self::Pass => 4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub title: String,
    pub evidence: String,
    pub next_step: String,
}
impl Finding {
    pub fn new(
        severity: Severity,
        title: impl Into<String>,
        evidence: impl Into<String>,
        next_step: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            title: title.into(),
            evidence: evidence.into(),
            next_step: next_step.into(),
        }
    }
    pub fn row(&self) -> Vec<String> {
        vec![
            self.severity.label().into(),
            self.title.clone(),
            self.evidence.clone(),
            self.next_step.clone(),
        ]
    }
}

pub fn local_findings(s: &Snapshot) -> Vec<Finding> {
    if s.timestamp.timestamp() == 0 {
        return vec![];
    }
    let mut out = Vec::new();
    if s.primary.is_none() {
        out.push(Finding::new(
            Severity::Warning,
            "No default route observed",
            "No default interface is visible in this network namespace.",
            "Inspect Routes and Interfaces; check DHCP or namespace restrictions.",
        ));
    }
    if let Some(i) = s.primary_interface() {
        if i.state != "up" && i.state != "unknown" {
            out.push(Finding::new(
                Severity::Error,
                "Network link is down",
                format!("{} reports {}", i.name, i.state),
                "Check cable, Wi-Fi association, radio and interface state.",
            ));
        }
        if i.addresses.is_empty() {
            out.push(Finding::new(
                Severity::Error,
                "Interface has no IP address",
                i.name.clone(),
                "Inspect DHCP or static IP configuration on Interfaces.",
            ));
        }
    }
    if s.dns.is_empty() {
        out.push(Finding::new(
            Severity::Warning,
            "No DNS resolver observed",
            s.dns_source.clone(),
            "Inspect DNS configuration and run a DNS lookup.",
        ));
    }
    if !s.warnings.is_empty() {
        out.push(Finding::new(
            Severity::Unknown,
            "Some network data is unavailable",
            s.warnings.join("; "),
            "Open System for capabilities and permissions. Missing data can limit diagnosis.",
        ));
    }
    out
}

pub fn http_status(code: u16) -> Finding {
    match code {
        404 => Finding::new(
            Severity::Warning,
            "HTTP 404 · resource not found",
            "The server returned 404; the HTTP connection worked.",
            "Check the URL/path, application routes or reverse-proxy mapping.",
        ),
        401 | 403 => Finding::new(
            Severity::Warning,
            format!("HTTP {code} · access denied"),
            "The server is reachable but refused this request.",
            "Check authentication and server access rules.",
        ),
        405 => Finding::new(
            Severity::Warning,
            "HTTP 405 · HEAD unsupported",
            "The endpoint rejected the inspector's HEAD request.",
            "Check the endpoint with its supported method; this is not an outage.",
        ),
        429 => Finding::new(
            Severity::Warning,
            "HTTP 429 · rate limited",
            "The server is reachable and is limiting requests.",
            "Check Retry-After and reduce request frequency.",
        ),
        400..=499 => Finding::new(
            Severity::Warning,
            format!("HTTP {code} · request rejected"),
            "An HTTP response was received.",
            "Inspect the requested URL, method and response headers.",
        ),
        500..=599 => Finding::new(
            Severity::Error,
            format!("HTTP {code} · server error"),
            "The HTTP server responded with a server-side failure.",
            "Inspect the application, upstream server and reverse-proxy logs.",
        ),
        200..=399 => Finding::new(
            Severity::Pass,
            format!("HTTP {code} · endpoint reachable"),
            "The request received an HTTP response.",
            "Inspect headers and timings if the application still behaves unexpectedly.",
        ),
        _ => Finding::new(
            Severity::Unknown,
            "No valid HTTP status",
            format!("Reported status: {code}"),
            "Inspect transport errors and retry the endpoint.",
        ),
    }
}

pub fn transport_failure(error: &str) -> Finding {
    let e = error.to_ascii_lowercase();
    if e.contains("could not resolve") || e.contains("couldn't resolve") {
        Finding::new(
            Severity::Error,
            "DNS resolution failed",
            error,
            "Run DNS lookup; inspect resolver configuration and DNS-over-TLS if enabled.",
        )
    } else if e.contains("certificate") || e.contains("ssl") || e.contains("tls") {
        Finding::new(
            Severity::Error,
            "TLS verification or handshake failed",
            error,
            "Inspect the certificate, hostname, system clock and trusted CA store.",
        )
    } else if e.contains("unavailable") {
        Finding::new(
            Severity::Unknown,
            "Required diagnostic backend unavailable",
            error,
            "Open System and install the optional backend for this test.",
        )
    } else if e.contains("timed out") || e.contains("timeout") {
        Finding::new(
            Severity::Error,
            "Endpoint timed out",
            error,
            "Test TCP connectivity; inspect gateway, VPN, proxy and firewall.",
        )
    } else {
        Finding::new(
            Severity::Error,
            "Endpoint connection failed",
            error,
            "Test another endpoint and inspect routing, proxy and firewall.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_errors_are_not_outages() {
        for code in [404, 405, 503] {
            let f = http_status(code);
            assert!(!f.title.contains("internet"));
            assert!(matches!(f.severity, Severity::Error | Severity::Warning));
        }
        assert_eq!(http_status(200).severity, Severity::Pass);
    }
    #[test]
    fn missing_backend_is_unknown_not_failure() {
        assert_eq!(
            transport_failure("curl is unavailable").severity,
            Severity::Unknown
        );
        assert!(transport_failure("curl: (60) SSL certificate expired")
            .title
            .contains("TLS"));
        assert!(transport_failure("Could not resolve host")
            .title
            .contains("DNS"));
    }
}
