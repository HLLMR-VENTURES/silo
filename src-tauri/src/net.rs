//! Outbound-URL validation — an SSRF guard for native HTTP. The webview and the catalog are
//! treated as potentially compromised, so any URL that reaches `ureq` from them is checked
//! first: HTTPS only (plain `http` allowed only to localhost, for a self-hosted / dev endpoint),
//! and never a loopback / link-local / private / unspecified IP host. Blocks a compromised
//! frontend or upstream from steering a native request at cloud metadata (169.254.169.254) or
//! an internal service.
//!
//! Also home to the shared HTTP agents: every native request goes through one of them, so each
//! has connect/read timeouts and Silo's User-Agent, and none uses a proxy from the environment
//! (a proxy would resolve names itself, out of the guard's sight).
//!
//! - `api()` talks only to GitHub's API. It keeps ureq's redirect handling, which GitHub's API
//!   uses for renamed repos.
//! - `catalog()` talks only to the catalog base and follows no redirects.
//! - `download()` and `guarded_api()` fetch URLs the catalog or GitHub handed us. They are
//!   HTTPS-only, follow no redirects on their own (use [`get_guarded`], which checks every
//!   hop), and resolve names through [`public_addrs`], so a public-looking name that resolves
//!   to a private address is refused and the address that was checked is the one connected to.
//!
//! Every agent hands back 4xx/5xx answers as responses (ureq's `http_status_as_error` is off),
//! and Silo sends requests only through [`CallExt`] / [`SendExt`], which turn those answers
//! into [`HttpError::Status`] with the response kept, so callers can still read its headers
//! and body.

use std::io::{self, Read};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use ureq::config::{Config, ConfigBuilder};
use ureq::http::Uri;
use ureq::typestate::{AgentScope, WithBody, WithoutBody};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{
    ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};
use ureq::{Agent, RequestBuilder, ResponseExt};
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

/// Most redirects `api()` follows on its own. ureq 2 (`redirects(5)`) errored on the fifth
/// redirect, i.e. followed four; ureq 3 follows `max_redirects` and errors on the next.
const API_MAX_REDIRECTS: u32 = 4;

/// Largest body [`RespExt::into_string`] reads (ureq 2's `into_string` limit).
const STRING_LIMIT: u64 = 10 * 1024 * 1024;

/// A response from one of Silo's agents.
pub type Response = ureq::http::Response<ureq::Body>;

/// An agent config with connect / per-read / per-write timeouts and an optional overall cap, so
/// a silent or stalled server makes the request fail instead of hanging forever. No proxy from
/// the environment, and 4xx/5xx come back as responses (see the module docs).
fn agent_config(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> ConfigBuilder<AgentScope> {
    Agent::config_builder()
        .timeout_connect(Some(connect))
        .timeout_per_read(Some(read))
        .timeout_per_write(Some(read))
        .timeout_global(overall)
        .user_agent(USER_AGENT)
        .proxy(None)
        .http_status_as_error(false)
}

/// An agent from `config` that resolves names with [`SiloResolver`] and connects with
/// [`SiloConnector`].
fn agent_with(config: Config, public_only: bool) -> Agent {
    Agent::with_parts(
        config,
        SiloConnector::default(),
        SiloResolver { public_only },
    )
}

/// Build an agent with [`agent_config`]'s timeouts, following no redirects.
pub(crate) fn build_agent(connect: Duration, read: Duration, overall: Option<Duration>) -> Agent {
    agent_with(
        agent_config(connect, read, overall)
            .max_redirects(0)
            .build(),
        false,
    )
}

/// An agent for URLs Silo didn't choose itself, used only through [`get_guarded`]: HTTPS only,
/// no automatic redirects, and a resolver that never hands ureq an address the guard refuses.
/// `overall` caps the whole fetch, every redirect included, from connecting to reading the
/// body. ureq stops waiting for a slow DNS lookup when the cap runs out (the lookup thread is
/// left to finish on its own); time spent there uses up the cap.
pub struct GuardedAgent {
    agent: Agent,
    overall: Option<Duration>,
}

/// Build a [`GuardedAgent`] with [`agent_config`]'s connect / read timeouts.
pub(crate) fn build_guarded_agent(
    connect: Duration,
    read: Duration,
    overall: Option<Duration>,
) -> GuardedAgent {
    let config = agent_config(connect, read, None)
        .https_only(true)
        .max_redirects(0)
        .build();
    GuardedAgent {
        agent: agent_with(config, true),
        overall,
    }
}

/// `api()`'s agent with these timeouts: follows up to [`API_MAX_REDIRECTS`] redirects.
fn build_api_agent(connect: Duration, read: Duration, overall: Option<Duration>) -> Agent {
    agent_with(
        agent_config(connect, read, overall)
            .max_redirects(API_MAX_REDIRECTS)
            // For "(redirected from …)" in a status error's text, as ureq 2 wrote it.
            .save_redirect_history(true)
            .build(),
        false,
    )
}

/// For API calls to GitHub's API: a request that hasn't finished in 60 s fails. Follows
/// redirects (GitHub's API uses them for renamed repos). Never give it a URL from the catalog
/// or the webview; use `guarded_api()`.
pub fn api() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        build_api_agent(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(Duration::from_secs(60)),
        )
    })
}

/// For calls to Silo's catalog: `api()`'s timeouts, but no redirects. The catalog never
/// redirects, so a redirect means something is wrong and is not followed (see
/// [`refuse_redirect`]). No address filter: the base is fixed in release builds, and a
/// developer's catalog can be on localhost.
pub fn catalog() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        build_agent(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(Duration::from_secs(60)),
        )
    })
}

/// A response from an agent with redirects off, refused when it is a redirect (3xx), which
/// such an agent returns as a success.
pub(crate) fn refuse_redirect(resp: Response) -> Result<Response, u16> {
    match resp.status().as_u16() {
        s @ 300..=399 => Err(s),
        _ => Ok(resp),
    }
}

/// `api()`'s timeouts, guarded (see [`GuardedAgent`]): for small fetches of URLs the catalog
/// or GitHub handed us (catalog images, a gist's raw file). Use with [`get_guarded`].
pub fn guarded_api() -> &'static GuardedAgent {
    static AGENT: OnceLock<GuardedAgent> = OnceLock::new();
    AGENT.get_or_init(|| {
        build_guarded_agent(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(Duration::from_secs(60)),
        )
    })
}

/// For file downloads (mods, updates), guarded (see [`GuardedAgent`]): no overall cap (large
/// mods), but a stalled read fails after 30 s. Use with [`get_guarded`].
pub fn download() -> &'static GuardedAgent {
    static AGENT: OnceLock<GuardedAgent> = OnceLock::new();
    AGENT
        .get_or_init(|| build_guarded_agent(Duration::from_secs(10), Duration::from_secs(30), None))
}

