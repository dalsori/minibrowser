#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::http::{Request, Response};
use tauri::webview::PageLoadEvent;
use tauri::WindowEvent;
use tauri::{Manager, Position, Size, State, Webview, WebviewBuilder, WebviewUrl, WindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

struct AppState {
    home_url: Mutex<url::Url>,
    adblock: Arc<AtomicBool>,
    engine: Mutex<String>,
}

struct Workspaces {
    labels: Mutex<Vec<String>>,
    active: Mutex<usize>,
    creating: AtomicBool,
}

// Mantiene audio/vídeo en segundo plano sin desactivar el throttling general de pestañas.
const BROWSER_ARGS: &str = "--disable-background-media-suspend";
const PARKED_X: i32 = 100_000;
const MAX_WORKSPACES: usize = 4;

#[derive(serde::Serialize)]
struct StatePayload {
    adblock: bool,
    engine: String,
}

#[derive(serde::Serialize)]
struct WorkspacePayload {
    active: usize,
    total: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DiskSettings {
    adblock: bool,
    engine: String,
}

const ADBLOCK: &[&str] = &[
    "2mdn.net",
    "adcolony.com",
    "adform.net",
    "admarvel.com",
    "adnxs.com",
    "adsafeprotected.com",
    "adservice.google.com",
    "adsrvr.org",
    "adsterra.com",
    "adsymptotic.com",
    "adskeeper.com",
    "adjust.com",
    "admob.com",
    "adroll.com",
    "adobedtm.com",
    "amplitude.com",
    "appsflyer.com",
    "appnexus.com",
    "atdmt.com",
    "bidswitch.net",
    "bluekai.com",
    "branch.io",
    "burstnet.com",
    "c1exchange.com",
    "casalemedia.com",
    "chartbeat.com",
    "chartboost.com",
    "clickbank.net",
    "clicktale.net",
    "clarity.ms",
    "comscore.com",
    "contextweb.com",
    "crazyegg.com",
    "criteo.com",
    "criteo.net",
    "demdex.net",
    "doubleclick.net",
    "everesttech.net",
    "exelator.com",
    "eyeota.net",
    "facebook.net",
    "fullstory.com",
    "fyber.com",
    "googleadservices.com",
    "google-analytics.com",
    "googlesyndication.com",
    "googletagmanager.com",
    "googletagservices.com",
    "gumgum.com",
    "heap.io",
    "hilltopads.net",
    "hotjar.com",
    "improvedigital.com",
    "imrworldwide.com",
    "indexww.com",
    "inmobi.com",
    "inspectlet.com",
    "ironsrc.com",
    "juicyads.com",
    "kochava.com",
    "krxd.net",
    "lijit.com",
    "loopme.me",
    "luckyorange.com",
    "mathtag.com",
    "media.net",
    "mgid.com",
    "mintegral.com",
    "mixpanel.com",
    "moatads.com",
    "mopub.com",
    "mouseflow.com",
    "mparticle.com",
    "nativo.com",
    "omtrdc.net",
    "onetag.com",
    "openx.net",
    "optimizely.com",
    "outbrain.com",
    "parsely.com",
    "plista.com",
    "popads.net",
    "propellerads.com",
    "pubmatic.com",
    "quantserve.com",
    "rfihub.com",
    "rhythmone.com",
    "rlcdn.com",
    "rubiconproject.com",
    "scorecardresearch.com",
    "segment.com",
    "sharethrough.com",
    "smartadserver.com",
    "smaato.net",
    "sovrn.com",
    "spotxchange.com",
    "taboola.com",
    "tapad.com",
    "tapjoy.com",
    "teads.tv",
    "trafficrooster.com",
    "tremorhub.com",
    "tribalfusion.com",
    "triplelift.com",
    "undertone.com",
    "unityads.unity3d.com",
    "verve.com",
    "vungle.com",
    "vwo.com",
    "yieldmo.com",
    "zedo.com",
];

// Patrones de URL de anuncios (sobre todo YouTube, no cubiertos por dominio).
const ADBLOCK_URL: &[&str] = &[
    "youtube.com/api/stats/ads",
    "youtube.com/pagead/",
    "/youtubei/v1/player/get_ad_break",
    "/get_midroll_info",
    "youtube.com/api/timedtext?type=track",
    "adservice.google.com",
    "googleads.g.doubleclick.net",
    "pagead2.googlesyndication.com",
    "securepubads.g.doubleclick.net",
    "static.doubleclick.net",
    "ad.doubleclick.net",
    "g.doubleclick.net",
    "google.com/aclk",
    "google.com/pagead/",
    "/pagead/",
];

fn blocked_hosts() -> HashSet<String> {
    ADBLOCK.iter().map(|s| s.to_string()).collect()
}

fn is_blocked(host: &str, blocked: &HashSet<String>) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    for b in blocked {
        if host == *b || host.ends_with(&format!(".{}", b)) {
            return true;
        }
    }
    false
}

fn url_blocked(uri: &str) -> bool {
    let s = uri;
    for p in ADBLOCK_URL {
        if s.contains(p) {
            return true;
        }
    }
    // anuncios de video de YouTube: videoplayback de googlevideo con parametro oad
    if let Some(pos) = s.find("://") {
        let rest = &s[pos + 3..];
        let host = rest.split('/').next().unwrap_or("");
        if host.ends_with("googlevideo.com") && s.contains("oad=") {
            return true;
        }
    }
    false
}

// Atajos globales inyectados en TODA página (remota incluida).
const INIT_JS: &str = r#"
(() => {
  const selectors = [
    '[data-text-ad]', '.uEierd',
    '.video-ads', '.ytp-ad-module', 'ytd-ad-slot-renderer',
    'ytd-promoted-sparkles-web-renderer', 'ytd-display-ad-renderer',
    'ytd-in-feed-ad-layout-renderer', 'ytd-banner-promo-renderer',
    'ytd-action-companion-ad-renderer', 'ytd-promoted-video-renderer'
  ];
  let scheduled = false;
  const clean = () => {
    scheduled = false;
    document.querySelectorAll(selectors.join(',')).forEach((node) => node.remove());
    document.querySelectorAll('.ytp-ad-skip-button, .ytp-skip-ad-button, .ytp-ad-skip-button-modern').forEach((button) => button.click());
    const player = document.querySelector('.html5-video-player.ad-showing');
    const video = player?.querySelector('video');
    if (video && Number.isFinite(video.duration)) video.currentTime = Math.max(0, video.duration - 0.1);
  };
  const scheduleClean = () => {
    if (!scheduled) { scheduled = true; requestAnimationFrame(clean); }
  };
  const start = () => {
    clean();
    new MutationObserver(scheduleClean).observe(document.documentElement, { childList: true, subtree: true });
  };
  window.__TAURI__?.core.invoke('getstate').then((state) => { if (state.adblock) start(); }).catch(() => {});
})();

setTimeout(() => {
  window.__TAURI__?.core.invoke('getworkspaces').then(({ active, total }) => {
    let badge = document.querySelector('#minibrowser-workspace-badge');
    if (!badge) {
      badge = document.createElement('div');
      badge.id = 'minibrowser-workspace-badge';
      Object.assign(badge.style, {
        position: 'fixed', left: '14px', bottom: '14px', zIndex: '2147483647',
        padding: '6px 9px', borderRadius: '8px', pointerEvents: 'none',
        background: '#15171dee', color: '#c8ced8', border: '1px solid #3d4552',
        font: '11px/1.2 system-ui, sans-serif', boxShadow: '0 8px 24px #0008'
      });
      document.documentElement.appendChild(badge);
    }
    badge.textContent = `Espacio ${active} / ${total}`;
  }).catch(() => {});
}, 250);

document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  const isLocal = location.hostname === 'tauri.localhost';
  const showError = (error) => {
    let notice = document.querySelector('#minibrowser-error');
    if (!notice) {
      notice = document.createElement('div');
      notice.id = 'minibrowser-error';
      Object.assign(notice.style, {
        position: 'fixed', right: '16px', bottom: '16px', zIndex: '2147483647',
        maxWidth: '360px', padding: '12px 14px', borderRadius: '10px',
        background: '#2b171a', color: '#ffb4b4', border: '1px solid #71343b',
        font: '13px/1.4 system-ui, sans-serif', boxShadow: '0 12px 32px #0008'
      });
      document.documentElement.appendChild(notice);
    }
    notice.textContent = `Minibrowser: ${String(error)}`;
    clearTimeout(window.__minibrowserErrorTimer);
    window.__minibrowserErrorTimer = setTimeout(() => notice.remove(), 5000);
  };
  const go = (cmd, args) => {
    e.preventDefault();
    try { window.__TAURI__.core.invoke(cmd, args).catch(showError); } catch (error) { showError(error); }
  };

  // workspaces (tmux-like)
  if (mod && key === 'arrowleft') { go('ws', { dir: -1 }); return; }
  if (mod && key === 'arrowright') { go('ws', { dir: 1 }); return; }
  if (mod && key === 't') { go('wsnew'); return; }

  if (isLocal) {
    // páginas locales: Ctrl+K/Ctrl+L enfocan la búsqueda
    if (mod && (key === 'k' || key === 'l')) {
      e.preventDefault();
      const el = document.querySelector('#search-input') || document.querySelector('input');
      if (el) el.focus();
    } else if (mod && key === 'e') go('settings');
    else if (mod && key === 'r') go('reload');
    else if (mod && key === '[') go('back');
    else if (mod && key === ']') go('forward');
    return;
  }
  // páginas remotas
  if (mod && (key === 'k' || key === 'l')) go('home');
  else if (mod && key === 'e') go('settings');
  else if (mod && key === 'r') go('reload');
  else if (mod && key === '[') go('back');
  else if (mod && key === ']') go('forward');
  else if (e.key === 'Escape') go('home');
});
"#;

