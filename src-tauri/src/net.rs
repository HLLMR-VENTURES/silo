//! Outbound-URL validation — an SSRF guard for native HTTP. The webview and the catalog are
//! treated as potentially compromised, so any URL that reaches `ureq` from them is checked
//! first: HTTPS only (plain `http` allowed only to localhost, for a self-hosted / dev endpoint),
//! and never a loopback / link-local / private / unspecified IP host. Blocks a compromised
//! frontend or upstream from steering a native request at cloud metadata (169.254.169.254) or
//! an internal service.
//!
//! Also home to the shared HTTP agents (`api()`, `download()`): every native request goes
//! through one of them, so each has connect/read timeouts and Silo's User-Agent.

use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;
use url::{Host, Url};

/// The User-Agent every native request sends (GitHub rejects requests without one).
pub const USER_AGENT: &str = concat!(
    "Silo/",
    env!("CARGO_PKG_VERSION"),
    " (+https://silo.hllmr.com)"
);

/// Build an agent with connect / read / write timeouts and an optional overall cap, so a
/// silent or stalled server makes the request fail instead of hanging forever.
pub(crate) fn build_agent(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(connect)
        .timeout_read(read)
        .timeout_write(read)
        .user_agent(USER_AGENT);
    if let Some(t) = overall {
        b = b.timeout(t);
    }
    b.build()
}

/// For API calls: a request that hasn't finished in 60 s fails.
pub fn api() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        build_agent(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(Duration::from_secs(60)),
        )
    })
}

/// For file downloads: no overall cap (large mods), but a stalled read fails after 30 s.
pub fn download() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| build_agent(Duration::from_secs(10), Duration::from_secs(30), None))
}

/// Validate a URL the native layer is about to fetch.
///
/// - Scheme must be `https`, except plain `http` to a loopback/localhost host when
///   `allow_localhost` (a self-hosted or dev endpoint).
/// - The host must not be a loopback / link-local / private / unspecified / CGNAT address
///   (unless it's loopback and `allow_localhost`).
///
/// Returns the parsed `Url` on success.
pub fn validate_outbound_url(raw: &str, allow_localhost: bool) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| format!("not a valid URL: {raw}"))?;
    let host = url.host().ok_or_else(|| "URL has no host".to_string())?;

    let is_loopback_host = match &host {
        Host::Domain(d) => d.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => ip.is_loopback(),
    };

    match url.scheme() {
        "https" => {}
        "http" if allow_localhost && is_loopback_host => {}
        _ => return Err("only https URLs are allowed".to_string()),
    }

    match host {
        Host::Ipv4(ip) => reject_bad_ip(IpAddr::V4(ip), allow_localhost)?,
        Host::Ipv6(ip) => reject_bad_ip(IpAddr::V6(ip), allow_localhost)?,
        Host::Domain(d) => {
            if d.eq_ignore_ascii_case("localhost") && !allow_localhost {
                return Err("refusing a localhost URL".to_string());
            }
        }
    }
    Ok(url)
}

fn reject_bad_ip(ip: IpAddr, allow_localhost: bool) -> Result<(), String> {
    if ip.is_loopback() {
        return if allow_localhost {
            Ok(())
        } else {
            Err("refusing a loopback address".to_string())
        };
    }
    let bad = match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // 100.64.0.0/10 carrier-grade NAT
                || { let o = v4.octets(); o[0] == 100 && (o[1] & 0xc0) == 0x40 }
        }
        IpAddr::V6(v6) => {
            v6.is_unspecified() || {
                let s = v6.segments();
                // fc00::/7 unique-local, fe80::/10 link-local
                (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
            }
        }
    };
    if bad {
        Err("refusing a request to a private / link-local address".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_public_https_only() {
        assert!(validate_outbound_url("https://silo-api.hllmr.com/mods", false).is_ok());
        assert!(
            validate_outbound_url("https://github.com/x/y/releases/download/v1/a.zip", false)
                .is_ok()
        );
        assert!(
            validate_outbound_url("https://cdn27.giants-software.com/modHub/x.zip", false).is_ok()
        );
    }

    #[test]
    fn blocks_ssrf_and_non_https() {
        // non-https; cloud-metadata (link-local); private; CGNAT; loopback v4/v6; ULA v6;
        // localhost; wrong scheme; and garbage — all rejected without a dev flag.
        let bad = [
            "http://silo-api.hllmr.com/",
            "https://169.254.169.254/latest/meta",
            "https://10.0.0.5/x",
            "https://192.168.1.1/x",
            "https://172.16.9.9/x",
            "https://100.64.0.1/x",
            "https://127.0.0.1/x",
            "https://[::1]/x",
            "https://[fc00::1]/x",
            "https://localhost/x",
            "ftp://example.com/x",
            "not a url",
        ];
        for u in bad {
            let blocked = validate_outbound_url(u, false).is_err();
            assert!(blocked, "should block {u}");
        }
    }

    #[test]
    fn allows_localhost_in_dev_mode_only() {
        assert!(validate_outbound_url("http://localhost:8787/mods", true).is_ok());
        assert!(validate_outbound_url("http://127.0.0.1:8787/mods", true).is_ok());
        assert!(validate_outbound_url("https://localhost:8787/mods", true).is_ok());
        // non-loopback private is still blocked even in dev mode
        assert!(validate_outbound_url("http://10.0.0.5/x", true).is_err());
        assert!(validate_outbound_url("https://192.168.0.2/x", true).is_err());
    }

    #[test]
    fn a_silent_server_times_out_instead_of_hanging() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        // Accept and hold the connection open without ever answering.
        let hold = std::thread::spawn(move || {
            let (_s, _) = listener.accept().unwrap();
            let _ = done_rx.recv_timeout(Duration::from_secs(10));
        });
        let agent = build_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let start = std::time::Instant::now();
        let res = agent.get(&format!("http://{addr}/")).call();
        let took = start.elapsed();
        let _ = done_tx.send(());
        hold.join().unwrap();
        assert!(res.is_err());
        assert!(took < Duration::from_secs(5), "took {took:?}");
    }
}