/// Name resolution for every agent, with a failed lookup reported as [`DnsFailed`].
///
/// - Plain agents use the system resolver through ureq's `DefaultResolver`, which honours the
///   request's timeouts.
/// - With `public_only`, Silo looks the name up itself (same system resolver, same timeout
///   handling), keeps only the addresses [`public_addrs`] allows, and hands ureq at most the
///   first [`MAX_ADDRS`] of those; ureq connects to nothing else. Filtering before that cap
///   (ureq's own resolver stops at 16 answers) keeps a public address that comes after 16
///   private ones, as ureq 2 did.
#[derive(Debug)]
struct SiloResolver {
    public_only: bool,
}

/// Most addresses ureq takes from a resolver (`ResolvedSocketAddrs`'s capacity).
const MAX_ADDRS: usize = 16;

impl Resolver for SiloResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let netloc = || {
            uri.scheme()
                .zip(uri.authority())
                .and_then(|(s, a)| DefaultResolver::host_and_port(s, a))
        };
        if !self.public_only {
            return match DefaultResolver::default().resolve(uri, config, timeout) {
                Ok(found) => Ok(found),
                Err(ureq::Error::Io(e)) => Err(DnsFailed::error(netloc().unwrap_or_default(), e)),
                Err(ureq::Error::HostNotFound) => {
                    let e = io::Error::new(io::ErrorKind::NotFound, "no address found");
                    Err(DnsFailed::error(netloc().unwrap_or_default(), e))
                }
                // A timeout, a bad URI: ureq's own error.
                Err(e) => Err(e),
            };
        }
        let Some(netloc) = netloc() else {
            return Err(ureq::Error::BadUri(format!("no host and port in {uri}")));
        };
        let all = lookup_within(&netloc, timeout)?;
        let kept = guarded_addrs(&netloc, all).map_err(|e| DnsFailed::error(netloc, e))?;
        let mut out = self.empty();
        for a in kept {
            out.push(a);
        }
        Ok(out)
    }
}

/// What a guarded agent may connect to out of `all` (every address a lookup of `netloc`
/// found): the public ones, at most [`MAX_ADDRS`], filtered before the cap. The cap is ureq 3's
/// (a resolver hands over a fixed 16-slot list, for every agent), so a 17th usable answer is
/// never tried where ureq 2 would have tried it; a known, accepted loss for hosts with more
/// than 16 addresses.
fn guarded_addrs(netloc: &str, all: Vec<SocketAddr>) -> io::Result<Vec<SocketAddr>> {
    let mut kept = public_addrs(netloc, |_| Ok(all.clone()))?;
    kept.truncate(MAX_ADDRS);
    Ok(kept)
}

/// Every address the system resolver has for `netloc`, giving up when `timeout` runs out the
/// way ureq's `DefaultResolver` does: with no timeout the lookup runs on this thread; with one,
/// on its own thread, which is left to finish if the time runs out first.
fn lookup_within(netloc: &str, timeout: NextTimeout) -> Result<Vec<SocketAddr>, ureq::Error> {
    let failed = |e: io::Error| DnsFailed::error(netloc.to_string(), e);
    if timeout.after.is_not_happening() {
        return netloc
            .to_socket_addrs()
            .map(Iterator::collect)
            .map_err(failed);
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let owned = netloc.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(owned.to_socket_addrs().map(Iterator::collect::<Vec<_>>));
    });
    match rx.recv_timeout(*timeout.after) {
        Ok(found) => found.map_err(failed),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(ureq::Error::Timeout(timeout.reason))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(failed(io::Error::other(
            "the lookup stopped without an answer",
        ))),
    }
}

/// ureq's own connector, called once per address so the connect budget is one absolute
/// deadline across all of them, as in ureq 2. ureq 3's own loop splits the budget between
/// addresses up front and doesn't recheck it after a refused or unreachable address, so a host
/// whose addresses each refuse slowly could take several times the budget.
///
/// An I/O failure while connecting (refused, unreachable, reset during the TLS handshake) is
/// reported as [`ConnectFailed`], ureq 2's "Connection Failed". Other errors pass through.
#[derive(Debug, Default)]
struct SiloConnector(DefaultConnector);

impl Connector<()> for SiloConnector {
    type Out = Box<dyn Transport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        // ureq's clock (tests inside ureq inject one); a real request always has an exact time.
        let now = || match (details.current_time)() {
            ureq::unversioned::transport::time::Instant::Exact(t) => t,
            _ => Instant::now(),
        };
        connect_each(&details.addrs, details.timeout, &now, |addr, timeout| {
            let mut one = details.resolver.empty();
            one.push(addr);
            let single = ConnectionDetails {
                uri: details.uri,
                addrs: one,
                config: details.config,
                request_level: details.request_level,
                resolver: details.resolver,
                now: (details.current_time)(),
                timeout,
                current_time: details.current_time.clone(),
                run_connector: details.run_connector.clone(),
                register_shutdown: details.register_shutdown.clone(),
            };
            self.0.connect(&single, chained)
        })
        .map_err(|e| match e {
            ureq::Error::Io(source) => ConnectFailed::error(source),
            e => e,
        })
    }
}

/// The shortest connect attempt ureq makes (its `MIN_PER_ADDRESS_TIMEOUT`).
const MIN_ATTEMPT: Duration = Duration::from_millis(10);

/// Try `attempt` on each address in turn until one connects, within one deadline (`timeout`
/// from the first attempt). As in ureq 2 (`connect_host`): with more than one address, each
/// attempt gets half the time left; a failed attempt (an I/O error or its own timeout) moves
/// on to the next address; once the deadline has passed, it is a timeout with `timeout`'s
/// reason. Any other error ends it.
fn connect_each<T>(
    addrs: &[SocketAddr],
    timeout: NextTimeout,
    now: &dyn Fn() -> Instant,
    mut attempt: impl FnMut(SocketAddr, NextTimeout) -> Result<T, ureq::Error>,
) -> Result<T, ureq::Error> {
    use ureq::unversioned::transport::time::Duration as Budget;
    let deadline = (!timeout.after.is_not_happening()).then(|| now() + *timeout.after);
    let expired = || ureq::Error::Timeout(timeout.reason);
    let multiple = addrs.len() > 1;
    let mut last = None;
    for &addr in addrs {
        let mut this = timeout;
        if let Some(d) = deadline {
            let left = d.saturating_duration_since(now());
            if left.is_zero() {
                return Err(expired());
            }
            let share = if multiple { left / 2 } else { left };
            // ureq's TCP connect never waits less than 10 ms (`MIN_PER_ADDRESS_TIMEOUT`), so say
            // so here: an attempt can end at most 10 ms past the deadline, and the next check
            // stops there. (Also never zero, which ureq would read as "one second".)
            this.after = Budget::Exact(share.max(MIN_ATTEMPT));
        }
        match attempt(addr, this) {
            Ok(v) => return Ok(v),
            Err(e @ (ureq::Error::Io(_) | ureq::Error::Timeout(_))) => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    if deadline.is_some_and(|d| now() >= d) {
        return Err(expired());
    }
    Err(last.unwrap_or_else(|| {
        ureq::Error::Io(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "no address to connect to",
        ))
    }))
}

/// A connection that couldn't be made (ureq 2's "Connection Failed"). Carried in
/// `ureq::Error::Other`.
#[derive(Debug)]
pub(crate) struct ConnectFailed(io::Error);

impl ConnectFailed {
    pub(crate) fn error(source: io::Error) -> ureq::Error {
        ureq::Error::Other(Box::new(ConnectFailed(source)))
    }
}

impl std::fmt::Display for ConnectFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Connect error: {}", self.0)
    }
}

