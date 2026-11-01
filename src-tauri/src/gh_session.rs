//! Which GitHub token may be sent right now. One `Session` orders three things:
//!
//! * **Requests.** A stored token is read as a [`Bearer`] tied to the session it was read in,
//!   and every request asks for admission ([`Bearer::admit`]) just before it goes out. A
//!   Disconnect or a new sign-in ends the session, so a token copied before then is never sent
//!   afterwards. Disconnect also waits for the requests already admitted to finish.
//! * **Sign-ins.** Each device flow, PAT save and cancel starts a new sign-in attempt; only the
//!   newest attempt may store its token ([`Session::commit`]), and its own requests (the code
//!   exchange, `whoami`) are admitted only while it's the newest. An older sign-in that finishes
//!   late can't overwrite a newer one, and one still out when a Disconnect ends can't land.
//! * **Device flows.** Only the newest attempt's flow is remembered, until GitHub's own expiry,
//!   so a stale start can't push out a valid flow.
//!
//! **Signed out.** After a Disconnect, no stored token is used until a sign-in commits, even if
//! removing it from storage failed. A file beside the database (the marker, see
//! [`Session::use_marker`]) keeps that across restarts: it's written before a Disconnect or a
//! sign-in touches storage, and removed only once storage is consistent again, so a crash, a
//! failed cleanup or a half-saved sign-in all leave the account signed out on the next run.

use std::path::PathBuf;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A device code is never remembered longer than this, whatever GitHub's `expires_in` says.
const MAX_FLOW_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// Why Silo didn't send an authenticated request.
pub const NOT_SENT: &str = "Silo didn't send this to GitHub: the account was disconnected or \
     changed meanwhile. Try again.";

/// Why a sign-in didn't finish: a newer one, a cancel or a Disconnect replaced it.
pub const SUPERSEDED: &str = "This sign-in was cancelled or replaced by a newer one.";

/// A sign-in couldn't write the marker before saving, so it saved nothing.
const MARK_FAILED: &str = "Silo couldn't save your GitHub sign-in, because it couldn't write a \
     file beside its database";

/// A sign-in saved everything but couldn't remove the marker an earlier Disconnect left.
const UNMARK_FAILED: &str = "Silo saved your GitHub sign-in but couldn't remove a leftover \
     file beside its database, so it stays disconnected. Connect again";

pub struct Session {
    state: Mutex<State>,
    /// Signalled whenever an admitted request finishes.
    idle: Condvar,
}

struct State {
    /// Bumped when a Disconnect finishes and when a sign-in stores a token: a token read under
    /// an older value may be stale.
    session: u64,
    /// Bumped by every sign-in start, cancel and commit, and when a Disconnect starts and ends:
    /// only the newest may commit.
    attempt: u64,
    /// Disconnects running now. Nothing is admitted meanwhile.
    closing: usize,
    /// No stored token may be used: a Disconnect ran (in this run, or in an earlier one whose
    /// cleanup failed) or a failed sign-in may have changed the token, and no sign-in has stored
    /// one since.
    signed_out: bool,
    /// Admitted requests still running.
    in_flight: usize,
    /// The newest attempt's device flow.
    flow: Option<Flow>,
    /// Where the signed-out marker lives (beside the database); None until the app sets it.
    marker: Option<PathBuf>,
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

/// Why a sign-in's store failed, and whether the stored token may have changed anyway (it was
/// written, but the rest wasn't). Then the account is signed out rather than left half-switched.
#[derive(Debug, PartialEq)]
pub struct StoreFailed {
    pub message: String,
    pub token_changed: bool,
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

/// A Disconnect that has started: nothing is admitted and no sign-in already out can store.
/// [`SignOut::finish`] clears storage; dropping it unfinished still ends the session.
pub struct SignOut<'s> {
    gate: &'s Session,
    /// The marker was written when the Disconnect started.
    marked: bool,
    done: bool,
}

impl Drop for SignOut<'_> {
    fn drop(&mut self) {
        if !self.done {
            let mut g = self.gate.lock();
            Session::end_sign_out(&mut g);
        }
    }
}