fn resolve_query(q: &str, engine: &str) -> String {
    let q = q.trim();
    if q.is_empty() {
        return match engine {
            "google" => "https://www.google.com/".into(),
            "bing" => "https://www.bing.com/".into(),
            _ => "https://html.duckduckgo.com/html/".into(),
        };
    }
    let lower = q.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return q.to_string();
    }
    let looks_like_url = !q.contains(' ')
        && (q.contains('.')
            || q.contains(':')
            || q.contains('/')
            || q.eq_ignore_ascii_case("localhost"));
    if looks_like_url {
        let scheme = if q.eq_ignore_ascii_case("localhost")
            || lower.starts_with("localhost:")
            || lower.starts_with("127.0.0.1")
            || lower.starts_with("[::1]")
        {
            "http"
        } else {
            "https"
        };
        return format!("{}://{}", scheme, q);
    }
    let enc: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
    match engine {
        "google" => format!("https://www.google.com/search?q={}", enc),
        "bing" => format!("https://www.bing.com/search?q={}", enc),
        _ => format!("https://html.duckduckgo.com/html/?q={}", enc),
    }
}

fn build_home_url() -> url::Url {
    if cfg!(windows) {
        url::Url::parse("http://tauri.localhost").unwrap()
    } else {
        url::Url::parse("tauri://localhost").unwrap()
    }
}

