// Native suspension and process sampling are currently implemented for Windows only.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Manager, Webview};

#[cfg(windows)]
mod native;

pub const SCRIPT: &str = include_str!("memory.js");
pub const DEFAULT_BUDGET_MB: u64 = 1024;
const IDLE: Duration = Duration::from_secs(30);
const SETTLE: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Media {
    time: f64,
    volume: f64,
    muted: bool,
    rate: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub url: String,
    protected: bool,
    x: f64,
    y: f64,
    media: Vec<Media>,
}

impl Snapshot {
    fn valid(&self) -> bool {
        self.url.len() <= 8192
            && url::Url::parse(&self.url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            && self.x.is_finite()
            && self.y.is_finite()
            && self.media.len() <= 16
            && self.media.iter().all(|m| {
                m.time.is_finite()
                    && m.time >= 0.0
                    && (0.0..=1.0).contains(&m.volume)
                    && m.rate.is_finite()
                    && m.rate > 0.0
                    && m.rate <= 16.0
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Suspend,
    Discard,
}

#[derive(Clone, Debug)]
enum Phase {
    Awake,
    Checking {
        token: u64,
        action: Action,
        since: Instant,
    },
    Suspending {
        token: u64,
        since: Instant,
    },
    Suspended {
        since: Instant,
    },
    Discarded,
    Restoring,
}

struct Entry {
    last_active: Instant,
    last_check: Instant,
    phase: Phase,
    snapshot: Option<Snapshot>,
}

#[derive(Default)]
struct Controller {
    active: String,
    entries: HashMap<String, Entry>,
    token: u64,
    last_discard: Option<Instant>,
}

pub struct MemoryManager {
    pub budget_mb: AtomicU64,
    used_bytes: AtomicU64,
    controller: Mutex<Controller>,
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self {
            budget_mb: AtomicU64::new(DEFAULT_BUDGET_MB),
            used_bytes: AtomicU64::new(0),
            controller: Mutex::new(Controller::default()),
        }
    }
}

#[derive(serde::Serialize)]
pub struct Status {
    supported: bool,
    used_mb: Option<u64>,
    budget_mb: u64,
    suspended: usize,
    discarded: usize,
    over_budget: bool,
}

#[tauri::command]
pub fn memory_status(state: tauri::State<MemoryManager>) -> Status {
    let controller = state.controller.lock().unwrap();
    let bytes = state.used_bytes.load(Ordering::Relaxed);
    let budget_mb = state.budget_mb.load(Ordering::Relaxed);
    Status {
        supported: cfg!(windows),
        used_mb: (bytes > 0).then_some(bytes / 1024 / 1024),
        budget_mb,
        suspended: controller
            .entries
            .values()
            .filter(|e| matches!(e.phase, Phase::Suspended { .. }))
            .count(),
        discarded: controller
            .entries
            .values()
            .filter(|e| matches!(e.phase, Phase::Discarded))
            .count(),
        over_budget: bytes > budget_mb * 1024 * 1024,
    }
}

#[tauri::command]
pub fn memory_restore(webview: Webview, state: tauri::State<MemoryManager>) -> Option<Snapshot> {
    let controller = state.controller.lock().unwrap();
    let entry = controller.entries.get(webview.label())?;
    let snapshot = entry.snapshot.as_ref()?;
    (matches!(entry.phase, Phase::Restoring) && webview.url().ok()?.as_str() == snapshot.url)
        .then(|| snapshot.clone())
}

pub fn activate(webview: &Webview) -> Result<(), String> {
    let state = webview.state::<MemoryManager>();
    let previous_active = state.controller.lock().unwrap().active.clone();
    let snapshot = {
        let mut controller = state.controller.lock().unwrap();
        let previous = controller.active.clone();
        if previous != webview.label() {
            if let Some(entry) = controller.entries.get_mut(&previous) {
                entry.last_active = Instant::now();
            }
        }
        controller.active = webview.label().into();
        let now = Instant::now();
        let entry = controller
            .entries
            .entry(webview.label().into())
            .or_insert(Entry {
                last_active: now,
                last_check: now,
                phase: Phase::Awake,
                snapshot: None,
            });
        entry.last_active = now;
        if matches!(entry.phase, Phase::Discarded) {
            entry.phase = Phase::Restoring;
            entry.snapshot.clone()
        } else {
            if !matches!(entry.phase, Phase::Restoring) {
                entry.phase = Phase::Awake;
            }
            None
        }
    };
    #[cfg(windows)]
    native::resume(webview);
    if let Some(snapshot) = snapshot {
        if let Err(error) = webview.eval(format!(
            "location.replace({})",
            serde_json::to_string(&snapshot.url).unwrap()
        )) {
            let mut controller = state.controller.lock().unwrap();
            controller.active = previous_active;
            if let Some(entry) = controller.entries.get_mut(webview.label()) {
                entry.phase = Phase::Discarded;
            }
            return Err(error.to_string());
        }
    }
    Ok(())
}

pub fn page_load(webview: &Webview, event: tauri::webview::PageLoadEvent, url: &url::Url) {
    let state = webview.state::<MemoryManager>();
    let mut controller = state.controller.lock().unwrap();
    let Some(entry) = controller.entries.get_mut(webview.label()) else {
        return;
    };
    if matches!(entry.phase, Phase::Discarded) {
        return;
    }
    if matches!(entry.phase, Phase::Restoring) {
        if let Some(snapshot) = &entry.snapshot {
            if snapshot.url == url.as_str() {
                if matches!(event, tauri::webview::PageLoadEvent::Finished) {
                    let script = format!(
                        "window.__minibrowserMemoryRestore?.({})",
                        serde_json::to_string(snapshot).unwrap()
                    );
                    entry.phase = Phase::Awake;
                    entry.snapshot = None;
                    drop(controller);
                    let _ = webview.eval(&script);
                }
                return;
            }
        }
    }
    entry.phase = Phase::Awake;
    entry.snapshot = None;
    entry.last_check = Instant::now();
}

fn candidate(controller: &Controller, now: Instant, pressure: bool) -> Option<(String, Action)> {
    controller
        .entries
        .iter()
        .filter(|(label, entry)| {
            *label != &controller.active
                && now.duration_since(entry.last_active) >= IDLE
                && now.duration_since(entry.last_check) >= SETTLE
        })
        .filter_map(|(label, entry)| match entry.phase {
            Phase::Awake => Some((label.clone(), Action::Suspend, entry.last_active)),
            Phase::Suspended { since } if pressure && now.duration_since(since) >= SETTLE => {
                Some((label.clone(), Action::Discard, entry.last_active))
            }
            _ => None,
        })
        .min_by_key(|(_, _, last_active)| *last_active)
        .map(|(label, action, _)| (label, action))
}

fn tick(app: &tauri::AppHandle) {
    if app
        .state::<crate::Workspaces>()
        .creating
        .load(Ordering::Acquire)
    {
        return;
    }
    let state = app.state::<MemoryManager>();
    let now = Instant::now();
    let mut controller = state.controller.lock().unwrap();
    // A page may navigate or stop responding before its IPC reply arrives.
    for entry in controller.entries.values_mut() {
        if matches!(entry.phase, Phase::Checking { since, .. } | Phase::Suspending { since, .. }
            if now.duration_since(since) > SETTLE)
        {
            entry.phase = Phase::Awake;
        }
    }
    let settling = controller
        .last_discard
        .is_some_and(|time| now.duration_since(time) < SETTLE);
    // Keep suspending other idle pages while waiting to measure a discard's effect.
    let pressure = !settling
        && state.used_bytes.load(Ordering::Relaxed)
            > state.budget_mb.load(Ordering::Relaxed) * 1024 * 1024 * 9 / 10;
    let Some((label, action)) = candidate(&controller, now, pressure) else {
        return;
    };
    controller.token += 1;
    let token = controller.token;
    let entry = controller.entries.get_mut(&label).unwrap();
    entry.phase = Phase::Checking {
        token,
        action,
        since: now,
    };
    entry.last_check = now;
    drop(controller);
    if let Some(webview) = app.get_webview(&label) {
        #[cfg(windows)]
        native::resume_hidden(&webview);
        let _ = webview.eval(format!("window.__minibrowserMemoryCheck?.({token})"));
    }
}

#[tauri::command]
pub fn memory_report(webview: Webview, token: u64, snapshot: Snapshot) {
    if !snapshot.valid() || webview.url().ok().as_ref().map(|u| u.as_str()) != Some(&snapshot.url) {
        return;
    }
    let app = webview.app_handle().clone();
    // Serialize decisions with workspace switching and navigation on the UI thread.
    let _ = app.run_on_main_thread(move || {
        let state = webview.state::<MemoryManager>();
        let mut controller = state.controller.lock().unwrap();
        if controller.active == webview.label() {
            return;
        }
        let Some(entry) = controller.entries.get_mut(webview.label()) else {
            return;
        };
        let Phase::Checking {
            token: expected,
            action,
            ..
        } = entry.phase
        else {
            return;
        };
        if token != expected {
            return;
        }
        if snapshot.protected {
            entry.phase = Phase::Awake;
            drop(controller);
            #[cfg(windows)]
            native::resume(&webview);
            return;
        }
        entry.snapshot = Some(snapshot);
        entry.phase = Phase::Suspending {
            token,
            since: Instant::now(),
        };
        drop(controller);
        #[cfg(windows)]
        native::suspend(&webview, token, action);
    });
}

#[cfg(windows)]
fn is_pending(webview: &Webview, token: u64) -> bool {
    let state = webview.state::<MemoryManager>();
    let controller = state.controller.lock().unwrap();
    controller.active != webview.label() && controller.entries.get(webview.label())
        .is_some_and(|entry| matches!(entry.phase, Phase::Suspending { token: expected, .. } if token == expected))
}

#[cfg(windows)]
fn suspended(webview: &Webview, token: u64, action: Action, success: bool) {
    let state = webview.state::<MemoryManager>();
    let mut controller = state.controller.lock().unwrap();
    let is_active = controller.active == webview.label();
    let Some(entry) = controller.entries.get_mut(webview.label()) else {
        return;
    };
    if is_active
        || !matches!(entry.phase, Phase::Suspending { token: expected, .. } if token == expected)
    {
        drop(controller);
        if is_active {
            native::resume(webview);
        }
        return;
    }
    if !success {
        entry.phase = Phase::Awake;
        drop(controller);
        native::resume(webview);
        return;
    }
    let pressure = state.used_bytes.load(Ordering::Relaxed)
        > state.budget_mb.load(Ordering::Relaxed) * 1024 * 1024 * 9 / 10;
    if action == Action::Discard && pressure {
        entry.phase = Phase::Discarded;
        controller.last_discard = Some(Instant::now());
        drop(controller);
        let mut url = crate::build_home_url();
        url.set_path("sleeping.html");
        native::resume_hidden(webview);
        if webview
            .eval(format!(
                "location.replace({})",
                serde_json::to_string(url.as_str()).unwrap()
            ))
            .is_err()
        {
            if let Some(entry) = state
                .controller
                .lock()
                .unwrap()
                .entries
                .get_mut(webview.label())
            {
                entry.phase = Phase::Awake;
            }
            native::resume(webview);
        }
    } else {
        entry.phase = Phase::Suspended {
            since: Instant::now(),
        };
    }
}

pub fn start(app: &tauri::AppHandle) {
    if !cfg!(windows) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        if app.webviews().is_empty() {
            break;
        }
        #[cfg(windows)]
        if let Some(bytes) = native::process_tree_memory() {
            app.state::<MemoryManager>()
                .used_bytes
                .store(bytes, Ordering::Relaxed);
        } else {
            // Unknown usage must never trigger destructive memory pressure decisions.
            app.state::<MemoryManager>()
                .used_bytes
                .store(0, Ordering::Relaxed);
        }
        let handle = app.clone();
        if app.run_on_main_thread(move || tick(&handle)).is_err() {
            break;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(now: Instant, seconds: u64, phase: Phase) -> Entry {
        Entry {
            last_active: now - Duration::from_secs(seconds),
            last_check: now - SETTLE,
            phase,
            snapshot: None,
        }
    }
    #[test]
    fn protects_active_and_recent_workspaces_and_chooses_oldest() {
        let now = Instant::now();
        let mut c = Controller {
            active: "active".into(),
            ..Default::default()
        };
        c.entries
            .insert("active".into(), entry(now, 300, Phase::Awake));
        c.entries
            .insert("recent".into(), entry(now, 5, Phase::Awake));
        c.entries
            .insert("old".into(), entry(now, 100, Phase::Awake));
        c.entries
            .insert("older".into(), entry(now, 200, Phase::Awake));
        assert_eq!(
            candidate(&c, now, true),
            Some(("older".into(), Action::Suspend))
        );
        c.entries.remove("old");
        c.entries.remove("older");
        assert_eq!(candidate(&c, now, true), None);
    }
    #[test]
    fn discards_only_under_pressure_after_suspension_settles() {
        let now = Instant::now();
        let mut c = Controller::default();
        c.entries.insert(
            "ws".into(),
            entry(now, 100, Phase::Suspended { since: now }),
        );
        assert_eq!(candidate(&c, now, true), None);
        assert_eq!(candidate(&c, now + SETTLE, false), None);
        assert_eq!(
            candidate(&c, now + SETTLE, true),
            Some(("ws".into(), Action::Discard))
        );
        c.entries.get_mut("ws").unwrap().phase = Phase::Discarded;
        assert_eq!(candidate(&c, now + SETTLE, true), None);
    }
    #[test]
    fn rejects_invalid_restore_data() {
        let mut snapshot = Snapshot {
            url: "https://www.youtube.com/watch?v=test".into(),
            protected: false,
            x: 0.0,
            y: 100.0,
            media: vec![],
        };
        assert!(snapshot.valid());
        snapshot.url = "javascript:alert(1)".into();
        assert!(!snapshot.valid());
        snapshot.url = "https://example.com".into();
        snapshot.media.push(Media {
            time: f64::NAN,
            volume: 1.0,
            muted: false,
            rate: 1.0,
        });
        assert!(!snapshot.valid());
    }
}