impl std::error::Error for ConnectFailed {}

/// A name lookup that failed or found no address Silo may use (ureq 2's "Dns Failed"). Carried
/// in `ureq::Error::Other`.
#[derive(Debug)]
pub(crate) struct DnsFailed {
    netloc: String,
    source: io::Error,
}

impl DnsFailed {
    pub(crate) fn error(netloc: String, source: io::Error) -> ureq::Error {
        ureq::Error::Other(Box::new(DnsFailed { netloc, source }))
    }
}

impl std::fmt::Display for DnsFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "resolve dns name '{}': {}", self.netloc, self.source)
    }
}

impl std::error::Error for DnsFailed {}

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

/// A failed request: the server answered 4xx/5xx (ureq 2's `Error::Status`), or there was no
/// usable answer (ureq 2's `Error::Transport`).
#[derive(Debug)]
pub enum HttpError {
    Status(u16, Box<StatusError>),
    Transport(Box<TransportError>),
}

/// A 4xx/5xx answer, kept whole so its headers and body can be read.
#[derive(Debug)]
pub struct StatusError {
    url: String,
    /// The URL first asked for, when `api()` followed redirects to `url`.
    redirected_from: Option<String>,
    resp: Response,
}

impl StatusError {
    /// The URL that answered (after any redirects `api()` followed).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// A response header, when present and valid text.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.resp.header(name)
    }

    pub fn into_response(self) -> Response {
        self.resp
    }
}

/// A request that got no usable answer, with the URL it was for when known.
#[derive(Debug)]
pub struct TransportError {
    url: Option<String>,
    error: ureq::Error,
}

impl TransportError {
    pub fn error(&self) -> &ureq::Error {
        &self.error
    }

    /// The request was refused before it was sent because a header was malformed. Such an
    /// error never quotes the header (http's error text has no value in it).
    pub fn is_bad_header(&self) -> bool {
        matches!(&self.error, ureq::Error::Http(h) if is_header_error(h))
    }
}

fn is_header_error(h: &ureq::http::Error) -> bool {
    h.is::<ureq::http::header::InvalidHeaderValue>()
        || h.is::<ureq::http::header::InvalidHeaderName>()
}

impl HttpError {
    /// A 4xx/5xx answer from `url`.
    pub(crate) fn status(url: impl Into<String>, resp: Response) -> HttpError {
        HttpError::Status(
            resp.status().as_u16(),
            Box::new(StatusError {
                url: url.into(),
                redirected_from: None,
                resp,
            }),
        )
    }

    /// A failure with no usable answer.
    pub(crate) fn transport(url: Option<String>, error: ureq::Error) -> HttpError {
        HttpError::Transport(Box::new(TransportError { url, error }))
    }
}

/// ureq 2's label for a kind of failure, which Silo's error texts (and `error::classify`) have
/// always carried.
fn transport_label(e: &ureq::Error) -> Option<&'static str> {
    use ureq::Error as E;
    Some(match e {
        E::Other(o) if o.is::<DnsFailed>() => "Dns Failed",
        E::HostNotFound => "Dns Failed",
        E::Io(_) | E::Timeout(_) | E::Rustls(_) | E::Tls(_) => "Network Error",
        E::Other(o) if o.is::<ConnectFailed>() => "Connection Failed",
        E::ConnectionFailed => "Connection Failed",
        E::RequireHttpsOnly(_) => "Insecure request attempted with https_only set",
        E::TooManyRedirects => "Too Many Redirects",
        E::Http(h) if is_header_error(h) => "Bad Header",
        E::Http(_) | E::BadUri(_) => "Bad URL",
        E::ConnectProxyFailed(_) => "Proxy failed to connect",
        E::InvalidProxyUrl => "Malformed proxy",
        _ => return None,
    })
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ureq 2's wording: "<url>: status code 404".
            // ureq 2's wording: "<url>: status code 404 (redirected from <first url>)".
            HttpError::Status(code, s) => {
                write!(f, "{}: status code {code}", s.url)?;
                if let Some(first) = &s.redirected_from {
                    write!(f, " (redirected from {first})")?;
                }
                Ok(())
            }
            HttpError::Transport(t) => {
                if let Some(u) = &t.url {
                    write!(f, "{u}: ")?;
                }
                if let Some(label) = transport_label(&t.error) {
                    write!(f, "{label}: ")?;
                }
                match &t.error {
                    ureq::Error::Io(e) => write!(f, "{e}"),
                    ureq::Error::Timeout(which) => write!(f, "timed out ({which})"),
                    // ureq 2's text, with the limit it counted against (its `redirects(5)`).
                    ureq::Error::TooManyRedirects => {
                        write!(f, "reached max redirects ({})", API_MAX_REDIRECTS + 1)
                    }
                    ureq::Error::Other(o) => write!(f, "{o}"),
                    e => write!(f, "{e}"),
                }
            }
        }
    }
}

impl std::error::Error for HttpError {}

/// `r` with a 4xx/5xx answer turned into [`HttpError::Status`], as ureq 2 did.
fn checked(url: Option<String>, r: Result<Response, ureq::Error>) -> Result<Response, HttpError> {
    match r {
        Ok(resp) if resp.status().as_u16() >= 400 => {
            let url = resp.get_uri().to_string();
            let redirected_from = resp
                .get_redirect_history()
                .filter(|h| h.len() > 1)
                .map(|h| h[0].to_string());
            let mut e = HttpError::status(url, resp);
            if let HttpError::Status(_, s) = &mut e {
                s.redirected_from = redirected_from;
            }
            Err(e)
        }
        Ok(resp) => Ok(resp),
        Err(e) => Err(HttpError::transport(url, e)),
    }
}

/// The request's URL (for error texts), or the error for a header ureq 2 would have refused
/// to send: one with a byte outside visible ASCII, space and tab. ureq 3 lets through bytes
/// 0x80-0xFF in most headers (and refuses them in `Authorization` only once it is sending);
/// this keeps ureq 2's rule, checked before anything is sent.
fn prepare<B>(req: &RequestBuilder<B>) -> Result<Option<String>, HttpError> {
    let url = req.uri_ref().map(Uri::to_string);
    let bad = req.headers_ref().is_some_and(|h| {
        h.values().any(|v| {
            !v.as_bytes()
                .iter()
                .all(|b| matches!(b, b' ' | b'\t' | 0x21..=0x7e))
        })
    });
    if bad {
        let invalid = ureq::http::HeaderValue::from_bytes(b"\n").unwrap_err();
        let e = ureq::Error::Http(ureq::http::Error::from(invalid));
        return Err(HttpError::transport(url, e));
    }
    Ok(url)
}

/// Send a request that has no body. Every bodiless request Silo makes goes through this.
pub trait CallExt {
    fn call_checked(self) -> Result<Response, HttpError>;
}

