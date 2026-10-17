//! Folders the user chose in a folder picker Silo opened itself (`pick_folder`). The commands
//! that change where Silo writes, the mods folder and the library archive, accept a new folder
//! only from here (or one Silo already knows), so a compromised webview can't quietly point
//! them somewhere else.

use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    ModsFolder,
    Archive,
}

/// Recent picks per purpose that stay valid (pick, cancel a confirm, pick again…).
pub(crate) const KEEP: usize = 4;

pub const NOT_PICKED: &str = "Choose that folder with the Change… button, then try again.";

/// Each pick is kept as the folder's identity key (`modsroot::canonical_key`) taken when it was
/// picked, so a picked path later swapped for a link (or junction) to somewhere else no longer
/// matches.
#[derive(Default)]
pub struct Picks(Vec<(Purpose, String)>);

impl Picks {
    pub fn remember(&mut self, purpose: Purpose, path: &Path) {
        let key = crate::modsroot::canonical_key(path);
        self.0.retain(|(p, k)| !(*p == purpose && *k == key));
        self.0.push((purpose, key));
        let mut seen = 0;
        let mut i = self.0.len();
        while i > 0 {
            i -= 1;
            if self.0[i].0 == purpose {
                seen += 1;
                if seen > KEEP {
                    self.0.remove(i);
                }
            }
        }
    }

    pub fn allows(&self, purpose: Purpose, path: &Path) -> bool {
        let key = crate::modsroot::canonical_key(path);
        self.0.iter().any(|(p, k)| *p == purpose && *k == key)
    }

    /// Drop every pick for `purpose`: called once a command has used one, so it can't be
    /// replayed later.
    pub fn forget(&mut self, purpose: Purpose) {
        self.0.retain(|(p, _)| *p != purpose);
    }
}

static PICKS: Mutex<Picks> = Mutex::new(Picks(Vec::new()));

pub fn remember(purpose: Purpose, path: &Path) {
    PICKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remember(purpose, path);
}

pub fn allows(purpose: Purpose, path: &Path) -> bool {
    PICKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .allows(purpose, path)
}

/// See [`Picks::forget`]. Call only after the command that used the pick succeeded, so a
/// refused or cancelled attempt (e.g. "use it as it is" after a move is refused) can retry.
pub fn forget(purpose: Purpose) {
    PICKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .forget(purpose);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(tag: &str, n: usize) -> (std::path::PathBuf, Vec<std::path::PathBuf>) {
        let base = std::env::temp_dir().join(format!("silo_picks_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let v: Vec<_> = (0..n).map(|i| base.join(format!("d{i}"))).collect();
        for d in &v {
            std::fs::create_dir_all(d).unwrap();
        }
        (base, v)
    }

    #[test]
    fn only_a_folder_picked_for_that_purpose_is_allowed() {
        let (base, d) = dirs("purpose", 2);
        let mut p = Picks::default();
        p.remember(Purpose::Archive, &d[0]);
        assert!(p.allows(Purpose::Archive, &d[0]));
        assert!(
            !p.allows(Purpose::ModsFolder, &d[0]),
            "picked for the archive only"
        );
        assert!(!p.allows(Purpose::Archive, &d[1]), "never picked");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_the_last_few_picks_stay_valid() {
        let (base, d) = dirs("last", KEEP + 1);
        let mut p = Picks::default();
        for x in &d {
            p.remember(Purpose::ModsFolder, x);
        }
        assert!(!p.allows(Purpose::ModsFolder, &d[0]), "oldest pick dropped");
        assert!(d[1..].iter().all(|x| p.allows(Purpose::ModsFolder, x)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_pick_names_the_folder_it_was_not_a_link_put_there_later() {
        let (base, d) = dirs("swap", 2);
        let mut p = Picks::default();
        p.remember(Purpose::Archive, &d[0]);
        // The picked folder is swapped for a link (a junction on Windows) to another folder.
        std::fs::remove_dir(&d[0]).unwrap();
        crate::organize::link_dir_native(&d[1], &d[0]).unwrap();
        assert!(
            !p.allows(Purpose::Archive, &d[0]),
            "the path now leads elsewhere"
        );
        assert!(
            !p.allows(Purpose::Archive, &d[1]),
            "the link's target was never picked"
        );
        let _ = std::fs::remove_dir(&d[0]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_used_pick_can_not_be_replayed() {
        let (base, d) = dirs("used", 3);
        let mut p = Picks::default();
        p.remember(Purpose::Archive, &d[0]);
        p.remember(Purpose::Archive, &d[1]);
        p.remember(Purpose::ModsFolder, &d[2]);
        p.forget(Purpose::Archive);
        assert!(!p.allows(Purpose::Archive, &d[0]));
        assert!(!p.allows(Purpose::Archive, &d[1]));
        assert!(
            p.allows(Purpose::ModsFolder, &d[2]),
            "other purposes keep their picks"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