fn make_webview(
    label: &str,
    blocked: Arc<HashSet<String>>,
    adblock: Arc<AtomicBool>,
) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(label.to_string(), WebviewUrl::App("index.html".into()))
        .initialization_script(INIT_JS)
        .additional_browser_args(BROWSER_ARGS)
        .auto_resize()
        .on_web_resource_request(
            move |request: Request<Vec<u8>>, response: &mut Response<Cow<'static, [u8]>>| {
                if adblock.load(Ordering::Relaxed) {
                    let uri = request.uri().to_string();
                    let blocked_url = url_blocked(&uri);
                    let blocked_host = request
                        .uri()
                        .host()
                        .map(|h| is_blocked(h, &blocked))
                        .unwrap_or(false);
                    if blocked_host || blocked_url {
                        *response = Response::builder()
                            .status(204)
                            .body::<Cow<'static, [u8]>>(Cow::Borrowed(&[]))
                            .unwrap();
                    }
                }
            },
        )
}

fn show_workspace_badge(webview: &Webview, index: usize, total: usize) {
    let script = format!(
        r#"(() => {{
          let badge = document.querySelector('#minibrowser-workspace-badge');
          if (!badge) {{
            badge = document.createElement('div');
            badge.id = 'minibrowser-workspace-badge';
            Object.assign(badge.style, {{
              position: 'fixed', left: '14px', bottom: '14px', zIndex: '2147483647',
              padding: '6px 9px', borderRadius: '8px', pointerEvents: 'none',
              background: '#15171dee', color: '#c8ced8', border: '1px solid #3d4552',
              font: '11px/1.2 system-ui, sans-serif', boxShadow: '0 8px 24px #0008'
            }});
            document.documentElement.appendChild(badge);
          }}
          badge.textContent = 'Espacio {} / {}';
        }})()"#,
        index + 1,
        total
    );
    let _ = webview.eval(&script);
}