impl CallExt for RequestBuilder<WithoutBody> {
    fn call_checked(self) -> Result<Response, HttpError> {
        let url = prepare(&self)?;
        checked(url, self.call())
    }
}

/// Send a request with a body (POST / PUT / PATCH). Every such request Silo makes goes through
/// one of these.
pub trait SendExt {
    fn send_json_checked(self, data: impl serde::Serialize) -> Result<Response, HttpError>;
    fn send_form_checked(self, form: &[(&str, &str)]) -> Result<Response, HttpError>;
    /// No body, sent with `Content-Length: 0`.
    fn send_empty_checked(self) -> Result<Response, HttpError>;
}

/// `req` with `Content-Type: value` unless it already has a content type (ureq 2's rule).
fn default_content_type(req: RequestBuilder<WithBody>, value: &str) -> RequestBuilder<WithBody> {
    let has = req
        .headers_ref()
        .is_some_and(|h| h.contains_key(ureq::http::header::CONTENT_TYPE));
    if has {
        req
    } else {
        req.header(ureq::http::header::CONTENT_TYPE, value)
    }
}

impl SendExt for RequestBuilder<WithBody> {
    /// Compact JSON with `Content-Type: application/json`, as ureq 2 sent it (ureq 3's own
    /// `send_json` pretty-prints and adds `; charset=utf-8`).
    fn send_json_checked(self, data: impl serde::Serialize) -> Result<Response, HttpError> {
        let url = prepare(&self)?;
        let bytes = match serde_json::to_vec(&data) {
            Ok(b) => b,
            Err(e) => return Err(HttpError::transport(url, ureq::Error::Json(e))),
        };
        let req = default_content_type(self, "application/json");
        checked(url, req.send(&bytes[..]))
    }

    /// `application/x-www-form-urlencoded`, encoded the way ureq 2 did (the url crate's form
    /// serializer; ureq 3 uses a different escape set).
    fn send_form_checked(self, form: &[(&str, &str)]) -> Result<Response, HttpError> {
        let url = prepare(&self)?;
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let req = default_content_type(self, "application/x-www-form-urlencoded");
        checked(url, req.send(encoded.as_bytes()))
    }

    fn send_empty_checked(self) -> Result<Response, HttpError> {
        let url = prepare(&self)?;
        checked(url, self.send_empty())
    }
}

/// ureq 2's response conveniences, with ureq 2's limits.
pub trait RespExt {
    /// A header's value, when present and valid text.
    fn header(&self, name: &str) -> Option<&str>;
    /// The body as text (invalid UTF-8 replaced), refused over 10 MB.
    fn into_string(self) -> io::Result<String>;
    /// The body parsed as JSON. No size limit, as in ureq 2.
    fn into_json<T: serde::de::DeserializeOwned>(self) -> io::Result<T>;
    /// The body as a reader. No size limit: callers bound what they read.
    fn into_reader(self) -> BodyRead;
}

/// A response body as a reader (see [`RespExt::into_reader`]). A timeout while reading is an
/// `io::ErrorKind::TimedOut` error, as ureq 2 gave; ureq 3's reader reports it as `Other`
/// wrapping `ureq::Error::Timeout`.
pub struct BodyRead(ureq::BodyReader<'static>);

impl Read for BodyRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|e| {
            let timed_out = e
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<ureq::Error>())
                .is_some_and(|u| matches!(u, ureq::Error::Timeout(_)));
            if timed_out {
                io::Error::new(io::ErrorKind::TimedOut, e.to_string())
            } else {
                e
            }
        })
    }
}

impl RespExt for Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers().get(name).and_then(|v| v.to_str().ok())
    }

    fn into_string(self) -> io::Result<String> {
        let mut buf = Vec::new();
        self.into_reader()
            .take(STRING_LIMIT + 1)
            .read_to_end(&mut buf)?;
        if buf.len() as u64 > STRING_LIMIT {
            return Err(io::Error::other("response too big for into_string"));
        }
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    fn into_json<T: serde::de::DeserializeOwned>(self) -> io::Result<T> {
        let reader = io::BufReader::new(self.into_reader());
        serde_json::from_reader(reader).map_err(|e| {
            if e.io_error_kind() == Some(io::ErrorKind::TimedOut) {
                return io::Error::new(io::ErrorKind::TimedOut, e.to_string());
            }
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to read JSON: {e}"),
            )
        })
    }

    fn into_reader(self) -> BodyRead {
        // `limit(u64::MAX)`: ureq's `Body::into_reader` is documented as unlimited; say so.
        BodyRead(self.into_body().into_with_config().limit(u64::MAX).reader())
    }
}

