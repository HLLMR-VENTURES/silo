//! Folders the user chose in a folder picker Silo opened itself (`pick_folder`). The commands
//! that change where Silo writes, the mods folder and the library archive, accept a new folder
//! only from here (or one Silo already knows), so a compromised webview can't quietly point
//! them somewhere else.

use std::path::{Path, PathBuf};
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

#[derive(Default)]
pub struct Picks(Vec<(Purpose, PathBuf)>);

impl Picks {
    pub fn remember(&mut self, purpose: Purpose, path: &Path) {
        self.0
            .retain(|(p, f)| !(*p == purpose && crate::modsroot::same_folder(f, path)));
        self.0.push((purpose, path.to_path_buf()));
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
        self.0
            .iter()
            .any(|(p, f)| *p == purpose && crate::modsroot::same_folder(f, path))
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
}
