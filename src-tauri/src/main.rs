#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::http::{Request, Response};
use tauri::{
    Manager, Position, Size, State, Webview, WebviewBuilder, WebviewUrl, WindowBuilder,
};
use tauri::WindowEvent;

struct AppState {
    home_url: Mutex<url::Url>,
    adblock: Arc<AtomicBool>,
    engine: Mutex<String>,
    blocked: Arc<HashSet<String>>,
}

struct Workspaces {
    labels: Mutex<Vec<String>>,
    active: Mutex<usize>,
}

#[derive(serde::Serialize)]
struct StatePayload {
    adblock: bool,
    engine: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DiskSettings {
    adblock: bool,
    engine: String,
}

// Flags de Chromium: desactiva GPU (menos RAM) y fuerza DNS-over-HTTPS a Cloudflare.
const ARGS: &str = "--disable-gpu --enable-features=dns-over-https";

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
    "/youtubei/v1/player/get_ad_break",
    "youtube.com/api/timedtext?type=track",
    "adservice.google.com",
    "googleads.g.doubleclick.net",
    "pagead2.googlesyndication.com",
    "securepubads.g.doubleclick.net",
    "static.doubleclick.net",
    "ad.doubleclick.net",
    "g.doubleclick.net",
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
document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  const isLocal = location.hostname === 'tauri.localhost';
  const go = (cmd, args) => {
    e.preventDefault();
    try { window.__TAURI__.core.invoke(cmd, args); } catch (_) {}
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
        && (q.contains('.') || q.contains(':') || q.contains('/')
            || q.eq_ignore_ascii_case("localhost"));
    if looks_like_url {
        return format!("https://{}", q);
    }
    let enc: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
    match engine {
        "google" => format!("https://www.google.com/search?q={}", enc),
        "bing" => format!("https://www.bing.com/search?q={}", enc),
        _ => format!("https://html.duckduckgo.com/html/?q={}", enc),
    }
}

fn parse_url(s: &str) -> url::Url {
    url::Url::parse(s)
        .unwrap_or_else(|_| url::Url::parse("https://www.google.com/").unwrap())
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
        .additional_browser_args(ARGS)
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

#[tauri::command]
fn open(webview: Webview, state: State<AppState>, query: String) {
    let engine = state.engine.lock().unwrap().clone();
    let url = parse_url(&resolve_query(&query, &engine));
    let _ = webview.navigate(url);
}

#[tauri::command]
fn home(webview: Webview, state: State<AppState>) {
    let url = state.home_url.lock().unwrap().clone();
    let _ = webview.navigate(url);
}

#[tauri::command]
fn settings(webview: Webview, state: State<AppState>) {
    let mut url = state.home_url.lock().unwrap().clone();
    let _ = url.set_path("settings.html");
    let _ = webview.navigate(url);
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
fn ws(app: tauri::AppHandle, webview: Webview, dir: i32) {
    let st = app.state::<Workspaces>();
    let labels = st.labels.lock().unwrap();
    let n = labels.len();
    if n == 0 {
        return;
    }
    let cur = labels
        .iter()
        .position(|l| l == webview.label())
        .unwrap_or_else(|| *st.active.lock().unwrap());
    let target = (cur as i32 + dir).rem_euclid(n as i32) as usize;
    if target == cur {
        return;
    }
    let (cur_label, target_label) = (labels[cur].clone(), labels[target].clone());
    drop(labels);
    if let Some(w) = app.get_webview(&cur_label) {
        let _ = w.hide();
    }
    if let Some(w) = app.get_webview(&target_label) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    *st.active.lock().unwrap() = target;
}

#[tauri::command]
fn wsnew(app: tauri::AppHandle, webview: Webview, state: State<AppState>) {
    let st = app.state::<Workspaces>();
    let n = st.labels.lock().unwrap().len();
    let label = format!("ws{}", n);
    let window = webview.window();
    let Ok(size) = window.inner_size() else {
        return;
    };
    let builder = make_webview(&label, state.blocked.clone(), state.adblock.clone());
    let Ok(wv) = window.add_child(
        builder,
        Position::Physical(tauri::PhysicalPosition::new(0, 0)),
        Size::Physical(size),
    ) else {
        return;
    };
    let cur = *st.active.lock().unwrap();
    let cur_label = st.labels.lock().unwrap()[cur].clone();
    st.labels.lock().unwrap().push(label.clone());
    if let Some(w) = app.get_webview(&cur_label) {
        let _ = w.hide();
    }
    let _ = wv.show();
    let _ = wv.set_focus();
    *st.active.lock().unwrap() = n;
    let _ = wv.navigate(state.home_url.lock().unwrap().clone());
}

#[tauri::command]
fn setadblock(app: tauri::AppHandle, state: State<AppState>, enabled: bool) {
    state.adblock.store(enabled, Ordering::Relaxed);
    save_settings(&app, &state);
}

#[tauri::command]
fn setengine(app: tauri::AppHandle, state: State<AppState>, engine: String) {
    *state.engine.lock().unwrap() = engine;
    save_settings(&app, &state);
}

#[tauri::command]
fn getstate(state: State<AppState>) -> StatePayload {
    StatePayload {
        adblock: state.adblock.load(Ordering::Relaxed),
        engine: state.engine.lock().unwrap().clone(),
    }
}

fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

fn save_settings(app: &tauri::AppHandle, state: &AppState) {
    let Some(path) = settings_path(app) else { return };
    let data = DiskSettings {
        adblock: state.adblock.load(Ordering::Relaxed),
        engine: state.engine.lock().unwrap().clone(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string(&data).unwrap());
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

    tauri::Builder::default()
        .manage(AppState {
            home_url: Mutex::new(build_home_url()),
            adblock: adblock.clone(),
            engine: Mutex::new("google".into()),
            blocked: blocked.clone(),
        })
        .manage(Workspaces {
            labels: Mutex::new(vec!["ws0".to_string()]),
            active: Mutex::new(0),
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
            getstate
        ])
        .setup(move |app| {
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

            let app2 = app.handle().clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Resized(size) = event {
                    let ws = app2.state::<Workspaces>();
                    let labels = ws.labels.lock().unwrap();
                    for l in labels.iter() {
                        if let Some(w) = app2.get_webview(l) {
                            let _ = w.set_size(Size::Physical(*size));
                            let _ = w.set_position(Position::Physical(
                                tauri::PhysicalPosition::new(0, 0),
                            ));
                        }
                    }
                }
            });

            let size = window.inner_size()?;
            let wv = window.add_child(
                make_webview("ws0", blocked.clone(), adblock.clone()),
                Position::Physical(tauri::PhysicalPosition::new(0, 0)),
                Size::Physical(size),
            )?;
            let _ = wv.set_focus();

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
    }

    #[test]
    fn resolve_url_direct() {
        assert_eq!(resolve_query("example.com", "google"), "https://example.com");
        assert_eq!(
            resolve_query("https://x.org/a", "google"),
            "https://x.org/a"
        );
        assert_eq!(
            resolve_query("localhost:8080", "google"),
            "https://localhost:8080"
        );
    }

    #[test]
    fn resolve_url_search() {
        let g = resolve_query("gatos bonitos", "google");
        assert!(g.starts_with("https://www.google.com/search?q="));
        assert!(g.contains("gatos"));
    }
}