impl SignOut<'_> {
    /// Wait for the requests already admitted (each is bounded by the HTTP agent's timeouts),
    /// then run `clear` under the lock. `clear` is told whether the marker was written (when the
    /// Disconnect started), and returns whether storage is now clean (the token can't be read
    /// back and the account details are gone) along with its result; when it is, the marker is
    /// removed again. Then the session ends: a token read before is never sent, and no stored
    /// token is used until a sign-in commits.
    pub fn finish<T>(mut self, clear: impl FnOnce(bool) -> (bool, T)) -> T {
        let gate = self.gate;
        let marked = self.marked;
        let mut g = gate.lock();
        while g.in_flight > 0 {
            g = gate.idle.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        // Ends the session even if `clear` panics.
        struct End<'a>(MutexGuard<'a, State>);
        impl Drop for End<'_> {
            fn drop(&mut self) {
                Session::end_sign_out(&mut self.0);
            }
        }
        self.done = true;
        let end = End(g);
        let (clean, out) = clear(marked);
        if clean {
            let _ = Session::remove_marker(&end.0);
        }
        out
    }
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
                marker: None,
            }),
            idle: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keep the signed-out state in the file at `path` (beside the database). Call once at
    /// startup: a marker left by an earlier run means that run's Disconnect (or a failed
    /// sign-in) couldn't clean up storage, so the stored token isn't used.
    pub fn use_marker(&self, path: PathBuf) {
        let mut g = self.lock();
        if path.exists() {
            log::warn!("GitHub: an earlier Disconnect didn't finish; staying signed out");
            g.signed_out = true;
        }
        g.marker = Some(path);
    }

    /// Write the marker. Ok(true) when written, Ok(false) when there's no marker path (only
    /// before startup set it, and in tests).
    fn write_marker(g: &State) -> Result<bool, String> {
        let Some(p) = &g.marker else {
            return Ok(false);
        };
        std::fs::write(p, b"signed out\n").map_err(|e| {
            log::warn!("GitHub: couldn't write the signed-out marker: {e}");
            e.to_string()
        })?;
        Ok(true)
    }

    fn remove_marker(g: &State) -> Result<(), String> {
        let Some(p) = &g.marker else {
            return Ok(());
        };
        match std::fs::remove_file(p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                log::warn!("GitHub: couldn't remove the signed-out marker: {e}");
                Err(e.to_string())
            }
        }
    }

    fn marker_exists(g: &State) -> bool {
        g.marker.as_ref().is_some_and(|p| p.exists())
    }

    fn end_sign_out(g: &mut State) {
        g.session += 1;
        // A sign-in begun while this Disconnect ran is from before it ended: it can't land.
        g.attempt += 1;
        g.flow = None;
        g.signed_out = true;
        g.closing = g.closing.saturating_sub(1);
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

    /// Whether stored tokens are off limits now (signed out, or a Disconnect is running).
    pub fn signed_out(&self) -> bool {
        let g = self.lock();
        g.signed_out || g.closing > 0
    }

    /// Start a sign-in attempt (a device flow or a PAT save). Every older attempt loses its
    /// right to store a token or send requests. Call it first thing in the command, so attempts
    /// and Disconnects are ordered as the user clicked them.
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
    /// or a newer attempt is ordered entirely before or after it.
    ///
    /// The marker is written first (or nothing is stored), so a crash or a failure halfway
    /// leaves the account signed out on the next run, never half-switched. Once `store` ran,
    /// tokens read before are stale (the session moves on). On Ok, with the marker removed, the
    /// account is signed in. On a failure that may have changed the token it's signed out (the
    /// marker stays); on one that changed nothing, everything is as it was.
    pub fn commit<T>(
        &self,
        attempt: u64,
        store: impl FnOnce() -> Result<T, StoreFailed>,
    ) -> Option<Result<T, String>> {
        let mut g = self.lock();
        if g.attempt != attempt || g.closing > 0 {
            return None;
        }
        g.attempt += 1; // this attempt is done
        g.flow = None;
        let marked_before = Self::marker_exists(&g);
        if let Err(e) = Self::write_marker(&g) {
            return Some(Err(format!("{MARK_FAILED} ({e}).")));
        }
        let r = store();
        g.session += 1;
        Some(match r {
            Ok(v) => match Self::remove_marker(&g) {
                Ok(()) => {
                    g.signed_out = false;
                    Ok(v)
                }
                Err(e) => {
                    g.signed_out = true;
                    Err(format!("{UNMARK_FAILED} ({e})."))
                }
            },
            Err(e) if e.token_changed => {
                g.signed_out = true;
                Err(e.message)
            }
            Err(e) => {
                if !marked_before {
                    let _ = Self::remove_marker(&g);
                }
                Err(e.message)
            }
        })
    }

    /// Start a Disconnect, first thing in the command: from here nothing is admitted, and every
    /// sign-in already out (or begun before it ends) loses its right to store. Finish it with
    /// [`SignOut::finish`].
    /// The marker is written here, so even a crash before the Disconnect finishes leaves the
    /// account signed out on the next run.
    pub fn begin_sign_out(&self) -> SignOut<'_> {
        let mut g = self.lock();
        g.closing += 1;
        g.attempt += 1;
        g.flow = None;
        let marked = Self::write_marker(&g) == Ok(true);
        SignOut {
            gate: self,
            marked,
            done: false,
        }
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

    fn stored<'s>(gate: &'s Session, tok: &str) -> Bearer<'s> {
        gate.bearer(|| Some(tok.to_string())).unwrap()
    }

    fn sign_out(gate: &Session) {
        gate.begin_sign_out().finish(|_| (true, ()));
    }

    fn ok() -> Result<(), StoreFailed> {
        Ok(())
    }

    fn failed(token_changed: bool) -> Result<(), StoreFailed> {
        Err(StoreFailed {
            message: "busy".into(),
            token_changed,
        })
    }

    /// A fresh marker path in the temp folder, removed first.
    fn marker_path(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("silo_gh_marker_{}_{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn a_token_read_before_a_disconnect_is_never_sent_after_it() {
        let gate = Session::new();
        let a = stored(&gate, "tok-a");
        assert_eq!(a.admit().unwrap().header(), "Bearer tok-a");
        sign_out(&gate);
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
    fn a_disconnect_counts_from_when_it_was_clicked() {
        // The command started (clicked) but its blocking work hasn't run yet: already nothing
        // is admitted, and a sign-in that was out can't store.
        let gate = Session::new();
        let a = stored(&gate, "tok-a");
        let pat = gate.begin();
        let so = gate.begin_sign_out();
        assert!(a.admit().is_err());
        assert!(gate.bearer(|| Some("tok-a".into())).is_none());
        assert_eq!(gate.commit(pat, ok), None);
        // A sign-in begun while it runs can't land after it ends either.
        let during = gate.begin();
        so.finish(|_| (true, ()));
        assert_eq!(gate.commit(during, ok), None);
        // One begun afterwards (the user connects again) does.
        let after = gate.begin();
        assert_eq!(gate.commit(after, ok), Some(Ok(())));
        assert!(!gate.signed_out());
    }

    #[test]
    fn an_unfinished_sign_out_still_ends_the_session() {
        let gate = Session::new();
        let a = stored(&gate, "tok-a");
        drop(gate.begin_sign_out()); // e.g. its task never ran
        assert!(a.admit().is_err());
        assert!(gate.signed_out());
        let attempt = gate.begin();
        assert!(gate.commit(attempt, ok).is_some(), "not stuck closing");
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
        assert_eq!(gate.commit(attempt, ok), Some(Ok(())));
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
                sign_out(&gate);
                Some("tok-a".to_string())
            })
            .unwrap();
        assert!(b.admit().is_err());
        // A sign-in that commits in between does the same.
        let gate = Session::new();
        let attempt = gate.begin();
        let b = gate
            .bearer(|| {
                gate.commit(attempt, ok).unwrap().unwrap();
                Some("tok-a".to_string())
            })
            .unwrap();
        assert!(b.admit().is_err());
    }

    #[test]
    fn disconnect_waits_for_admitted_requests_and_admits_nothing_meanwhile() {
        // Leaked so the threads can borrow it, as they borrow the app's static session.
        let gate: &'static Session = Box::leak(Box::new(Session::new()));
        let (sent_tx, sent_rx) = mpsc::channel::<()>();
        let (finish_tx, finish_rx) = mpsc::channel::<()>();
        let request = {
            std::thread::spawn(move || {
                let b = stored(gate, "tok");
                let admitted = b.admit().unwrap();
                sent_tx.send(()).unwrap();
                finish_rx.recv().unwrap(); // the request is on the wire
                drop(admitted);
            })
        };
        sent_rx.recv().unwrap();
        let other = stored(gate, "tok");
        let (cleared_tx, cleared_rx) = mpsc::channel::<()>();
        let so = gate.begin_sign_out();
        let logout = {
            std::thread::spawn(move || {
                so.finish(|_| {
                    cleared_tx.send(()).unwrap();
                    (true, ())
                })
            })
        };
        // Disconnect has started: a new request isn't admitted, and the clear waits.
        assert!(other.admit().is_err());
        assert!(gate.bearer(|| Some("tok".into())).is_none());
        assert!(cleared_rx.recv_timeout(Duration::from_millis(100)).is_err());
        finish_tx.send(()).unwrap();
        cleared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        logout.join().unwrap();
        request.join().unwrap();
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
        assert_eq!(gate.commit(a, ok), None, "A can't overwrite B");
        assert_eq!(gate.commit(b, ok), Some(Ok(())));
        // An attempt commits once.
        assert_eq!(gate.commit(b, ok), None);
        // A PAT save started after a device flow replaces it too.
        let flow = gate.begin();
        let pat = gate.begin();
        assert_eq!(gate.commit(flow, ok), None);
        assert!(gate.commit(pat, ok).is_some());
        // And a Disconnect beats every sign-in already out.
        let late = gate.begin();
        sign_out(&gate);
        assert_eq!(gate.commit(late, ok), None);
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
        sign_out(&gate);
        assert!(gate.admit_attempt(c).is_err());
    }

    #[test]
    fn a_failed_store_signs_out_only_when_the_token_may_have_changed() {
        // Connected as A; storing B fails before anything was written: A stays connected.
        let gate = Session::new();
        let attempt = gate.begin();
        assert_eq!(
            gate.commit(attempt, || failed(false)),
            Some(Err("busy".into()))
        );
        assert!(!gate.signed_out());
        assert!(stored(&gate, "tok-a").admit().is_ok());
        // B's token was written but its account name wasn't: signed out, not half-switched.
        let old = stored(&gate, "tok-a");
        let attempt = gate.begin();
        assert!(gate.commit(attempt, || failed(true)).unwrap().is_err());
        assert!(gate.signed_out());
        assert!(old.admit().is_err());
        assert!(gate.bearer(|| Some("tok-b".into())).is_none());
        // A later sign-in that stores cleanly connects again.
        let attempt = gate.begin();
        gate.commit(attempt, ok).unwrap().unwrap();
        assert!(!gate.signed_out());
        assert!(stored(&gate, "tok-c").admit().is_ok());
    }

    #[test]
    fn a_disconnect_that_couldnt_clean_up_holds_after_a_restart() {
        let path = marker_path("restart");
        let gate = Session::new();
        gate.use_marker(path.clone());
        assert!(!gate.signed_out());
        // Storage couldn't be cleared: the marker stays.
        let marked = gate.begin_sign_out().finish(|marked| (false, marked));
        assert!(marked);
        assert!(path.exists());
        // Next run: still signed out, whatever storage returns.
        let next = Session::new();
        next.use_marker(path.clone());
        assert!(next.signed_out());
        assert!(next.bearer(|| Some("survivor".into())).is_none());
        // Connecting again clears it.
        let attempt = next.begin();
        next.commit(attempt, ok).unwrap().unwrap();
        assert!(!path.exists());
        assert!(!next.signed_out());
        // A Disconnect that cleaned up leaves no marker, so the next run starts normally (and
        // simply finds no token).
        next.begin_sign_out().finish(|_| (true, ()));
        assert!(!path.exists());
        let third = Session::new();
        third.use_marker(path.clone());
        assert!(!third.signed_out());
    }

    #[test]
    fn a_disconnect_interrupted_before_it_finished_holds_after_a_restart() {
        // Started (a request still admitted keeps it waiting), then Silo exits.
        let path = marker_path("interrupted");
        let gate = Session::new();
        gate.use_marker(path.clone());
        let b = stored(&gate, "tok");
        let _admitted = b.admit().unwrap();
        let so = gate.begin_sign_out();
        assert!(path.exists(), "marked before waiting");
        std::mem::forget(so); // the process ends here
        let next = Session::new();
        next.use_marker(path.clone());
        assert!(next.signed_out());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_sign_in_marks_before_it_stores() {
        let path = marker_path("before-store");
        let gate = Session::new();
        gate.use_marker(path.clone());
        let attempt = gate.begin();
        let seen = gate.commit(attempt, || {
            // A crash here (token half-written) leaves the marker behind.
            Ok::<_, StoreFailed>(path.exists())
        });
        assert_eq!(seen, Some(Ok(true)));
        assert!(!path.exists(), "removed once everything was stored");
        // No marker can be written: nothing is stored.
        let blocked = std::env::temp_dir()
            .join(format!("silo_gh_no_dir_{}", std::process::id()))
            .join("marker");
        let gate = Session::new();
        gate.use_marker(blocked);
        let attempt = gate.begin();
        let r = gate.commit(attempt, || -> Result<(), StoreFailed> {
            panic!("store must not run")
        });
        assert!(r.unwrap().unwrap_err().starts_with(MARK_FAILED));
    }

    #[test]
    fn a_sign_in_that_cant_remove_an_old_marker_stays_signed_out() {
        // The marker path is a directory, so removing it as a file fails.
        let path = marker_path("is-a-dir");
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let gate = Session::new();
        gate.use_marker(path.clone());
        assert!(gate.signed_out());
        let attempt = gate.begin();
        // Writing fails too (it's a directory): nothing is stored, still signed out.
        assert!(gate.commit(attempt, ok).unwrap().is_err());
        assert!(gate.signed_out());
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn a_half_switched_sign_in_holds_after_a_restart() {
        let path = marker_path("half");
        let gate = Session::new();
        gate.use_marker(path.clone());
        let attempt = gate.begin();
        assert!(gate.commit(attempt, || failed(true)).unwrap().is_err());
        assert!(path.exists());
        let next = Session::new();
        next.use_marker(path.clone());
        assert!(next.signed_out());
        // A failed store that changed nothing leaves things as they were: no marker when there
        // was none, the marker when there was one.
        let attempt = gate.begin();
        gate.commit(attempt, ok).unwrap().unwrap();
        assert!(!path.exists());
        let attempt = gate.begin();
        assert!(gate.commit(attempt, || failed(false)).unwrap().is_err());
        assert!(!path.exists());
        assert!(!gate.signed_out());
        gate.begin_sign_out().finish(|_| (false, ()));
        assert!(path.exists());
        let attempt = gate.begin();
        assert!(gate.commit(attempt, || failed(false)).unwrap().is_err());
        assert!(path.exists());
        assert!(gate.signed_out());
        let _ = std::fs::remove_file(&path);
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
        assert!(gate.commit(b, ok).is_some());
        // A cancel that arrives after the commit says the flow was already gone.
        assert!(!gate.cancel_flow("code-b"));
        // A cancel that wins: the poll can't commit afterwards.
        let c = gate.begin();
        gate.remember_flow("code-c", c, 900).unwrap();
        assert!(gate.cancel_flow("code-c"));
        assert_eq!(gate.commit(c, ok), None);
        assert_eq!(gate.flow("code-c"), FlowState::Gone);
    }

    #[test]
    fn a_disconnect_forgets_the_flow() {
        let gate = Session::new();
        let a = gate.begin();
        gate.remember_flow("code", a, 900).unwrap();
        sign_out(&gate);
        assert_eq!(gate.flow("code"), FlowState::Gone);
        // A start issued before the Disconnect answers after it: refused.
        assert!(gate.remember_flow("code", a, 900).is_err());
    }
}
