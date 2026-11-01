//! Outbound-URL validation — an SSRF guard for native HTTP. The webview and the catalog are
//! treated as potentially compromised, so any URL that reaches `ureq` from them is checked
//! first: HTTPS only (plain `http` allowed only to localhost, for a self-hosted / dev endpoint),
//! and never a loopback / link-local / private / unspecified IP host. Blocks a compromised
//! frontend or upstream from steering a native request at cloud metadata (169.254.169.254) or
//! an internal service.
//!
//! Also home to the shared HTTP agents: every native request goes through one of them, so each
//! has connect/read timeouts and Silo's User-Agent.
//!
//! - `api()` talks only to fixed hosts (api.github.com, the catalog base). It keeps ureq's
//!   redirect handling, which GitHub's API uses for renamed repos.
//! - `download()` and `guarded_api()` fetch URLs the catalog or GitHub handed us. They are
//!   HTTPS-only, follow no redirects on their own (use [`get_guarded`], which checks every
//!   hop), and resolve names through [`public_addrs`], so a public-looking name that resolves
//!   to a private address is refused and the address that was checked is the one connected to.

use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;
use url::{Host, Url};

/// The User-Agent every native request sends (GitHub rejects requests without one).
pub const USER_AGENT: &str = concat!(
    "Silo/",
    env!("CARGO_PKG_VERSION"),
    " (+https://silo.hllmr.com)"
);

/// Most redirects a guarded fetch follows. A GitHub release asset takes one
/// (`github.com` → `release-assets.githubusercontent.com`), or two when the repo was renamed.
pub(crate) const MAX_REDIRECTS: usize = 5;

/// An agent builder with connect / read / write timeouts and an optional overall cap, so a
/// silent or stalled server makes the request fail instead of hanging forever.
fn agent_builder(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> ureq::AgentBuilder {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(connect)
        .timeout_read(read)
        .timeout_write(read)
        .user_agent(USER_AGENT);
    if let Some(t) = overall {
        b = b.timeout(t);
    }
    b
}

/// Build an agent with [`agent_builder`]'s timeouts.
pub(crate) fn build_agent(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> ureq::Agent {
    agent_builder(connect, read, overall).build()
}

/// Build an agent for URLs Silo didn't choose itself: HTTPS only, no automatic redirects
/// (follow them with [`get_guarded`]), and a resolver that never hands ureq an address the
/// guard refuses.
pub(crate) fn build_guarded_agent(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> ureq::Agent {
    agent_builder(connect, read, overall)
        .https_only(true)
        .redirects(0)
        .resolver(|netloc: &str| public_addrs(netloc, system_lookup))
        .build()
}

/// For API calls to fixed hosts (GitHub's API, the catalog): a request that hasn't finished
/// in 60 s fails. Never give it a URL from the catalog or the webview; use `guarded_api()`.
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

/// `api()`'s timeouts, guarded (see [`build_guarded_agent`]): for small fetches of URLs the
/// catalog or GitHub handed us (catalog images, a gist's raw file). Use with [`get_guarded`].
pub fn guarded_api() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        build_guarded_agent(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(Duration::from_secs(60)),
        )
    })
}

/// For file downloads (mods, updates), guarded (see [`build_guarded_agent`]): no overall cap
/// (large mods), but a stalled read fails after 30 s. Use with [`get_guarded`].
pub fn download() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| build_guarded_agent(Duration::from_secs(10), Duration::from_secs(30), None))
}

/// The system resolver (what ureq uses by default).
fn system_lookup(netloc: &str) -> io::Result<Vec<SocketAddr>> {
    netloc.to_socket_addrs().map(Iterator::collect)
}

/// Resolve `netloc` (`host:port`) with `lookup` and keep only the addresses the guard allows,
/// so a public name that resolves to 10.x / 192.168.x / 169.254.x / loopback can't be
/// reached. Fails when nothing is left.
pub(crate) fn public_addrs(
    netloc: &str,
    lookup: impl Fn(&str) -> io::Result<Vec<SocketAddr>>,
) -> io::Result<Vec<SocketAddr>> {
    let kept: Vec<SocketAddr> = lookup(netloc)?
        .into_iter()
        .filter(|a| reject_bad_ip(a.ip(), false).is_ok())
        .collect();
    if kept.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing {netloc}: it has no public address"),
        ));
    }
    Ok(kept)
}

/// Why a guarded fetch failed: ureq's own error (a status or transport failure), or a URL the
/// guard refused (the first one or a redirect).
#[derive(Debug)]
pub enum FetchError {
    Http(Box<ureq::Error>),
    Refused(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Http(e) => write!(f, "{e}"),
            FetchError::Refused(s) => f.write_str(s),
        }
    }
}