#[tauri::command]
fn open(state: State<AppState>, query: String) -> String {
    let engine = state.engine.lock().unwrap().clone();
    resolve_query(&query, &engine)
}

#[tauri::command]
fn home(webview: Webview, state: State<AppState>) -> Result<(), String> {
    let url = state.home_url.lock().unwrap().clone();
    webview.navigate(url).map_err(|error| error.to_string())
}

#[tauri::command]
fn settings(webview: Webview, state: State<AppState>) -> Result<(), String> {
    let mut url = state.home_url.lock().unwrap().clone();
    url.set_path("settings.html");
    webview.navigate(url).map_err(|error| error.to_string())
}

#[tauri::command]
fn reload(webview: Webview) {
    let _ = webview.eval("window.location.reload()");
}

#[tauri::command]
fn back(webview: Webview) {
    let _ = webview.eval("window.history.back()");
}

#[tauri::command]
fn forward(webview: Webview) {
    let _ = webview.eval("window.history.forward()");
}

#[tauri::command]
fn ws(app: tauri::AppHandle, webview: Webview, state: State<Workspaces>, dir: i32) {
    let labels = state.labels.lock().unwrap();
    let n = labels.len();
    if n == 0 {
        return;
    }
    let cur = labels
        .iter()
        .position(|label| label == webview.label())
        .unwrap_or_else(|| *state.active.lock().unwrap());
    let target = (cur as i32 + dir).rem_euclid(n as i32) as usize;
    if target == cur {
        return;
    }
    let (current_label, target_label) = (labels[cur].clone(), labels[target].clone());
    drop(labels);
    if let Some(current) = app.get_webview(&current_label) {
        let _ = current.set_position(Position::Physical(tauri::PhysicalPosition::new(
            PARKED_X, 0,
        )));
    }
    if let Some(next) = app.get_webview(&target_label) {
        let _ = next.set_position(Position::Physical(tauri::PhysicalPosition::new(0, 0)));
        let _ = next.set_focus();
        show_workspace_badge(&next, target, n);
    }
    *state.active.lock().unwrap() = target;
}

fn create_workspace(app: &tauri::AppHandle, webview: Webview) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    let workspaces = app.state::<Workspaces>();
    if workspaces
        .creating
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return Err("Ya se está creando un workspace".into());
    }
    let mut labels = workspaces.labels.lock().unwrap();
    if labels.len() >= MAX_WORKSPACES {
        workspaces.creating.store(false, Ordering::Release);
        return Err(format!("Límite de {MAX_WORKSPACES} workspaces alcanzado"));
    }
    let cur = *workspaces.active.lock().unwrap();
    let label = format!("ws{}", labels.len());
    let current_label = labels[cur].clone();
    let window = webview.window();
    let Ok(size) = window.inner_size() else {
        workspaces.creating.store(false, Ordering::Release);
        return Err("No se pudo obtener el tamaño de la ventana".into());
    };
    labels.push(label.clone());
    drop(labels);

    let app_on_load = app.clone();
    let current_on_load = current_label.clone();
    let label_on_load = label.clone();
    let activated = Arc::new(AtomicBool::new(false));
    let activated_on_load = activated.clone();
    let builder = make_webview(&label, Arc::new(blocked_hosts()), app_state.adblock.clone())
        .on_page_load(move |new_webview, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished)
                && !activated_on_load.swap(true, Ordering::AcqRel)
            {
                if let Some(current) = app_on_load.get_webview(&current_on_load) {
                    let _ = current.set_position(Position::Physical(tauri::PhysicalPosition::new(
                        PARKED_X, 0,
                    )));
                }
                let _ = new_webview
                    .set_position(Position::Physical(tauri::PhysicalPosition::new(0, 0)));
                let _ = new_webview.set_focus();
                let state = app_on_load.state::<Workspaces>();
                let (index, total) = {
                    let labels = state.labels.lock().unwrap();
                    (
                        labels.iter().position(|item| item == &label_on_load),
                        labels.len(),
                    )
                };
                if let Some(index) = index {
                    *state.active.lock().unwrap() = index;
                    show_workspace_badge(&new_webview, index, total);
                }
                let _ = new_webview.eval(
                    "document.querySelector('#search-input')?.focus(); document.querySelector('#search-input')?.select()",
                );
                state.creating.store(false, Ordering::Release);
            }
        });
    let Ok(next) = window.add_child(
        builder,
        Position::Physical(tauri::PhysicalPosition::new(0, 0)),
        Size::Physical(size),
    ) else {
        workspaces
            .labels
            .lock()
            .unwrap()
            .retain(|item| item != &label);
        workspaces.creating.store(false, Ordering::Release);
        return Err("No se pudo crear el workspace".into());
    };
    drop(next);
    Ok(())
}

