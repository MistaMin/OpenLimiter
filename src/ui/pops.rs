//! Pop-out windows: the history or the radar in a window of their own. They belong to the
//! plug-in instance (its `Shared`), not to the editor, so they stay open when the plug-in
//! window closes; the instance closes them when it goes away ([`close_all`]). Where each
//! window was is kept in the preferences and used the next time it opens.

use std::sync::{Arc, Mutex, MutexGuard};

use openplugin::core::Editor;

use super::detached::{Detached, Kind};
use super::theme::pref_file;
use crate::shared::Shared;

#[cfg(target_os = "macos")]
use super::popout::Window;
#[cfg(target_os = "windows")]
use super::popwin::Window;
#[cfg(target_os = "linux")]
use super::popx11::Window;

/// What a platform window gets to build its editor from (Linux builds it on its own thread).
pub type Make = Box<dyn FnOnce() -> Box<dyn Editor> + Send>;

pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux"));

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
struct Pop {
    owner: usize,
    kind: Kind,
    win: Window,
}

// macOS and Windows windows are only touched on the main thread, where the editor and the
// instance live; Linux windows run on their own threads behind atomics.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
unsafe impl Send for Pop {}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl Drop for Pop {
    fn drop(&mut self) {
        if let Some(frame) = self.win.frame() {
            save(self.kind, frame);
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
static POPS: Mutex<Vec<Pop>> = Mutex::new(Vec::new());

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn pops() -> MutexGuard<'static, Vec<Pop>> {
    POPS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Takes out the windows matching `gone`; the caller drops them after the lock is released.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn take(gone: impl Fn(&Pop) -> bool) -> Vec<Pop> {
    let mut all = pops();
    let (out, keep) = std::mem::take(&mut *all).into_iter().partition(|p| gone(p));
    *all = keep;
    out
}

fn owner(shared: &Shared) -> usize {
    shared as *const Shared as usize
}

/// Whether `kind` is open for this instance. Forgets windows the user closed.
pub fn is_open(shared: &Shared, kind: Kind) -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        drop(take(|p| !p.win.is_open()));
        pops().iter().any(|p| p.owner == owner(shared) && p.kind == kind)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (shared, kind);
        false
    }
}

/// Opens `kind`, or closes it when it is open.
pub fn toggle(shared: &Arc<Shared>, kind: Kind) {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        if is_open(shared, kind) {
            drop(take(|p| p.owner == owner(shared) && p.kind == kind));
            return;
        }
        let made = shared.clone();
        let make: Make = Box::new(move || Box::new(Detached::new(kind, made)));
        if let Some(win) = Window::open(make, Detached::title(kind), load(kind)) {
            pops().push(Pop { owner: owner(shared), kind, win });
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = (shared, kind);
}

/// The instance is going away: close its windows before its code can be unloaded.
pub fn close_all(shared: &Shared) {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    drop(take(|p| p.owner == owner(shared)));
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = shared;
}

fn key(kind: Kind) -> &'static str {
    match kind {
        Kind::History => "history",
        Kind::Radar => "radar",
    }
}

/// `x y w h` of the window last time, in the platform's own screen units.
fn load(kind: Kind) -> Option<[i32; 4]> {
    let text = std::fs::read_to_string(pref_file("pops")?).ok()?;
    parse(&text, kind)
}

fn parse(text: &str, kind: Kind) -> Option<[i32; 4]> {
    let line = text.lines().find(|l| l.split_whitespace().next() == Some(key(kind)))?;
    let v: Vec<i32> = line.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    (v.len() == 4 && v[2] > 0 && v[3] > 0).then(|| [v[0], v[1], v[2], v[3]])
}

fn save(kind: Kind, f: [i32; 4]) {
    if cfg!(test) {
        return;
    }
    let Some(path) = pref_file("pops") else { return };
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut text: String = old.lines().filter(|l| l.split_whitespace().next() != Some(key(kind))).map(|l| format!("{l}\n")).collect();
    text.push_str(&format!("{} {} {} {} {}\n", key(kind), f[0], f[1], f[2], f[3]));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_read_back_per_window() {
        let text = "radar 10 -20 500 560\nhistory 1 2 0 5\n";
        assert_eq!(parse(text, Kind::Radar), Some([10, -20, 500, 560]));
        // A zero size is no frame.
        assert_eq!(parse(text, Kind::History), None);
        assert_eq!(parse("", Kind::Radar), None);
    }
}