/// GET `url` with a guarded agent, following up to [`MAX_REDIRECTS`] redirects by hand.
/// `check` runs on the first URL and on every redirect target before anything is sent there.
/// `auth` (an `Authorization` value) is sent only to the first URL's origin, never across
/// hosts or schemes.
pub(crate) fn get_guarded(
    agent: &ureq::Agent,
    url: &str,
    check: &dyn Fn(&Url) -> Result<(), String>,
    auth: Option<&str>,
) -> Result<ureq::Response, FetchError> {
    follow_redirects(url, check, |u, same_origin| {
        let mut req = agent.get(u.as_str());
        if let (Some(a), true) = (auth, same_origin) {
            req = req.set("Authorization", a);
        }
        req.call().map_err(Box::new)
    })
}

/// A `check` for [`get_guarded`]: [`validate_outbound_url`] without the localhost exception.
pub(crate) fn public_https(url: &Url) -> Result<(), String> {
    validate_outbound_url(url.as_str(), false).map(drop)
}

/// The redirect loop behind [`get_guarded`], with the request injected so it can be tested
/// without a network. `send(url, same_origin)` performs one request; `same_origin` says
/// whether `url` has the starting URL's scheme, host and port.
fn follow_redirects(
    start: &str,
    check: &dyn Fn(&Url) -> Result<(), String>,
    mut send: impl FnMut(&Url, bool) -> Result<ureq::Response, Box<ureq::Error>>,
) -> Result<ureq::Response, FetchError> {
    let first = Url::parse(start).map_err(|_| FetchError::Refused("not a valid URL".into()))?;
    check(&first).map_err(FetchError::Refused)?;
    let origin = first.origin();
    let mut url = first;
    let mut hops = 0;
    loop {
        let resp = send(&url, url.origin() == origin).map_err(FetchError::Http)?;
        if !matches!(resp.status(), 301 | 302 | 303 | 307 | 308) {
            return Ok(resp);
        }
        hops += 1;
        if hops > MAX_REDIRECTS {
            return Err(FetchError::Refused(format!(
                "too many redirects (more than {MAX_REDIRECTS})"
            )));
        }
        let next = resp
            .header("Location")
            .and_then(|loc| url.join(loc).ok())
            .ok_or_else(|| FetchError::Refused("a redirect had no usable Location".into()))?;
        check(&next).map_err(|e| FetchError::Refused(format!("Silo refused a redirect: {e}")))?;
        url = next;
    }
}

/// True when `s` is safe to hand to `Url::path_segments_mut` as ONE path segment: not
/// empty, not `.`/`..`, and free of ASCII control characters. The url crate strips tab/LF/CR
/// only after its own dot-segment check, so `.\t.` would otherwise become a real `..`.
pub(crate) fn safe_path_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.bytes().any(|b| b.is_ascii_control())
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
        Host::Domain(d) => is_localhost_name(d),
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => IpAddr::V6(*ip).to_canonical().is_loopback(),
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
            if is_localhost_name(d) && !allow_localhost {
                return Err("refusing a localhost URL".to_string());
            }
        }
    }
    Ok(url)
}

/// `localhost`, with or without the root dot, and any name under it (`api.localhost`), which
/// resolvers answer with loopback (RFC 6761).
fn is_localhost_name(d: &str) -> bool {
    let d = d.trim_end_matches('.').to_ascii_lowercase();
    d == "localhost" || d.ends_with(".localhost")
}

