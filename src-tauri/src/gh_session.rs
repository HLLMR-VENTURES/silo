//! Which GitHub token may be sent right now. One `Session` orders three things:
//!
//! * **Requests.** A stored token is read as a [`Bearer`] tied to the session it was read in,
//!   and every request asks for admission ([`Bearer::admit`]) just before it goes out. A
//!   Disconnect or a new sign-in ends the session, so a token copied before then is never sent
//!   afterwards. Disconnect also waits for the requests already admitted to finish.
//! * **Sign-ins.** Each device flow, PAT save and cancel starts a new sign-in attempt; only the
//!   newest attempt may store its token ([`Session::commit`]), and its own requests (the code
//!   exchange, `whoami`) are admitted only while it's the newest. An older sign-in that finishes
//!   late can't overwrite a newer one.
//! * **Device flows.** Only the newest attempt's flow is remembered, until GitHub's own expiry,
//!   so a stale start can't push out a valid flow.
//!
//! After a Disconnect, no stored token is used in this run of Silo until a sign-in commits,
//! even if removing it from storage failed.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long Disconnect waits for requests it already admitted. Each one is bounded by the HTTP
/// agent's own timeouts (`net::api`: 60 s overall), so this only matters for a stuck one.
const DRAIN: Duration = Duration::from_secs(75);

/// A device code is never remembered longer than this, whatever GitHub's `expires_in` says.
const MAX_FLOW_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// Why Silo didn't send an authenticated request.
pub const NOT_SENT: &str = "Silo didn't send this to GitHub: the account was disconnected or \
     changed meanwhile. Try again.";

/// Why a sign-in didn't finish: a newer one, a cancel or a Disconnect replaced it.
pub const SUPERSEDED: &str = "This sign-in was cancelled or replaced by a newer one.";

pub struct Session {
    state: Mutex<State>,
    /// Signalled whenever an admitted request finishes.
    idle: Condvar,
}

struct State {
    /// Bumped when a Disconnect finishes and when a sign-in stores a token: a token read under
    /// an older value may be stale.
    session: u64,
    /// Bumped by every sign-in start, cancel, commit and Disconnect: only the newest may commit.
    attempt: u64,
    /// Disconnects running now. Nothing is admitted meanwhile.
    closing: usize,
    /// A Disconnect ran and no sign-in has stored a token since: stored tokens aren't used.
    signed_out: bool,
    /// Admitted requests still running.
    in_flight: usize,
    /// The newest attempt's device flow.
    flow: Option<Flow>,
}

struct Flow {
    device_code: String,
    attempt: u64,
    expires: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ticket {
    /// A stored token, read under this session.
    Session(u64),
    /// A sign-in's own new token, for this attempt.
    Attempt(u64),
}

/// What Silo knows about a device code a poll asks about.
#[derive(Debug, PartialEq)]
pub enum FlowState {
    /// The newest sign-in's flow, still within GitHub's lifetime: poll it for this attempt.
    Live(u64),
    /// That flow, past its lifetime.
    Expired,
    /// Not the newest flow (cancelled, replaced, Disconnected, finished) or never issued.
    Gone,
}

/// A GitHub token tied to the session (or sign-in attempt) it belongs to. It can only be put on
/// a request through [`Bearer::admit`]. Deliberately not `Debug` or `Clone`.
pub struct Bearer<'s> {
    secret: String,
    ticket: Ticket,
    gate: &'s Session,
}

/// One admitted request. Holds Disconnect off until it's dropped, so keep it only around the
/// request itself.
pub struct Admitted<'s> {
    _pass: Pass<'s>,
    header: String,
}

/// A slot in the in-flight count.
pub struct Pass<'s>(&'s Session);

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        let mut g = self.0.lock();
        g.in_flight = g.in_flight.saturating_sub(1);
        drop(g);
        self.0.idle.notify_all();
    }
}

impl Admitted<'_> {
    /// The `Authorization` header value.
    pub fn header(&self) -> &str {
        &self.header
    }
}

impl<'s> Bearer<'s> {
    /// Admit one request carrying this token: only while its session (or sign-in) is still the
    /// current one and no Disconnect is running.
    pub fn admit(&self) -> Result<Admitted<'s>, String> {
        let pass = self.gate.pass(self.ticket)?;
        Ok(Admitted {
            _pass: pass,
            header: format!("Bearer {}", self.secret),
        })
    }
}

