use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, OnceLock};

struct CompatTracker {
    missing_apis: HashMap<String, u32>,
    js_exceptions: HashMap<String, u32>,
    unsupported_css: HashMap<String, u32>,
}

static COMPAT: OnceLock<Mutex<CompatTracker>> = OnceLock::new();

fn compat() -> &'static Mutex<CompatTracker> {
    COMPAT.get_or_init(|| {
        Mutex::new(CompatTracker {
            missing_apis: HashMap::new(),
            js_exceptions: HashMap::new(),
            unsupported_css: HashMap::new(),
        })
    })
}

/// Call when a Web API stub is invoked. Logs the first occurrence at WARN;
/// all occurrences are counted for the shutdown summary.
pub fn track_missing_api(name: &str) {
    let is_first = {
        let mut t = compat().lock().unwrap();
        let c = t.missing_apis.entry(name.to_string()).or_insert(0);
        *c += 1;
        *c == 1
    };
    if is_first {
        log::warn!(target: "aurora::api", "[API] missing: {}", name);
    }
}

/// Call when a JS exception is caught. Logs the first occurrence of each
/// unique message at WARN; all occurrences are counted.
pub fn track_js_exception(msg: &str) {
    let short = msg.lines().next().unwrap_or(msg);
    let is_first = {
        let mut t = compat().lock().unwrap();
        let c = t.js_exceptions.entry(short.to_string()).or_insert(0);
        *c += 1;
        *c == 1
    };
    if is_first {
        log::warn!(target: "aurora::js", "[JS] exception: {}", short);
    }
}

/// Call when the CSS engine encounters an unsupported feature.
pub fn track_css_unsupported(feature: &str) {
    let is_first = {
        let mut t = compat().lock().unwrap();
        let c = t.unsupported_css.entry(feature.to_string()).or_insert(0);
        *c += 1;
        *c == 1
    };
    if is_first {
        log::warn!(target: "aurora::css", "[CSS] unsupported: {}", feature);
    }
}

/// Print the full deduped compatibility summary to stderr. Call once at shutdown.
pub fn print_compat_summary() {
    let t = compat().lock().unwrap();
    if t.missing_apis.is_empty() && t.js_exceptions.is_empty() && t.unsupported_css.is_empty() {
        return;
    }

    eprintln!("\n=== Aurora compatibility summary ===");

    if !t.missing_apis.is_empty() {
        eprintln!("Missing Web APIs:");
        let mut v: Vec<_> = t.missing_apis.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in v {
            eprintln!("  {} x{}", name, count);
        }
    }

    if !t.js_exceptions.is_empty() {
        eprintln!("JS exceptions:");
        let mut v: Vec<_> = t.js_exceptions.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (msg, count) in v {
            eprintln!("  {} x{}", msg, count);
        }
    }

    if !t.unsupported_css.is_empty() {
        eprintln!("Unsupported CSS:");
        let mut v: Vec<_> = t.unsupported_css.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (feat, count) in v {
            eprintln!("  {} x{}", feat, count);
        }
    }
}

pub fn init() {
    let _ = fs::create_dir_all("logs");
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_path = format!("logs/aurora_{}.log", timestamp);

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                record.level(),
                message
            ))
        })
        // ── Default level ─────────────────────────────────────────────────────
        .level(log::LevelFilter::Info)
        // ── Aurora channels ───────────────────────────────────────────────────
        .level_for("aurora::net",    log::LevelFilter::Info)
        .level_for("aurora::html",   log::LevelFilter::Info)
        .level_for("aurora::parser", log::LevelFilter::Off)  // opt-in: flip to Trace
        .level_for("aurora::css",    log::LevelFilter::Warn)
        .level_for("aurora::js",     log::LevelFilter::Info) // TEMP: was Warn — need [yt-life]/console.log traces for custom-element debugging
        .level_for("aurora::api",    log::LevelFilter::Warn)
        .level_for("aurora::layout", log::LevelFilter::Debug)
        .level_for("aurora::render", log::LevelFilter::Debug)
        // ── Silence noisy deps ────────────────────────────────────────────────
        .level_for("selectors",      log::LevelFilter::Off)
        .chain(std::io::stderr())
        .chain(fern::log_file(&log_path).expect("could not open log file"))
        .apply()
        .ok();

    log::info!("Aurora logging started → {}", log_path);
}