fn reject_bad_ip(ip: IpAddr, allow_localhost: bool) -> Result<(), String> {
    // An IPv4-mapped IPv6 address (`::ffff:10.0.0.1`) connects to its IPv4 address on a
    // dual-stack socket, so it's judged as that address.
    let ip = ip.to_canonical();
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
                // 0.0.0.0/8 "this network": 0.x.x.x reaches the local host on Linux and macOS
                || v4.octets()[0] == 0
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
            // IPv4-mapped IPv6 reaches the IPv4 address on a dual-stack socket.
            "https://[::ffff:169.254.169.254]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://[::ffff:10.0.0.1]/x",
            "https://0.1.2.3/x",
            // Other spellings of localhost.
            "https://localhost./x",
            "https://api.localhost/x",
            "https://LOCALHOST/x",
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
        assert!(validate_outbound_url("http://[::ffff:127.0.0.1]:8787/mods", true).is_ok());
        // A public IPv4 address written IPv4-mapped is still public.
        assert!(validate_outbound_url("https://[::ffff:140.82.112.3]/x", false).is_ok());
        // non-loopback private is still blocked even in dev mode
        assert!(validate_outbound_url("http://10.0.0.5/x", true).is_err());
        assert!(validate_outbound_url("https://192.168.0.2/x", true).is_err());
    }

    /// A canned response with `status` and, when given, a `Location` header.
    fn reply(status: u16, location: Option<&str>) -> Result<ureq::Response, Box<ureq::Error>> {
        let loc = location
            .map(|l| format!("Location: {l}\r\n"))
            .unwrap_or_default();
        Ok(
            format!("HTTP/1.1 {status} X\r\n{loc}Content-Length: 0\r\n\r\n")
                .parse::<ureq::Response>()
                .unwrap(),
        )
    }

    /// Run `follow_redirects` from `start` against canned replies (in order), recording each
    /// URL requested and whether it would carry the credential.
    fn run_chain(
        start: &str,
        replies: Vec<(u16, Option<&str>)>,
    ) -> (Result<u16, String>, Vec<(String, bool)>) {
        let mut sent = Vec::new();
        let mut replies = replies.into_iter();
        let res = follow_redirects(start, &public_https, |u, same_origin| {
            sent.push((u.to_string(), same_origin));
            let (status, loc) = replies.next().expect("more requests than replies");
            reply(status, loc)
        })
        .map(|r| r.status())
        .map_err(|e| e.to_string());
        (res, sent)
    }

    #[test]
    fn a_redirect_to_a_private_http_address_is_refused_unsent() {
        let (res, sent) = run_chain(
            "https://cdn.example.com/a.zip",
            vec![(302, Some("http://192.168.1.1/admin"))],
        );
        assert!(res.unwrap_err().contains("refused a redirect"));
        assert_eq!(sent.len(), 1, "nothing may be sent to the redirect target");
        // Same for https to a private / loopback / metadata address.
        for target in [
            "https://10.0.0.1/x",
            "https://127.0.0.1/x",
            "https://169.254.169.254/latest/meta-data",
            "https://localhost/x",
        ] {
            let (res, sent) = run_chain("https://cdn.example.com/a.zip", vec![(307, Some(target))]);
            assert!(res.is_err(), "should refuse {target}");
            assert_eq!(sent.len(), 1);
        }
    }

    #[test]
    fn an_https_to_http_downgrade_is_refused() {
        let (res, sent) = run_chain(
            "https://cdn.example.com/a.zip",
            vec![(301, Some("http://cdn.example.com/a.zip"))],
        );
        assert!(res.unwrap_err().contains("https"));
        assert_eq!(sent.len(), 1);
    }

    #[test]
    fn redirects_are_followed_and_auth_stays_on_the_first_origin() {
        let (res, sent) = run_chain(
            "https://github.com/o/r/releases/download/v1/a.zip",
            vec![
                // A relative Location is resolved against the current URL.
                (301, Some("/o2/r/releases/download/v1/a.zip")),
                (
                    302,
                    Some("https://release-assets.githubusercontent.com/x?sig=1"),
                ),
                (200, None),
            ],
        );
        assert_eq!(res, Ok(200));
        assert_eq!(
            sent,
            vec![
                (
                    "https://github.com/o/r/releases/download/v1/a.zip".into(),
                    true
                ),
                (
                    "https://github.com/o2/r/releases/download/v1/a.zip".into(),
                    true
                ),
                (
                    "https://release-assets.githubusercontent.com/x?sig=1".into(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn a_redirect_loop_or_a_missing_location_fails() {
        let looped = vec![(302, Some("https://a.example.com/next")); MAX_REDIRECTS + 1];
        let (res, sent) = run_chain("https://a.example.com/", looped);
        assert!(res.unwrap_err().contains("too many redirects"));
        assert_eq!(sent.len(), MAX_REDIRECTS + 1);

        let (res, _) = run_chain("https://a.example.com/", vec![(302, None)]);
        assert!(res.unwrap_err().contains("Location"));
    }

    #[test]
    fn the_first_url_is_checked_too() {
        let (res, sent) = run_chain("http://10.0.0.1/a.zip", vec![]);
        assert!(res.is_err());
        assert!(sent.is_empty());
    }

    #[test]
    fn the_resolver_drops_private_addresses() {
        let stub = |answer: Vec<&'static str>| {
            move |_: &str| -> io::Result<Vec<SocketAddr>> {
                Ok(answer.iter().map(|a| a.parse().unwrap()).collect())
            }
        };
        // A public name that resolves only to private / loopback / link-local addresses.
        for private in [
            "10.0.0.1:443",
            "192.168.1.1:443",
            "169.254.169.254:443",
            "127.0.0.1:443",
            "[::1]:443",
            "[fd00::1]:443",
            "[::ffff:10.0.0.1]:443",
        ] {
            let res = public_addrs("evil.example.com:443", stub(vec![private]));
            assert!(res.is_err(), "should drop {private}");
        }
        // Mixed answers keep only the public addresses.
        let kept = public_addrs(
            "mixed.example.com:443",
            stub(vec!["10.0.0.1:443", "140.82.112.3:443", "[fe80::1]:443"]),
        )
        .unwrap();
        assert_eq!(
            kept,
            vec!["140.82.112.3:443".parse::<SocketAddr>().unwrap()]
        );
        // A lookup failure passes through.
        let failed = public_addrs("x:443", |_: &str| Err(io::Error::other("no such host")));
        assert!(failed.is_err());
    }

    #[test]
    fn the_guarded_agent_refuses_http_and_private_addresses() {
        let agent =
            build_guarded_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        // Plain http is refused before any connection.
        let http = agent.get("http://127.0.0.1:9/").call().unwrap_err();
        assert_eq!(http.kind(), ureq::ErrorKind::InsecureRequestHttpsOnly);
        // https to a loopback address is refused by the resolver, before any connection.
        let lo = agent.get("https://127.0.0.1:9/").call().unwrap_err();
        assert_eq!(lo.kind(), ureq::ErrorKind::Dns);
    }

    #[test]
    fn a_real_redirect_to_a_private_address_is_not_followed() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Answer one request with a redirect to a LAN address.
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let Some(mut s) = accept_within(&listener, deadline) else {
                return;
            };
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            let _ = s.write_all(
                b"HTTP/1.1 302 Found\r\nLocation: http://192.168.1.1/\r\n\
                  Content-Length: 0\r\nConnection: close\r\n\r\n",
            );
        });
        // The guarded agents refuse loopback outright, so drive the same loop with a plain
        // agent that, like them, doesn't follow redirects, and a check that allows this
        // loopback test server (and still refuses every private address).
        let agent = agent_builder(Duration::from_millis(500), Duration::from_millis(500), None)
            .redirects(0)
            .build();
        let res = get_guarded(
            &agent,
            &format!("http://{addr}/a.zip"),
            &|u| validate_outbound_url(u.as_str(), true).map(drop),
            None,
        );
        server.join().unwrap();
        let err = res.unwrap_err().to_string();
        assert!(err.contains("refused a redirect"), "{err}");
    }

    /// Accept one connection, giving up at `deadline` so a test server thread always ends.
    fn accept_within(
        listener: &std::net::TcpListener,
        deadline: std::time::Instant,
    ) -> Option<std::net::TcpStream> {
        listener.set_nonblocking(true).ok()?;
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((s, _)) => {
                    // Accepted sockets may inherit non-blocking mode (Windows); undo it.
                    s.set_nonblocking(false).ok()?;
                    return Some(s);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return None,
            }
        }
        None
    }

    #[test]
    fn a_silent_server_times_out_instead_of_hanging() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        // Accept and hold the connection open without ever answering. Everything here is
        // bounded (accept deadline, hold timeout), so even an agent without timeouts gets the
        // socket closed on it after ~10 s and the test fails instead of hanging.
        let hold = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            if let Some(_s) = accept_within(&listener, deadline) {
                let _ = done_rx.recv_timeout(Duration::from_secs(10));
            }
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

    #[test]
    fn a_slow_but_steady_download_is_not_cut_off() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        const BODY: &[u8] = b"abcdefgh";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Answer with one byte every 200 ms (~1.6 s in all): each gap is under the 500 ms read
        // timeout, the whole transfer well over it.
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let Some(mut s) = accept_within(&listener, deadline) else {
                return;
            };
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                BODY.len()
            );
            if s.write_all(head.as_bytes()).is_err() {
                return;
            }
            for b in BODY {
                std::thread::sleep(Duration::from_millis(200));
                if s.write_all(&[*b]).and_then(|_| s.flush()).is_err() {
                    return;
                }
            }
        });
        let agent = build_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let start = std::time::Instant::now();
        let body = agent
            .get(&format!("http://{addr}/"))
            .call()
            .map_err(|e| e.to_string())
            .and_then(|r| r.into_string().map_err(|e| e.to_string()));
        let took = start.elapsed();
        server.join().unwrap();
        assert_eq!(body.as_deref(), Ok("abcdefgh"));
        assert!(took >= Duration::from_millis(1000), "took {took:?}");
    }
}