/// Why a guarded fetch failed: an HTTP failure (a status or transport failure), or a URL the
/// guard refused (the first one or a redirect).
#[derive(Debug)]
pub enum FetchError {
    Http(HttpError),
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
/// hosts or schemes. The agent's overall cap counts from the first request, so redirects
/// don't extend it; it covers reading the final body too (see [`GuardedAgent`] for DNS).
pub(crate) fn get_guarded(
    g: &GuardedAgent,
    url: &str,
    check: &dyn Fn(&Url) -> Result<(), String>,
    auth: Option<&str>,
) -> Result<Response, FetchError> {
    let deadline = g.overall.map(|d| Instant::now() + d);
    follow_redirects(url, check, |u, same_origin| {
        let mut req = g.agent.get(u.as_str());
        if let Some(d) = deadline {
            let left = d.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let timed_out = ureq::Error::Timeout(ureq::Timeout::Global);
                return Err(HttpError::transport(Some(u.to_string()), timed_out));
            }
            req = req.config().timeout_global(Some(left)).build();
        }
        if let (Some(a), true) = (auth, same_origin) {
            req = req.header("Authorization", a);
        }
        req.call_checked()
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
    mut send: impl FnMut(&Url, bool) -> Result<Response, HttpError>,
) -> Result<Response, FetchError> {
    let first = Url::parse(start).map_err(|_| FetchError::Refused("not a valid URL".into()))?;
    check(&first).map_err(FetchError::Refused)?;
    let origin = first.origin();
    let mut url = first;
    let mut hops = 0;
    loop {
        let resp = send(&url, url.origin() == origin).map_err(FetchError::Http)?;
        match resp.status().as_u16() {
            301 | 302 | 303 | 307 | 308 => {}
            // 300 / 304 / 305 and the like: nothing to follow, and no body to use.
            s @ 300..=399 => {
                return Err(FetchError::Refused(format!(
                    "unexpected response (error {s})"
                )))
            }
            _ => return Ok(resp),
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
    if let IpAddr::V6(v6) = ip {
        let s = v6.segments();
        // NAT64. The well-known prefix 64:ff9b::/96 reaches the IPv4 address in the low 32
        // bits, so judge that one (a DNS64 network answers github.com this way). The rest of
        // 64:ff9b::/32 is refused: the local-use 64:ff9b:1::/48 is split by each network as it
        // likes, so which IPv4 address it reaches can't be read from the address.
        if s[0] == 0x64 && s[1] == 0xff9b {
            if s[2..6] != [0, 0, 0, 0] {
                return Err("refusing a NAT64 address Silo can't check".to_string());
            }
            let v4 = std::net::Ipv4Addr::from((u32::from(s[6]) << 16) | u32::from(s[7]));
            return reject_bad_ip(IpAddr::V4(v4), false);
        }
    }
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
                // fc00::/7 unique-local, fe80::/10 link-local, fec0::/10 site-local (deprecated,
                // still routed on some internal networks)
                (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || (s[0] & 0xffc0) == 0xfec0
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
            "https://[fec0::1234]/x",
            "https://[64:ff9b::a9fe:a9fe]/x",
            "https://[64:ff9b:1::7f00:1]/x",
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

    /// A canned response with `status`, the given headers and `body`.
    fn canned(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Response {
        let mut b = ureq::http::Response::builder().status(status);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(ureq::Body::builder().data(body.to_vec())).unwrap()
    }

    /// A canned response with `status` and, when given, a `Location` header.
    fn reply(status: u16, location: Option<&str>) -> Result<Response, HttpError> {
        let loc: Vec<(&str, &str)> = location.map(|l| ("Location", l)).into_iter().collect();
        Ok(canned(status, &loc, b""))
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
        .map(|r| r.status().as_u16())
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

        // Any other 3xx is an error, not an empty success, and nothing more is requested.
        for status in [300, 304, 305] {
            let (res, sent) = run_chain("https://a.example.com/", vec![(status, Some("/x"))]);
            assert!(res.unwrap_err().contains(&status.to_string()));
            assert_eq!(sent.len(), 1);
        }
    }

    #[test]
    fn a_catalog_redirect_is_refused() {
        for status in [301, 302, 303, 307, 308] {
            let resp = reply(status, Some("http://192.168.1.1/")).unwrap();
            assert_eq!(refuse_redirect(resp).unwrap_err(), status);
        }
        assert!(refuse_redirect(reply(200, None).unwrap()).is_ok());
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
            "[fec0::1234]:443",
            // NAT64 of a private address, and the local-use NAT64 prefix in any layout (as a
            // /64 prefix, 64:ff9b:1::808:808 reaches 0.0.0.8, not 8.8.8.8).
            "[64:ff9b::c0a8:101]:443",
            "[64:ff9b:1::a00:1]:443",
            "[64:ff9b:1::808:808]:443",
            "[64:ff9b:1:ab::8c52:7003]:443",
            "[64:ff9b:0:1::8c52:7003]:443",
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
        // On a DNS64 network a public site answers with NAT64 of its public address: kept.
        assert!(public_addrs("github.com:443", stub(vec!["[64:ff9b::8c52:7003]:443"])).is_ok());
        // A lookup failure passes through.
        let failed = public_addrs("x:443", |_: &str| Err(io::Error::other("no such host")));
        assert!(failed.is_err());
    }

    /// The transport error inside `e`.
    fn transport(e: HttpError) -> Box<TransportError> {
        match e {
            HttpError::Transport(t) => t,
            HttpError::Status(s, _) => panic!("expected a transport error, got status {s}"),
        }
    }

    #[test]
    fn the_guarded_agent_refuses_http_and_private_addresses() {
        let agent =
            build_guarded_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        // Plain http is refused before any connection.
        let http = transport(
            agent
                .agent
                .get("http://127.0.0.1:9/")
                .call_checked()
                .unwrap_err(),
        );
        assert!(
            matches!(http.error(), ureq::Error::RequireHttpsOnly(_)),
            "{:?}",
            http.error()
        );
        // https to a loopback address is refused by the resolver, before any connection: the
        // only address it has isn't public.
        for target in [
            "https://127.0.0.1:9/",
            "https://[::1]:9/",
            "https://localhost:9/",
        ] {
            let e = agent.agent.get(target).call_checked().unwrap_err();
            let text = e.to_string();
            let lo = transport(e);
            assert!(
                matches!(lo.error(), ureq::Error::Other(o) if o.is::<DnsFailed>()),
                "{target}: {:?}",
                lo.error()
            );
            assert!(text.contains("Dns Failed"), "{text}");
            assert!(text.contains("no public address"), "{text}");
        }
    }

    #[test]
    fn no_agent_takes_a_proxy_or_turns_statuses_into_bare_errors() {
        // A proxy resolves names itself, past the guard's resolver, and ureq 3 reads one from
        // the environment by default. ureq 2 (no `proxy-from-env` feature) never did.
        let agents = [
            ("api", api()),
            ("catalog", catalog()),
            ("guarded_api", &guarded_api().agent),
            ("download", &download().agent),
        ];
        for (name, a) in agents {
            let c = a.config();
            assert!(c.proxy().is_none(), "{name} has a proxy");
            assert!(!c.http_status_as_error(), "{name}");
            assert_eq!(
                c.timeouts().connect,
                Some(Duration::from_secs(10)),
                "{name}"
            );
            assert_eq!(
                c.timeout_per_read(),
                Some(Duration::from_secs(30)),
                "{name}"
            );
            assert_eq!(
                c.timeout_per_write(),
                Some(Duration::from_secs(30)),
                "{name}"
            );
        }
        assert_eq!(api().config().max_redirects(), API_MAX_REDIRECTS);
        assert_eq!(
            api().config().timeouts().global,
            Some(Duration::from_secs(60))
        );
        assert_eq!(catalog().config().max_redirects(), 0);
        assert_eq!(
            catalog().config().timeouts().global,
            Some(Duration::from_secs(60))
        );
        for g in [guarded_api(), download()] {
            assert!(g.agent.config().https_only());
            assert_eq!(g.agent.config().max_redirects(), 0);
            // The overall cap is applied per fetch by `get_guarded`, not by the agent.
            assert_eq!(g.agent.config().timeouts().global, None);
        }
        assert_eq!(guarded_api().overall, Some(Duration::from_secs(60)));
        assert_eq!(download().overall, None);
    }

    /// One canned answer per connection, in order, each request's head and body recorded. The
    /// thread returns what it received, once no request comes for 2 s.
    fn serve(
        listener: std::net::TcpListener,
        answers: Vec<String>,
    ) -> std::thread::JoinHandle<Vec<String>> {
        use std::io::Write;
        std::thread::spawn(move || {
            let mut got = Vec::new();
            for answer in answers {
                let deadline = Instant::now() + Duration::from_secs(2);
                let Some(mut s) = accept_within(&listener, deadline) else {
                    break;
                };
                got.push(read_request(&mut s));
                let _ = s.write_all(answer.as_bytes());
            }
            got
        })
    }

    /// One request's head plus its body (as far as `Content-Length` says), as text.
    fn read_request(s: &mut std::net::TcpStream) -> String {
        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
        let mut req = Vec::new();
        let mut buf = [0u8; 1024];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => req.extend_from_slice(&buf[..n]),
            }
        }
        let text = String::from_utf8_lossy(&req).to_string();
        let head_len = text.find("\r\n\r\n").map_or(text.len(), |i| i + 4);
        let want = text
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        while req.len() < head_len + want {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => req.extend_from_slice(&buf[..n]),
            }
        }
        String::from_utf8_lossy(&req).to_string()
    }

    fn answer(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn quick_agent() -> Agent {
        build_agent(Duration::from_millis(500), Duration::from_millis(500), None)
    }

    #[test]
    fn an_error_status_comes_back_with_its_response() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(
            listener,
            vec![
                answer(
                    "403 Forbidden",
                    "x-ratelimit-remaining: 0\r\n",
                    "{\"message\":\"API rate limit exceeded\"}",
                ),
                answer("500 Internal Server Error", "", "oops"),
                answer("200 OK", "", "fine"),
            ],
        );
        let url = format!("http://{addr}/x");
        let e = quick_agent().get(&url).call_checked().unwrap_err();
        assert_eq!(e.to_string(), format!("{url}: status code 403"));
        let HttpError::Status(403, s) = e else {
            panic!("expected a 403, got {e}");
        };
        assert_eq!(s.url(), url);
        assert_eq!(s.header("x-ratelimit-remaining"), Some("0"));
        let body = s.into_response().into_string().unwrap();
        assert!(body.contains("rate limit"), "{body}");
        // A 5xx is an error too, and a 2xx is not.
        let e = quick_agent().get(&url).call_checked().unwrap_err();
        assert!(matches!(e, HttpError::Status(500, _)), "{e}");
        let ok = quick_agent().get(&url).call_checked().unwrap();
        assert_eq!(ok.into_string().unwrap(), "fine");
        assert_eq!(server.join().unwrap().len(), 3);
    }

    #[test]
    fn an_error_status_ends_a_guarded_fetch() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(listener, vec![answer("404 Not Found", "", "")]);
        let res = get_guarded(
            &loopback_guarded(None),
            &format!("http://{addr}/a.zip"),
            &allow_loopback,
            None,
        );
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(
            matches!(res, Err(FetchError::Http(HttpError::Status(404, _)))),
            "{:?}",
            res.map(|r| r.status())
        );
    }

    #[test]
    fn the_api_agent_follows_four_redirects_without_the_token() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hop = |n: usize| answer("302 Found", &format!("Location: http://{addr}/{n}\r\n"), "");
        let mut answers: Vec<String> = (1..=4).map(hop).collect();
        answers.push(answer("200 OK", "", "{\"ok\":true}"));
        let server = serve(listener, answers);
        let agent = build_api_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let resp = agent
            .get(&format!("http://{addr}/0"))
            .header("Authorization", "Bearer ghp_TESTtest0123456789")
            .call_checked()
            .unwrap();
        let v: serde_json::Value = resp.into_json().unwrap();
        assert_eq!(v["ok"], true);
        let got = server.join().unwrap();
        assert_eq!(got.len(), 5);
        assert!(got[0].contains("ghp_TESTtest0123456789"));
        // ureq 2 and 3 both drop Authorization on every redirect (RedirectAuthHeaders::Never).
        for later in &got[1..] {
            assert!(
                !later.to_ascii_lowercase().contains("authorization"),
                "{later}"
            );
        }

        // A fifth redirect is too many.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hop = |n: usize| answer("302 Found", &format!("Location: http://{addr}/{n}\r\n"), "");
        let server = serve(listener, (1..=5).map(hop).collect());
        let e = agent
            .get(&format!("http://{addr}/0"))
            .call_checked()
            .unwrap_err();
        assert!(
            matches!(&e, HttpError::Transport(t) if matches!(t.error(), ureq::Error::TooManyRedirects)),
            "{e}"
        );
        assert!(
            e.to_string()
                .ends_with("Too Many Redirects: reached max redirects (5)"),
            "{e}"
        );
        assert_eq!(server.join().unwrap().len(), 5);
    }

    #[test]
    fn bodiless_writes_send_content_length_zero_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(
            listener,
            vec![
                answer("204 No Content", "", ""),
                answer("204 No Content", "", ""),
            ],
        );
        let url = format!("http://{addr}/user/starred/o/r");
        quick_agent()
            .put(&url)
            .header("Content-Length", "0")
            .send_empty_checked()
            .unwrap();
        quick_agent()
            .delete(&url)
            .header("Content-Length", "0")
            .call_checked()
            .unwrap();
        for req in server.join().unwrap() {
            let lower = req.to_ascii_lowercase();
            assert_eq!(lower.matches("content-length: 0").count(), 1, "{req}");
            assert!(!lower.contains("transfer-encoding"), "{req}");
        }
    }

