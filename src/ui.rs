use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

pub fn step(msg: impl std::fmt::Display) {
    if !quiet() {
        eprintln!("{} {}", style("::").blue().bold(), msg);
    }
}

pub fn success(msg: impl std::fmt::Display) {
    if !quiet() {
        eprintln!("{} {}", style("✓").green().bold(), msg);
    }
}

pub fn warn(msg: impl std::fmt::Display) {
    eprintln!("{} {}", style("!").yellow().bold(), msg);
}

pub fn confirm(prompt: &str, yes: bool) -> bool {
    if yes {
        return true;
    }
    if !console::user_attended_stderr() {
        return false;
    }
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(true)
        .interact()
        .unwrap_or(false)
}

pub fn bytes_bar(len: Option<u64>, label: &str) -> ProgressBar {
    if quiet() {
        return ProgressBar::hidden();
    }
    let bar = match len {
        Some(len) => ProgressBar::new(len).with_style(
            ProgressStyle::with_template(
                "{msg:20!} [{bar:30.cyan/blue}] {bytes:>10}/{total_bytes:10} {bytes_per_sec:>12} {eta:>4}",
            )
            .unwrap()
            .progress_chars("=> "),
        ),
        None => ProgressBar::new_spinner().with_style(
            ProgressStyle::with_template("{spinner} {msg:20!} {bytes:>10} {bytes_per_sec:>12}")
                .unwrap(),
        ),
    };
    bar.set_message(label.to_string());
    bar
}

pub fn spinner(msg: impl Into<String>) -> ProgressBar {
    if quiet() {
        return ProgressBar::hidden();
    }
    let bar = ProgressBar::new_spinner()
        .with_style(ProgressStyle::with_template("{spinner:.blue} {msg}").unwrap());
    bar.set_message(msg.into());
    bar.enable_steady_tick(std::time::Duration::from_millis(100));
    bar
}

/// npub for a hex pubkey, shortened for display
pub fn npub(hex: &str) -> String {
    match hex.parse::<ritualistic::PubKey>() {
        Ok(pk) => pk.to_npub(),
        Err(_) => hex.to_string(),
    }
}

pub fn short_npub(hex: &str) -> String {
    let n = npub(hex);
    if n.len() > 16 {
        format!("{}…{}", &n[..10], &n[n.len() - 4..])
    } else {
        n
    }
}

pub fn human_size(bytes: u64) -> String {
    indicatif::HumanBytes(bytes).to_string()
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn date(ts: u64) -> String {
    // days since epoch to a civil date, so we don't need a date crate just for this
    let days = (ts / 86400) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