#[tauri::command]
fn wsnew(app: tauri::AppHandle, webview: Webview) -> Result<(), String> {
    create_workspace(&app, webview)
}

#[tauri::command]
fn setadblock(app: tauri::AppHandle, state: State<AppState>, enabled: bool) -> Result<(), String> {
    let previous = state.adblock.load(Ordering::Relaxed);
    state.adblock.store(enabled, Ordering::Relaxed);
    if let Err(error) = save_settings(&app, &state) {
        state.adblock.store(previous, Ordering::Relaxed);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
fn setengine(app: tauri::AppHandle, state: State<AppState>, engine: String) -> Result<(), String> {
    if !matches!(engine.as_str(), "ddg" | "google" | "bing") {
        return Err("Motor de búsqueda no válido".into());
    }
    let previous = {
        let mut current = state.engine.lock().unwrap();
        std::mem::replace(&mut *current, engine)
    };
    if let Err(error) = save_settings(&app, &state) {
        *state.engine.lock().unwrap() = previous;
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
fn getstate(state: State<AppState>) -> StatePayload {
    StatePayload {
        adblock: state.adblock.load(Ordering::Relaxed),
        engine: state.engine.lock().unwrap().clone(),
    }
}

#[tauri::command]
fn getworkspaces(state: State<Workspaces>) -> WorkspacePayload {
    WorkspacePayload {
        active: *state.active.lock().unwrap() + 1,
        total: state.labels.lock().unwrap().len(),
    }
}

fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

fn save_settings(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    let path = settings_path(app)
        .ok_or_else(|| "No se pudo encontrar la carpeta de configuración".to_string())?;
    let data = DiskSettings {
        adblock: state.adblock.load(Ordering::Relaxed),
        engine: state.engine.lock().unwrap().clone(),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string(&data).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

fn load_settings(app: &tauri::AppHandle) -> DiskSettings {
    let Some(path) = settings_path(app) else {
        return DiskSettings {
            adblock: true,
            engine: "google".into(),
        };
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(DiskSettings {
            adblock: true,
            engine: "google".into(),
        })
}

fn main() {
    let blocked = Arc::new(blocked_hosts());
    let adblock = Arc::new(AtomicBool::new(true));
    let workspace_shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyT);
    let handler_shortcut = workspace_shortcut;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &handler_shortcut && event.state() == ShortcutState::Pressed {
                        let state = app.state::<Workspaces>();
                        let active = *state.active.lock().unwrap();
                        let label = state.labels.lock().unwrap().get(active).cloned();
                        if let Some(webview) = label.and_then(|label| app.get_webview(&label)) {
                            let _ = create_workspace(app, webview);
                        }
                    }
                })
                .build(),
        )
        .manage(AppState {
            home_url: Mutex::new(build_home_url()),
            adblock: adblock.clone(),
            engine: Mutex::new("google".into()),
        })
        .manage(Workspaces {
            labels: Mutex::new(vec!["ws0".to_string()]),
            active: Mutex::new(0),
            creating: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            open,
            home,
            settings,
            reload,
            back,
            forward,
            ws,
            wsnew,
            setadblock,
            setengine,
            getstate,
            getworkspaces
        ])
        .setup(move |app| {
            app.global_shortcut().register(workspace_shortcut)?;
            let disk = load_settings(app.handle());
            adblock.store(disk.adblock, Ordering::Relaxed);
            *app.state::<AppState>().engine.lock().unwrap() = disk.engine;
            *app.state::<AppState>().home_url.lock().unwrap() = build_home_url();

            let window = WindowBuilder::new(app, "main")
                .title(" ")
                .inner_size(1440.0, 810.0)
                .min_inner_size(480.0, 320.0)
                .center()
                .build()?;
            let focus_app = app.handle().clone();
            let focus_shortcut = workspace_shortcut;
            window.on_window_event(move |event| match event {
                WindowEvent::Focused(true) => {
                    let _ = focus_app.global_shortcut().register(focus_shortcut);
                }
                WindowEvent::Focused(false) => {
                    let _ = focus_app.global_shortcut().unregister(focus_shortcut);
                }
                _ => {}
            });

            let size = window.inner_size()?;
            let wv = window.add_child(
                make_webview("ws0", blocked.clone(), adblock.clone()),
                Position::Physical(tauri::PhysicalPosition::new(0, 0)),
                Size::Physical(size),
            )?;
            let _ = wv.set_focus();
            show_workspace_badge(&wv, 0, 1);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al ejecutar minibrowser");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> HashSet<String> {
        blocked_hosts()
    }

    #[test]
    fn adblock_blocks_exact_domain() {
        let s = set();
        assert!(is_blocked("doubleclick.net", &s));
    }

    #[test]
    fn adblock_blocks_subdomain() {
        let s = set();
        assert!(is_blocked("securepubads.g.doubleclick.net", &s));
        assert!(is_blocked("www.google-analytics.com", &s));
    }

    #[test]
    fn adblock_allows_normal_sites() {
        let s = set();
        assert!(!is_blocked("example.com", &s));
        assert!(!is_blocked("wikipedia.org", &s));
        assert!(!is_blocked("doubleclick.evil.com", &s));
    }

    #[test]
    fn adblock_case_insensitive() {
        let s = set();
        assert!(is_blocked("DOUBLECLICK.NET", &s));
    }

    #[test]
    fn url_blocked_youtube_ads() {
        let u = "https://www.youtube.com/api/stats/ads?x=1";
        assert!(url_blocked(u));
        let u = "https://rr1---sn-xxx.googlevideo.com/videoplayback?id=abc&oad=1&x=2";
        assert!(url_blocked(u));
        let u = "https://rr1.googlevideo.com/videoplayback?id=abc&x=2";
        assert!(!url_blocked(u));
        assert!(url_blocked("https://www.google.com/aclk?sa=L&ai=test"));
        assert!(url_blocked(
            "https://www.youtube.com/pagead/interaction/?ai=test"
        ));
    }

    #[test]
    fn resolve_url_direct() {
        assert_eq!(
            resolve_query("example.com", "google"),
            "https://example.com"
        );
        assert_eq!(
            resolve_query("https://x.org/a", "google"),
            "https://x.org/a"
        );
        assert_eq!(
            resolve_query("localhost:8080", "google"),
            "http://localhost:8080"
        );
        assert_eq!(
            resolve_query("127.0.0.1:3000", "google"),
            "http://127.0.0.1:3000"
        );
    }

    #[test]
    fn resolve_url_search() {
        let g = resolve_query("gatos bonitos", "google");
        assert!(g.starts_with("https://www.google.com/search?q="));
        assert!(g.contains("gatos"));
    }

    #[test]
    fn resolve_empty_query_uses_selected_engine() {
        assert_eq!(
            resolve_query("  ", "ddg"),
            "https://html.duckduckgo.com/html/"
        );
        assert_eq!(resolve_query("", "bing"), "https://www.bing.com/");
    }
}