    #[test]
    fn json_and_form_bodies_are_sent_as_before() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(
            listener,
            vec![answer("200 OK", "", "{}"), answer("200 OK", "", "{}")],
        );
        let url = format!("http://{addr}/x");
        quick_agent()
            .post(&url)
            .send_json_checked(serde_json::json!({ "files": { "a b": { "content": "c" } } }))
            .unwrap();
        quick_agent()
            .post(&url)
            .header("Accept", "application/json")
            .send_form_checked(&[("client_id", "abc"), ("scope", "read:user public_repo")])
            .unwrap();
        let got = server.join().unwrap();
        let json = got[0].to_ascii_lowercase();
        assert!(
            json.contains("content-type: application/json\r\n"),
            "{json}"
        );
        assert!(
            got[0].ends_with("{\"files\":{\"a b\":{\"content\":\"c\"}}}"),
            "{}",
            got[0]
        );
        let form = got[1].to_ascii_lowercase();
        assert!(
            form.contains("content-type: application/x-www-form-urlencoded"),
            "{form}"
        );
        assert!(form.contains("user-agent: silo/"), "{form}");
        assert!(
            got[1].ends_with("client_id=abc&scope=read%3Auser+public_repo"),
            "{}",
            got[1]
        );
    }

    #[test]
    fn body_helpers_keep_ureq_2_limits() {
        const MB10: usize = 10 * 1024 * 1024;
        // into_string: up to 10 MB, invalid UTF-8 replaced; more is refused.
        let exact = canned(200, &[], &vec![b'a'; MB10]);
        assert_eq!(exact.into_string().unwrap().len(), MB10);
        let over = canned(200, &[], &vec![b'a'; MB10 + 1]);
        assert!(over.into_string().is_err());
        let bad = canned(200, &[], b"caf\xe9");
        assert_eq!(bad.into_string().unwrap(), "caf\u{fffd}");
        // into_json: no limit (ureq 3's read_json stops at 10 MB).
        let big = format!("\"{}\"", "a".repeat(MB10 + 10));
        let s: String = canned(200, &[], big.as_bytes()).into_json().unwrap();
        assert_eq!(s.len(), MB10 + 10);
        let e = canned(200, &[], b"{nope")
            .into_json::<serde_json::Value>()
            .unwrap_err();
        assert!(e.to_string().starts_with("Failed to read JSON"), "{e}");
        // into_reader: no limit either.
        let mut n = 0;
        let mut r = canned(200, &[], &vec![0u8; MB10 + 1]).into_reader();
        let mut buf = [0u8; 65536];
        loop {
            match r.read(&mut buf).unwrap() {
                0 => break,
                k => n += k,
            }
        }
        assert_eq!(n, MB10 + 1);
        // header(): case-insensitive, None when absent.
        let h = canned(200, &[("X-OAuth-Scopes", "gist, repo")], b"");
        assert_eq!(h.header("x-oauth-scopes"), Some("gist, repo"));
        assert_eq!(h.header("retry-after"), None);
    }

    /// Serve up to `max` requests on `listener`, answering each after `delay` with a 302 to
    /// `location`. The thread returns how many it answered, once no request comes for 2 s.
    fn serve_redirects(
        listener: std::net::TcpListener,
        max: usize,
        delay: Duration,
        location: String,
    ) -> std::thread::JoinHandle<usize> {
        use std::io::Write;
        std::thread::spawn(move || {
            let mut served = 0;
            while served < max {
                let deadline = Instant::now() + Duration::from_secs(2);
                let Some(mut s) = accept_within(&listener, deadline) else {
                    break;
                };
                let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                std::thread::sleep(delay);
                let head = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = s.write_all(head.as_bytes());
                served += 1;
            }
            served
        })
    }

    /// The guarded agents refuse loopback outright, so the tests against a local server drive
    /// the same loop with a plain agent that, like them, doesn't follow redirects, and a
    /// check that allows this loopback server (and still refuses every private address).
    fn loopback_guarded(overall: Option<Duration>) -> GuardedAgent {
        GuardedAgent {
            agent: quick_agent(),
            overall,
        }
    }

    fn allow_loopback(u: &Url) -> Result<(), String> {
        validate_outbound_url(u.as_str(), true).map(drop)
    }

    #[test]
    fn a_real_redirect_to_a_private_address_is_not_followed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve_redirects(listener, 1, Duration::ZERO, "http://192.168.1.1/".into());
        let res = get_guarded(
            &loopback_guarded(None),
            &format!("http://{addr}/a.zip"),
            &allow_loopback,
            None,
        );
        assert_eq!(server.join().unwrap(), 1);
        let err = res.unwrap_err().to_string();
        assert!(err.contains("refused a redirect"), "{err}");
    }

    #[test]
    fn redirects_share_one_overall_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Each hop takes 300 ms: under the 700 ms cap on its own, but not all of them together.
        let server = serve_redirects(
            listener,
            MAX_REDIRECTS + 1,
            Duration::from_millis(300),
            format!("http://{addr}/next"),
        );
        let start = Instant::now();
        let res = get_guarded(
            &loopback_guarded(Some(Duration::from_millis(700))),
            &format!("http://{addr}/a.zip"),
            &allow_loopback,
            None,
        );
        let took = start.elapsed();
        let served = server.join().unwrap();
        let err = res.unwrap_err();
        assert!(matches!(err, FetchError::Http(_)), "{err}");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(served < MAX_REDIRECTS, "served {served}");
        assert!(took < Duration::from_millis(1500), "took {took:?}");
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
        let res = agent.get(&format!("http://{addr}/")).call_checked();
        let took = start.elapsed();
        let _ = done_tx.send(());
        hold.join().unwrap();
        let err = res.unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(took < Duration::from_secs(5), "took {took:?}");
    }

    #[test]
    fn a_slow_but_steady_download_is_not_cut_off() {
        use std::io::Write;
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
            .call_checked()
            .map_err(|e| e.to_string())
            .and_then(|r| r.into_string().map_err(|e| e.to_string()));
        let took = start.elapsed();
        server.join().unwrap();
        assert_eq!(body.as_deref(), Ok("abcdefgh"));
        assert!(took >= Duration::from_millis(1000), "took {took:?}");
    }

    #[test]
    fn a_body_that_stalls_mid_download_times_out() {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        // Send the head and part of the body, then go quiet (but keep the socket open).
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let Some(mut s) = accept_within(&listener, deadline) else {
                return;
            };
            let _ = read_request(&mut s);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc");
            let _ = s.flush();
            let _ = done_rx.recv_timeout(Duration::from_secs(10));
        });
        let agent = build_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let start = std::time::Instant::now();
        let resp = agent
            .get(&format!("http://{addr}/"))
            .call_checked()
            .unwrap();
        let mut body = Vec::new();
        let res = resp.into_reader().read_to_end(&mut body);
        let took = start.elapsed();
        let _ = done_tx.send(());
        server.join().unwrap();
        let err = res.unwrap_err();
        assert!(err.to_string().contains("timeout"), "{err}");
        // ureq 2's kind, so code that checks for `TimedOut` (and `into_json`) still sees it.
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");
        assert!(took < Duration::from_secs(5), "took {took:?}");
    }

    #[test]
    fn a_public_address_after_sixteen_private_ones_is_kept() {
        // ureq's own resolver keeps only the first 16 answers; the guard filters first.
        let mut all: Vec<SocketAddr> = (1..=16)
            .map(|i| format!("10.0.0.{i}:443").parse().unwrap())
            .collect();
        all.push("140.82.112.3:443".parse().unwrap());
        assert_eq!(
            guarded_addrs("many.example.com:443", all).unwrap(),
            vec!["140.82.112.3:443".parse::<SocketAddr>().unwrap()]
        );
        // More than 16 public answers: the first 16, as many as ureq takes.
        let all: Vec<SocketAddr> = (1..=20)
            .map(|i| format!("140.82.112.{i}:443").parse().unwrap())
            .collect();
        let kept = guarded_addrs("cdn.example.com:443", all.clone()).unwrap();
        assert_eq!(kept, all[..MAX_ADDRS].to_vec());
        // Nothing public: refused.
        let private: Vec<SocketAddr> = vec!["192.168.0.1:443".parse().unwrap()];
        assert!(guarded_addrs("evil.example.com:443", private).is_err());
    }

    #[test]
    fn a_lookup_without_a_timeout_finds_every_address() {
        let none = NextTimeout {
            after: ureq::unversioned::transport::time::Duration::NotHappening,
            reason: ureq::Timeout::Resolve,
            per_read: None,
            per_write: None,
        };
        let found = lookup_within("127.0.0.1:443", none).unwrap();
        assert_eq!(found, vec!["127.0.0.1:443".parse::<SocketAddr>().unwrap()]);
        // With a budget, the lookup runs on its own thread and still answers.
        let some = NextTimeout {
            after: ureq::unversioned::transport::time::Duration::Exact(Duration::from_secs(5)),
            ..none
        };
        assert_eq!(lookup_within("127.0.0.1:443", some).unwrap(), found);
    }

    #[test]
    fn a_refused_connection_reads_as_offline_even_as_text() {
        // A port nothing listens on: bind, note it, close it.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // Windows retries a refused loopback connect for about 2 s before giving up.
        let agent = build_agent(Duration::from_secs(8), Duration::from_millis(500), None);
        let e = agent
            .get(&format!("http://127.0.0.1:{port}/"))
            .call_checked()
            .unwrap_err();
        let text = e.to_string();
        assert!(text.contains("Connection Failed"), "{text}");
        // Callers like gh_err and catalog_err hand the text on; it must classify the same way
        // as the typed error does.
        assert_eq!(
            crate::error::AppError::from(text.clone()).code,
            crate::error::code::NET_OFFLINE,
            "{text}"
        );
        assert_eq!(
            crate::error::AppError::from(e).code,
            crate::error::code::NET_OFFLINE
        );
    }

    #[test]
    fn api_does_not_replay_a_write_on_a_307() {
        // ureq 2 handed back the 307 as a bare success; ureq 3 refuses to resend a body, and
        // Silo reports that as a failure rather than a write that never happened.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(
            listener,
            vec![answer(
                "307 Temporary Redirect",
                &format!("Location: http://{addr}/elsewhere\r\n"),
                "",
            )],
        );
        let agent = build_api_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let e = agent
            .put(&format!("http://{addr}/x"))
            .send_json_checked(serde_json::json!({ "a": 1 }))
            .unwrap_err();
        assert!(
            matches!(&e, HttpError::Transport(t) if matches!(t.error(), ureq::Error::RedirectFailed)),
            "{e}"
        );
        assert_eq!(
            server.join().unwrap().len(),
            1,
            "the body is sent once only"
        );
    }

    #[test]
    fn writes_redirected_with_307_or_308_fail_without_resending() {
        // DELETE, an empty PUT and a JSON PATCH: ureq 3 won't repeat any of them at the new
        // location (ureq 2 returned the 3xx as a success); Silo reports a failure.
        for status in ["307 Temporary Redirect", "308 Permanent Redirect"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let moved = || answer(status, &format!("Location: http://{addr}/new\r\n"), "");
            let server = serve(listener, vec![moved(), moved(), moved()]);
            let agent =
                build_api_agent(Duration::from_millis(500), Duration::from_millis(500), None);
            let url = format!("http://{addr}/x");
            let results = [
                agent.delete(&url).call_checked().map(drop),
                agent
                    .put(&url)
                    .header("Content-Length", "0")
                    .send_empty_checked()
                    .map(drop),
                agent
                    .patch(&url)
                    .send_json_checked(serde_json::json!({ "a": 1 }))
                    .map(drop),
            ];
            for r in results {
                let e = r.unwrap_err();
                assert!(
                    matches!(&e, HttpError::Transport(t) if matches!(t.error(), ureq::Error::RedirectFailed)),
                    "{status}: {e}"
                );
            }
            let got = server.join().unwrap();
            assert_eq!(got.len(), 3, "{status}: one request each, none resent");
            assert!(got.iter().all(|r| r.contains(" /x ")), "{got:?}");
        }
    }

    #[test]
    fn a_status_after_a_redirect_names_both_urls() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = serve(
            listener,
            vec![
                answer("301 Moved", &format!("Location: http://{addr}/new\r\n"), ""),
                answer("404 Not Found", "", ""),
            ],
        );
        let agent = build_api_agent(Duration::from_millis(500), Duration::from_millis(500), None);
        let e = agent
            .get(&format!("http://{addr}/old"))
            .call_checked()
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            format!("http://{addr}/new: status code 404 (redirected from http://{addr}/old)")
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }

    /// A clock tests move by hand.
    struct FakeClock(std::cell::Cell<Instant>);

    impl FakeClock {
        fn new() -> Self {
            FakeClock(std::cell::Cell::new(Instant::now()))
        }
        fn now(&self) -> Instant {
            self.0.get()
        }
        fn advance(&self, d: Duration) {
            self.0.set(self.0.get() + d);
        }
    }

    fn budget(after: Option<Duration>) -> NextTimeout {
        use ureq::unversioned::transport::time::Duration as B;
        NextTimeout {
            after: after.map_or(B::NotHappening, B::Exact),
            reason: ureq::Timeout::Connect,
            per_read: None,
            per_write: None,
        }
    }

    fn addrs(n: usize) -> Vec<SocketAddr> {
        (1..=n)
            .map(|i| format!("140.82.112.{i}:443").parse().unwrap())
            .collect()
    }

    fn refused() -> ureq::Error {
        ureq::Error::Io(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"))
    }

    #[test]
    fn slow_refusals_share_one_connect_deadline() {
        // Sixteen addresses that each refuse after 2 s, with a 10 s budget: ureq 3's own loop
        // gives every one of them ~5 s and so takes ~32 s; Silo's stops at the deadline.
        let clock = FakeClock::new();
        let start = clock.now();
        let mut given = Vec::new();
        let res: Result<(), _> = connect_each(
            &addrs(16),
            budget(Some(Duration::from_secs(10))),
            &|| clock.now(),
            |_, t| {
                given.push(*t.after);
                clock.advance((*t.after).min(Duration::from_secs(2)));
                Err(refused())
            },
        );
        let spent = clock.now() - start;
        // The budget, plus at most one attempt at ureq's 10 ms floor.
        assert!(
            spent <= Duration::from_secs(10) + MIN_ATTEMPT,
            "spent {spent:?}"
        );
        assert!(given.iter().all(|g| *g >= MIN_ATTEMPT), "{given:?}");
        assert!(
            matches!(res, Err(ureq::Error::Timeout(ureq::Timeout::Connect))),
            "{res:?}"
        );
        // ureq 2's split: half of what's left, every time.
        assert_eq!(given[0], Duration::from_secs(5));
        assert_eq!(given[1], Duration::from_millis(4000));
        assert!(given.windows(2).all(|w| w[1] <= w[0]), "{given:?}");
    }

    #[test]
    fn a_later_address_can_still_connect() {
        let clock = FakeClock::new();
        let mut tried = Vec::new();
        let res = connect_each(
            &addrs(3),
            budget(Some(Duration::from_secs(10))),
            &|| clock.now(),
            |a, _| {
                tried.push(a);
                if tried.len() < 3 {
                    clock.advance(Duration::from_millis(100));
                    Err(refused())
                } else {
                    Ok(a)
                }
            },
        );
        assert_eq!(res.unwrap(), addrs(3)[2]);
        assert_eq!(tried, addrs(3));
        // Every address refusing at once: the last refusal, not a timeout.
        let res: Result<(), _> = connect_each(
            &addrs(3),
            budget(Some(Duration::from_secs(10))),
            &|| clock.now(),
            |_, _| Err(refused()),
        );
        assert!(matches!(res, Err(ureq::Error::Io(_))), "{res:?}");
        // A single address gets the whole budget; with no budget, none is set.
        let res = connect_each(
            &addrs(1),
            budget(Some(Duration::from_secs(10))),
            &|| clock.now(),
            |_, t| Ok(*t.after),
        );
        assert_eq!(res.unwrap(), Duration::from_secs(10));
        let res = connect_each(&addrs(2), budget(None), &|| clock.now(), |_, t| {
            Ok(t.after.is_not_happening())
        });
        assert!(res.unwrap());
        // Any other error (a TLS failure, say) ends it at once.
        let mut calls = 0;
        let res: Result<(), _> = connect_each(
            &addrs(3),
            budget(Some(Duration::from_secs(10))),
            &|| clock.now(),
            |_, _| {
                calls += 1;
                Err(ureq::Error::Tls("bad certificate"))
            },
        );
        assert!(matches!(res, Err(ureq::Error::Tls(_))), "{res:?}");
        assert_eq!(calls, 1);
    }
}