/// For a read that also works without an account: the header when the token is admitted,
/// otherwise None, and the request goes anonymously (never with a token that's no longer
/// current).
pub fn admit_optional<'s>(token: Option<&Bearer<'s>>) -> Option<Admitted<'s>> {
    token.and_then(|t| t.admit().ok())
}

impl Session {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                session: 0,
                attempt: 0,
                closing: 0,
                signed_out: false,
                in_flight: 0,
                flow: None,
            }),
            idle: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn current(g: &State, ticket: Ticket) -> bool {
        g.closing == 0
            && match ticket {
                Ticket::Session(s) => !g.signed_out && g.session == s,
                Ticket::Attempt(a) => g.attempt == a,
            }
    }

    fn pass(&self, ticket: Ticket) -> Result<Pass<'_>, String> {
        let mut g = self.lock();
        if !Self::current(&g, ticket) {
            return Err(match ticket {
                Ticket::Session(_) => NOT_SENT,
                Ticket::Attempt(_) => SUPERSEDED,
            }
            .to_string());
        }
        g.in_flight += 1;
        Ok(Pass(self))
    }

    /// The stored token as a [`Bearer`] for the current session. None when signed out, while a
    /// Disconnect runs, or when `read` finds nothing. The session is noted before `read` runs, so
    /// a Disconnect or sign-in that lands in between makes the token's requests refused rather
    /// than sent under the wrong session.
    pub fn bearer(&self, read: impl FnOnce() -> Option<String>) -> Option<Bearer<'_>> {
        let session = {
            let g = self.lock();
            if g.closing > 0 || g.signed_out {
                return None;
            }
            g.session
        };
        Some(Bearer {
            secret: read()?,
            ticket: Ticket::Session(session),
            gate: self,
        })
    }

    /// Whether a Disconnect happened in this run and no sign-in stored a token since.
    pub fn signed_out(&self) -> bool {
        let g = self.lock();
        g.signed_out || g.closing > 0
    }

    /// Start a sign-in attempt (a device flow or a PAT save). Every older attempt loses its
    /// right to store a token or send requests.
    pub fn begin(&self) -> u64 {
        let mut g = self.lock();
        g.attempt += 1;
        g.flow = None;
        g.attempt
    }

    /// A sign-in's new token, for its own requests (`whoami`) while it's the newest attempt.
    pub fn sign_in_bearer(&self, attempt: u64, token: &str) -> Bearer<'_> {
        Bearer {
            secret: token.to_string(),
            ticket: Ticket::Attempt(attempt),
            gate: self,
        }
    }

    /// Admit a sign-in's unauthenticated request (the device-code exchange), which still yields
    /// a token: only while `attempt` is the newest.
    pub fn admit_attempt(&self, attempt: u64) -> Result<Pass<'_>, String> {
        self.pass(Ticket::Attempt(attempt))
    }

    /// Remember the device flow `attempt` started, for its polls, until GitHub's `expires_in`
    /// (seconds) runs out. Refused when a newer attempt (or a cancel, or a Disconnect) came
    /// since, so a start that answers late can't replace the newer flow.
    pub fn remember_flow(
        &self,
        device_code: &str,
        attempt: u64,
        expires_in: u64,
    ) -> Result<(), String> {
        self.remember_flow_at(device_code, attempt, expires_in, Instant::now())
    }

    fn remember_flow_at(
        &self,
        device_code: &str,
        attempt: u64,
        expires_in: u64,
        now: Instant,
    ) -> Result<(), String> {
        let mut g = self.lock();
        if g.attempt != attempt || g.closing > 0 {
            return Err(SUPERSEDED.into());
        }
        let life = Duration::from_secs(expires_in).min(MAX_FLOW_LIFETIME);
        g.flow = Some(Flow {
            device_code: device_code.to_string(),
            attempt,
            expires: now + life,
        });
        Ok(())
    }

    /// What a poll for `device_code` may do.
    pub fn flow(&self, device_code: &str) -> FlowState {
        self.flow_at(device_code, Instant::now())
    }

    fn flow_at(&self, device_code: &str, now: Instant) -> FlowState {
        let mut g = self.lock();
        let Some(f) = g.flow.as_ref() else {
            return FlowState::Gone;
        };
        if f.device_code != device_code || f.attempt != g.attempt || g.closing > 0 {
            return FlowState::Gone;
        }
        if now >= f.expires {
            g.flow = None;
            return FlowState::Expired;
        }
        FlowState::Live(f.attempt)
    }

    /// Cancel the device flow for `device_code`. True when it was the newest sign-in and is
    /// now cancelled; false when it was already gone (finished, replaced, expired, or never
    /// remembered), so the caller should re-read the connection state.
    pub fn cancel_flow(&self, device_code: &str) -> bool {
        let mut g = self.lock();
        let live = matches!(&g.flow, Some(f) if f.device_code == device_code);
        if !live {
            return false;
        }
        let newest = g.flow.as_ref().is_some_and(|f| f.attempt == g.attempt);
        g.flow = None;
        if newest {
            g.attempt += 1;
        }
        newest
    }

    /// Store a sign-in's token, only if `attempt` is still the newest and no Disconnect is
    /// running; None (and `store` didn't run) otherwise. Runs under the lock, so a Disconnect
    /// or a newer attempt is ordered entirely before or after it. Once `store` ran, tokens read
    /// before are stale (the session moves on); on Ok the account counts as signed in again.
    pub fn commit<T, E>(
        &self,
        attempt: u64,
        store: impl FnOnce() -> Result<T, E>,
    ) -> Option<Result<T, E>> {
        let mut g = self.lock();
        if g.attempt != attempt || g.closing > 0 {
            return None;
        }
        let r = store();
        g.session += 1;
        g.attempt += 1; // this attempt is done
        g.flow = None;
        if r.is_ok() {
            g.signed_out = false;
        }
        Some(r)
    }

    /// Disconnect. At once: nothing more is admitted and every sign-in still out loses its right
    /// to store. Then it waits (up to [`DRAIN`]) for the requests already admitted, runs `clear`
    /// under the lock, and ends the session: a token read before can't be sent afterwards, and
    /// no stored token is used until a sign-in commits.
    pub fn sign_out<T>(&self, clear: impl FnOnce() -> T) -> T {
        self.sign_out_within(DRAIN, clear)
    }

    fn sign_out_within<T>(&self, drain: Duration, clear: impl FnOnce() -> T) -> T {
        let mut g = self.lock();
        g.closing += 1;
        g.attempt += 1;
        g.flow = None;
        let deadline = Instant::now() + drain;
        while g.in_flight > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                log::warn!(
                    "GitHub: {} request(s) still running at Disconnect; signing out anyway",
                    g.in_flight
                );
                break;
            }
            g = self
                .idle
                .wait_timeout(g, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        // Ends the session even if `clear` panics.
        struct Reopen<'a>(MutexGuard<'a, State>);
        impl Drop for Reopen<'_> {
            fn drop(&mut self) {
                self.0.session += 1;
                self.0.signed_out = true;
                self.0.closing -= 1;
            }
        }
        let _reopen = Reopen(g);
        clear()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::Arc;

    fn stored<'s>(gate: &'s Session, tok: &str) -> Bearer<'s> {
        gate.bearer(|| Some(tok.to_string())).unwrap()
    }

    #[test]
    fn a_token_read_before_a_disconnect_is_never_sent_after_it() {
        let gate = Session::new();
        let a = stored(&gate, "tok-a");
        assert_eq!(a.admit().unwrap().header(), "Bearer tok-a");
        gate.sign_out(|| ());
        assert_eq!(a.admit().err().as_deref(), Some(NOT_SENT));
        assert!(
            admit_optional(Some(&a)).is_none(),
            "an anonymous read goes without it"
        );
        // Signed out: whatever storage still returns isn't used.
        assert!(gate.bearer(|| Some("left-behind".into())).is_none());
        assert!(gate.signed_out());
    }

    #[test]
    fn a_new_sign_in_retires_tokens_read_before_it() {
        let gate = Session::new();
        let a = stored(&gate, "tok-a");
        let attempt = gate.begin();
        assert!(
            a.admit().is_ok(),
            "starting a sign-in doesn't change the account yet"
        );
        assert_eq!(gate.commit(attempt, || Ok::<_, ()>(())), Some(Ok(())));
        assert!(a.admit().is_err());
        assert!(stored(&gate, "tok-b").admit().is_ok());
    }

    #[test]
    fn the_session_is_noted_before_the_token_is_read() {
        // A Disconnect lands while the token is being read (outside the lock): that token is
        // never admitted.
        let gate = Session::new();
        let b = gate
            .bearer(|| {
                gate.sign_out(|| ());
                Some("tok-a".to_string())
            })
            .unwrap();
        assert!(b.admit().is_err());
        // A sign-in that commits in between does the same.
        let gate = Session::new();
        let attempt = gate.begin();
        let b = gate
            .bearer(|| {
                gate.commit(attempt, || Ok::<_, ()>(())).unwrap().unwrap();
                Some("tok-a".to_string())
            })
            .unwrap();
        assert!(b.admit().is_err());
    }

    #[test]
    fn disconnect_waits_for_admitted_requests_and_admits_nothing_meanwhile() {
        let gate = Arc::new(Session::new());
        let (sent_tx, sent_rx) = mpsc::channel::<()>();
        let (finish_tx, finish_rx) = mpsc::channel::<()>();
        let request = {
            let gate = gate.clone();
            std::thread::spawn(move || {
                let b = stored(&gate, "tok");
                let admitted = b.admit().unwrap();
                sent_tx.send(()).unwrap();
                finish_rx.recv().unwrap(); // the request is on the wire
                drop(admitted);
            })
        };
        sent_rx.recv().unwrap();
        let other = stored(&gate, "tok");
        let (cleared_tx, cleared_rx) = mpsc::channel::<()>();
        let logout = {
            let gate = gate.clone();
            std::thread::spawn(move || gate.sign_out(|| cleared_tx.send(()).unwrap()))
        };
        // Disconnect has started: a new request isn't admitted, and the clear waits.
        while !gate.signed_out() {
            std::thread::yield_now();
        }
        assert!(other.admit().is_err());
        assert!(gate.bearer(|| Some("tok".into())).is_none());
        assert!(cleared_rx.recv_timeout(Duration::from_millis(100)).is_err());
        finish_tx.send(()).unwrap();
        cleared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        logout.join().unwrap();
        request.join().unwrap();
    }

    #[test]
    fn disconnect_stops_waiting_for_a_stuck_request() {
        let gate = Session::new();
        let b = stored(&gate, "tok");
        let stuck = b.admit().unwrap();
        assert_eq!(
            gate.sign_out_within(Duration::from_millis(20), || "cleared"),
            "cleared"
        );
        drop(stuck);
        assert!(b.admit().is_err());
    }

    #[test]
    fn only_the_newest_sign_in_may_store() {
        // Flow A's poll is out; the user cancels, signs in as B, then A's poll answers.
        let gate = Session::new();
        let a = gate.begin();
        gate.remember_flow("code-a", a, 900).unwrap();
        assert!(gate.cancel_flow("code-a"));
        let b = gate.begin();
        gate.remember_flow("code-b", b, 900).unwrap();
        assert_eq!(
            gate.commit(a, || Ok::<_, ()>("A")),
            None,
            "A can't overwrite B"
        );
        assert_eq!(gate.commit(b, || Ok::<_, ()>("B")), Some(Ok("B")));
        // An attempt commits once.
        assert_eq!(gate.commit(b, || Ok::<_, ()>("B again")), None);
        // A PAT save started after a device flow replaces it too.
        let flow = gate.begin();
        let pat = gate.begin();
        assert_eq!(gate.commit(flow, || Ok::<_, ()>(())), None);
        assert!(gate.commit(pat, || Ok::<_, ()>(())).is_some());
        // And a Disconnect beats every sign-in already out.
        let late = gate.begin();
        gate.sign_out(|| ());
        assert_eq!(gate.commit(late, || Ok::<_, ()>(())), None);
    }

    #[test]
    fn a_superseded_sign_in_sends_nothing() {
        let gate = Session::new();
        let a = gate.begin();
        let whoami = gate.sign_in_bearer(a, "new-a");
        assert!(gate.admit_attempt(a).is_ok());
        assert!(whoami.admit().is_ok());
        gate.begin();
        assert_eq!(gate.admit_attempt(a).err().as_deref(), Some(SUPERSEDED));
        assert_eq!(whoami.admit().err().as_deref(), Some(SUPERSEDED));
        // A Disconnect does the same.
        let c = gate.begin();
        gate.sign_out(|| ());
        assert!(gate.admit_attempt(c).is_err());
    }

    #[test]
    fn a_failed_store_still_moves_the_session_on() {
        let gate = Session::new();
        let old = stored(&gate, "tok-a");
        gate.sign_out(|| ());
        let attempt = gate.begin();
        assert_eq!(
            gate.commit(attempt, || Err::<(), _>("locked")),
            Some(Err("locked"))
        );
        assert!(old.admit().is_err());
        // Still signed out: the store failed, so nothing new was connected.
        assert!(gate.signed_out());
        let attempt = gate.begin();
        gate.commit(attempt, || Ok::<_, ()>(())).unwrap().unwrap();
        assert!(!gate.signed_out());
        assert!(stored(&gate, "tok-b").admit().is_ok());
    }

    #[test]
    fn a_stale_start_cant_replace_the_newer_flow() {
        let gate = Session::new();
        let a = gate.begin();
        let b = gate.begin();
        gate.remember_flow("code-b", b, 900).unwrap();
        // A's start answers after B's: refused, and B's flow is still there.
        assert_eq!(
            gate.remember_flow("code-a", a, 900).err().as_deref(),
            Some(SUPERSEDED)
        );
        assert_eq!(gate.flow("code-b"), FlowState::Live(b));
        assert_eq!(gate.flow("code-a"), FlowState::Gone);
        // However many stale starts answer.
        for i in 0..20 {
            let _ = gate.remember_flow(&format!("stale-{i}"), a, 900);
        }
        assert_eq!(gate.flow("code-b"), FlowState::Live(b));
        assert_eq!(gate.flow("never-issued"), FlowState::Gone);
    }

    #[test]
    fn a_flow_lives_as_long_as_github_says() {
        let gate = Session::new();
        let t0 = Instant::now();
        let a = gate.begin();
        gate.remember_flow_at("code", a, 900, t0).unwrap();
        assert_eq!(
            gate.flow_at("code", t0 + Duration::from_secs(899)),
            FlowState::Live(a)
        );
        assert_eq!(
            gate.flow_at("code", t0 + Duration::from_secs(900)),
            FlowState::Expired
        );
        assert_eq!(
            gate.flow_at("code", t0 + Duration::from_secs(901)),
            FlowState::Gone
        );
        // A huge expires_in is capped.
        let b = gate.begin();
        gate.remember_flow_at("long", b, u64::MAX, t0).unwrap();
        assert_eq!(
            gate.flow_at("long", t0 + MAX_FLOW_LIFETIME),
            FlowState::Expired
        );
    }

    #[test]
    fn cancel_only_ends_the_flow_it_names() {
        let gate = Session::new();
        let a = gate.begin();
        gate.remember_flow("code-a", a, 900).unwrap();
        assert!(!gate.cancel_flow("other"));
        assert_eq!(gate.flow("code-a"), FlowState::Live(a));
        // Cancelling A after B started doesn't touch B.
        let b = gate.begin();
        assert!(!gate.cancel_flow("code-a"));
        gate.remember_flow("code-b", b, 900).unwrap();
        assert!(!gate.cancel_flow("code-a"));
        assert_eq!(gate.flow("code-b"), FlowState::Live(b));
        assert!(gate.commit(b, || Ok::<_, ()>(())).is_some());
        // A cancel that arrives after the commit says the flow was already gone.
        assert!(!gate.cancel_flow("code-b"));
        // A cancel that wins: the poll can't commit afterwards.
        let c = gate.begin();
        gate.remember_flow("code-c", c, 900).unwrap();
        assert!(gate.cancel_flow("code-c"));
        assert_eq!(gate.commit(c, || Ok::<_, ()>(())), None);
        assert_eq!(gate.flow("code-c"), FlowState::Gone);
    }

    #[test]
    fn a_disconnect_forgets_the_flow() {
        let gate = Session::new();
        let a = gate.begin();
        gate.remember_flow("code", a, 900).unwrap();
        gate.sign_out(|| ());
        assert_eq!(gate.flow("code"), FlowState::Gone);
        // A start issued before the Disconnect answers after it: refused.
        assert!(gate.remember_flow("code", a, 900).is_err());
    }
}
