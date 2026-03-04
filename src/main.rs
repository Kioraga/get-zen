use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation,
    PolicyType, ProgressBar, ScrolledWindow, TextTag, TextTagTable, TextView, WrapMode,
};
use reqwest::blocking::Client;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use std::{fs, thread};

rust_i18n::i18n!("locales");
use rust_i18n::t;

const APP_ID: &str = "io.github.kioraga.get-zen";
const ZEN_URL: &str =
    "https://github.com/zen-browser/desktop/releases/latest/download/zen-x86_64.AppImage";
const GEAR_LEVER_API_URL: &str =
    "https://api.github.com/repos/pkgforge-dev/Gear-Lever-AppImage/releases/latest";

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Message {
    Log(LogLevel, String),
    Progress(f64),
    Pulse,
    Done,
    Uninstalled,
    DownloadProgress { downloaded: u64, total: Option<u64>, speed_bps: f64 },
    Error(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

fn main() -> glib::ExitCode {
    // Auto-detect system locale checking several variables in priority order,
    // skipping empty values (LANGUAGE may be set but empty in GUI sessions).
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    if locale.starts_with("es") {
        rust_i18n::set_locale("es");
    } else {
        rust_i18n::set_locale("en");
    }

    // When running as an AppImage, prepend $APPDIR/usr/share to XDG_DATA_DIRS
    // BEFORE GTK initialises (GTK reads XDG_DATA_DIRS at its first init call).
    if let Ok(appdir) = std::env::var("APPDIR") {
        let share = format!("{}/usr/share", appdir);
        let current = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        if !current.split(':').any(|p| p == share) {
            std::env::set_var("XDG_DATA_DIRS", format!("{}:{}", share, current));
        }
    }

    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    // ── Icon theme search path (needed for AppImage / Wayland) ───────────────
    // Set the global default icon name for all windows.
    gtk4::Window::set_default_icon_name("get-zen");

    if let Some(display) = gtk4::gdk::Display::default() {
        let theme = gtk4::IconTheme::for_display(&display);
        // When running inside an AppImage, $APPDIR points to the mounted dir.
        if let Ok(appdir) = std::env::var("APPDIR") {
            theme.add_search_path(format!("{}/usr/share/icons", appdir));
        }
        // Also look for icons next to the binary (installed layout).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(bin_dir) = exe.parent() {
                theme.add_search_path(bin_dir.join("../share/icons"));
            }
        }
        // Development fallback: `cargo run` sets cwd to the project root,
        // where AppDir/usr/share/icons contains the icon.
        if let Ok(cwd) = std::env::current_dir() {
            let dev_icons = cwd.join("AppDir/usr/share/icons");
            if dev_icons.is_dir() {
                theme.add_search_path(dev_icons);
            }
        }
    }

    let window = ApplicationWindow::builder()
        .application(app)
        .title(&*t!("ui.window_title"))
        .default_width(640)
        .default_height(480)
        .resizable(false)
        .icon_name("get-zen")
        .build();

    // ── CSS ──────────────────────────────────────────────────────────────────
    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_string(
        "
        .log-view { font-family: monospace; font-size: 12px; }
        .tag-success { color: #26a269; }
        .tag-warning { color: #cd9309; }
        .tag-error   { color: #e01b24; }
        .tag-info    { color: #3584e4; }
        button.suggested-action label { color: white; }
        ",
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // ── Layout ───────────────────────────────────────────────────────────────
    let root = GtkBox::new(Orientation::Vertical, 0);

    // Header
    let header_box = GtkBox::new(Orientation::Vertical, 6);
    header_box.set_margin_top(20);
    header_box.set_margin_bottom(16);
    header_box.set_margin_start(20);
    header_box.set_margin_end(20);
    let title = Label::new(Some(&*t!("ui.title")));
    title.add_css_class("title-1");
    title.set_halign(Align::Start);

    let subtitle = Label::new(Some(&*t!("ui.subtitle")));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(Align::Start);
    subtitle.set_wrap(true);

    header_box.append(&title);
    header_box.append(&subtitle);

    // Log text view with tags
    let tag_table = TextTagTable::new();
    for (name, color) in [
        ("success", "#26a269"),
        ("warning", "#cd9309"),
        ("error", "#e01b24"),
        ("info", "#3584e4"),
        ("normal", ""),
    ] {
        let tag = TextTag::new(Some(name));
        if !color.is_empty() {
            tag.set_foreground(Some(color));
        }
        tag_table.add(&tag);
    }
    let buffer = gtk4::TextBuffer::new(Some(&tag_table));

    let text_view = TextView::with_buffer(&buffer);
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_wrap_mode(WrapMode::WordChar);
    text_view.set_monospace(true);
    text_view.set_left_margin(10);
    text_view.set_right_margin(10);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .margin_start(20)
        .margin_end(20)
        .child(&text_view)
        .build();

    // Progress bar
    let progress = ProgressBar::new();
    progress.set_margin_start(20);
    progress.set_margin_end(20);
    progress.set_margin_top(10);
    progress.set_margin_bottom(6);
    progress.set_show_text(true);
    progress.set_text(Some(&*t!("ui.ready")));

    // Button row
    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(Align::End);
    btn_box.set_margin_start(20);
    btn_box.set_margin_end(20);
    btn_box.set_margin_top(4);
    btn_box.set_margin_bottom(20);

    let cancel_btn = Button::with_label(&*t!("ui.btn_cancel"));
    let uninstall_btn = Button::with_label(&*t!("ui.btn_uninstall"));
    uninstall_btn.add_css_class("destructive-action");
    let install_btn = Button::with_label(&*t!("ui.btn_install"));
    install_btn.add_css_class("suggested-action");

    btn_box.append(&cancel_btn);
    btn_box.append(&uninstall_btn);
    btn_box.append(&install_btn);

    root.append(&header_box);
    root.append(&scroll);
    root.append(&progress);
    root.append(&btn_box);

    window.set_child(Some(&root));

    // ── Channels ─────────────────────────────────────────────────────────────
    let message_queue: Arc<Mutex<VecDeque<Message>>> = Arc::new(Mutex::new(VecDeque::new()));
    let running = Arc::new(AtomicBool::new(false));

    // ── Install button ────────────────────────────────────────────────────────
    {
        let running = running.clone();
        let queue = message_queue.clone();
        let install_btn_ref = install_btn.clone();
        let uninstall_btn_ref = uninstall_btn.clone();
        let cancel_btn_ref = cancel_btn.clone();

        install_btn.connect_clicked(move |_| {
            if running.load(Ordering::SeqCst) {
                return;
            }
            running.store(true, Ordering::SeqCst);
            install_btn_ref.set_sensitive(false);
            uninstall_btn_ref.set_sensitive(false);
            cancel_btn_ref.set_label(&*t!("ui.btn_cancel"));
            cancel_btn_ref.set_sensitive(false);

            let queue = queue.clone();
            thread::spawn(move || {
                run_installation(queue);
            });
        });
    }

    // ── Uninstall button ──────────────────────────────────────────────────────
    {
        let running = running.clone();
        let queue = message_queue.clone();
        let install_btn_ref = install_btn.clone();
        let uninstall_btn_ref = uninstall_btn.clone();
        let cancel_btn_ref = cancel_btn.clone();

        uninstall_btn.connect_clicked(move |_| {
            if running.load(Ordering::SeqCst) {
                return;
            }
            running.store(true, Ordering::SeqCst);
            install_btn_ref.set_sensitive(false);
            uninstall_btn_ref.set_sensitive(false);
            cancel_btn_ref.set_label(&*t!("ui.btn_cancel"));
            cancel_btn_ref.set_sensitive(false);

            let queue = queue.clone();
            thread::spawn(move || {
                run_uninstall(queue);
            });
        });
    }

    // ── Cancel button ─────────────────────────────────────────────────────────
    {
        let window_ref = window.clone();
        cancel_btn.connect_clicked(move |_| {
            window_ref.close();
        });
    }

    // ── Poll message queue every 50ms on the main thread ─────────────────────
    {
        let queue = message_queue.clone();
        let buffer = buffer.clone();
        let text_view = text_view.clone();
        let progress = progress.clone();
        let install_btn = install_btn.clone();
        let uninstall_btn = uninstall_btn.clone();
        let cancel_btn = cancel_btn.clone();
        let running = running.clone();

        // Mark at the start of the active download progress line (None = no active download)
        let dl_mark: Rc<RefCell<Option<gtk4::TextMark>>> = Rc::new(RefCell::new(None));

        glib::timeout_add_local(Duration::from_millis(50), move || {
            let mut q = queue.lock().unwrap();
            while let Some(msg) = q.pop_front() {
                match msg {
                    Message::Log(level, text) => {
                        // Close the active progress line
                        *dl_mark.borrow_mut() = None;
                        let tag_name = match level {
                            LogLevel::Success => "success",
                            LogLevel::Warning => "warning",
                            LogLevel::Error => "error",
                            LogLevel::Info => "info",
                        };
                        let prefix = match level {
                            LogLevel::Success => "✔ ",
                            LogLevel::Warning => "⚠ ",
                            LogLevel::Error => "✖ ",
                            LogLevel::Info => "● ",
                        };
                        let mut end = buffer.end_iter();
                        buffer.insert_with_tags_by_name(
                            &mut end,
                            &format!("{}{}\n", prefix, text),
                            &[tag_name],
                        );
                        let mark = buffer.create_mark(None, &buffer.end_iter(), false);
                        text_view.scroll_mark_onscreen(&mark);
                        buffer.delete_mark(&mark);
                    }
                    Message::DownloadProgress { downloaded, total, speed_bps } => {
                        let bar_text = format_download_bar(downloaded, total, speed_bps);
                        let mut mark_opt = dl_mark.borrow_mut();
                        if let Some(ref mark) = *mark_opt {
                            // Update the existing line in the buffer
                            let mut start = buffer.iter_at_mark(mark);
                            let mut end = start.clone();
                            end.forward_to_line_end();
                            buffer.delete(&mut start, &mut end);
                            let mut ins = buffer.iter_at_mark(mark);
                            buffer.insert_with_tags_by_name(&mut ins, &bar_text, &["info"]);
                        } else {
                            // First progress line: insert and save mark
                            let offset = buffer.end_iter().offset();
                            let mut end = buffer.end_iter();
                            buffer.insert_with_tags_by_name(
                                &mut end,
                                &format!("{}\n", bar_text),
                                &["info"],
                            );
                            let mark_iter = buffer.iter_at_offset(offset);
                            let mark = buffer.create_mark(
                                Some("dl_progress"),
                                &mark_iter,
                                true, // left gravity: the mark does not shift on insert
                            );
                            *mark_opt = Some(mark);
                            let scroll = buffer.create_mark(None, &buffer.end_iter(), false);
                            text_view.scroll_mark_onscreen(&scroll);
                            buffer.delete_mark(&scroll);
                        }
                    }
                    Message::Progress(value) => {
                        progress.set_fraction(value);
                        progress.set_text(Some(&format!("{:.0}%", value * 100.0)));
                    }
                    Message::Pulse => {
                        progress.pulse();
                    }
                    Message::Done => {
                        running.store(false, Ordering::SeqCst);
                        progress.set_fraction(1.0);
                        progress.set_text(Some(&*t!("ui.install_complete")));
                        install_btn.set_sensitive(false);
                        uninstall_btn.set_sensitive(true);
                        cancel_btn.set_label(&*t!("ui.btn_close"));
                        cancel_btn.set_sensitive(true);
                    }
                    Message::Uninstalled => {
                        running.store(false, Ordering::SeqCst);
                        progress.set_fraction(1.0);
                        progress.set_text(Some(&*t!("ui.uninstall_complete")));
                        install_btn.set_sensitive(true);
                        uninstall_btn.set_sensitive(false);
                        cancel_btn.set_label(&*t!("ui.btn_close"));
                        cancel_btn.set_sensitive(true);
                    }
                    Message::Error(err) => {
                        running.store(false, Ordering::SeqCst);
                        let mut end = buffer.end_iter();
                        buffer.insert_with_tags_by_name(
                            &mut end,
                            &format!("✖ {}\n", err),
                            &["error"],
                        );
                        progress.set_text(Some(&*t!("ui.operation_error")));
                        install_btn.set_sensitive(true);
                        uninstall_btn.set_sensitive(true);
                        cancel_btn.set_sensitive(true);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    window.present();
}

// ── Worker thread ─────────────────────────────────────────────────────────────

fn push(queue: &Arc<Mutex<VecDeque<Message>>>, msg: Message) {
    queue.lock().unwrap().push_back(msg);
}

fn run_installation(queue: Arc<Mutex<VecDeque<Message>>>) {
    macro_rules! log {
        ($level:expr, $($arg:tt)*) => {
            push(&queue, Message::Log($level, format!($($arg)*)));
        };
    }
    macro_rules! progress {
        ($v:expr) => {
            push(&queue, Message::Progress($v));
        };
    }

    log!(LogLevel::Info, "{}", t!("install.header"));
    log!(LogLevel::Info, "{}", t!("install.creating_tmp"));

    let temp_dir = match tempfile::Builder::new()
        .prefix("zen_install_")
        .tempdir()
    {
        Ok(d) => d,
        Err(e) => {
            push(&queue, Message::Error(t!("install.error_create_tmp", error = e.to_string()).to_string()));
            return;
        }
    };

    log!(
        LogLevel::Info,
        "{}",
        t!("install.tmp_dir", path = temp_dir.path().display().to_string())
    );

    let zen_path = temp_dir.path().join("zen.AppImage");
    let gear_lever_path = temp_dir.path().join("gear_lever.AppImage");

    // ── Download Zen Browser ───────────────────────────────────────────────
    log!(LogLevel::Info, "{}", t!("install.downloading_zen"));
    progress!(0.02);

    if let Err(e) = download_with_retry(ZEN_URL, &zen_path, &queue, 5, (0.05, 0.42)) {
        push(&queue, Message::Error(
            t!("install.error_download_zen", error = e.to_string()).to_string()
        ));
        return;
    }

    log!(LogLevel::Info, "{}", t!("install.waiting_next"));
    thread::sleep(Duration::from_secs(2));

    // ── Get GearLever URL ──────────────────────────────────────────────────
    log!(LogLevel::Info, "{}", t!("install.getting_gearlever_url"));
    let gear_lever_url = match get_gear_lever_url(&queue) {
        Ok(url) => {
            log!(LogLevel::Success, "{}", t!("install.url_obtained", url = url));
            url
        }
        Err(e) => {
            push(&queue, Message::Error(
                t!("install.error_gearlever_url", error = e.to_string()).to_string()
            ));
            return;
        }
    };
    progress!(0.50);

    // ── Download GearLever ─────────────────────────────────────────────────
    log!(LogLevel::Info, "{}", t!("install.downloading_gearlever"));
    if let Err(e) = download_with_retry(&gear_lever_url, &gear_lever_path, &queue, 5, (0.52, 0.80)) {
        push(&queue, Message::Error(
            t!("install.error_download_gearlever", error = e.to_string()).to_string()
        ));
        return;
    }

    // ── Execution permissions ──────────────────────────────────────────────
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(&zen_path, fs::Permissions::from_mode(0o755));
    let _ = fs::set_permissions(&gear_lever_path, fs::Permissions::from_mode(0o755));

    // ── Copy AppImage to persistent location ─────────────────────────────
    // GearLever generates the .desktop pointing to the path it is given.
    // If we pass the temp dir path, the .desktop will be broken after cleanup.
    // So we copy first to ~/AppImages/.
    let home_dir = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => {
            push(&queue, Message::Error(
                t!("install.error_no_home").to_string(),
            ));
            return;
        }
    };
    let appimages_dir = home_dir.join("AppImages");

    if let Err(e) = fs::create_dir_all(&appimages_dir) {
        push(&queue, Message::Error(t!("install.error_create_appimages", error = e.to_string()).to_string()));
        return;
    }

    // ── Remove previous installation so GearLever creates a new .desktop ───
    log!(LogLevel::Info, "{}", t!("install.removing_previous"));
    let apps_dir = home_dir.join(".local/share/applications");
    let icons_dir = home_dir.join(".local/share/icons");
    for desktop in find_zen_desktop_files(&apps_dir) {
        let _ = fs::remove_file(&desktop);
        log!(LogLevel::Info, "{}", t!("install.removed_file", path = desktop.display().to_string()));
    }
    // Remove any Zen AppImage that GearLever has saved (the name is assigned
    // by GearLever from internal metadata: could be zen.AppImage,
    // zen_browser.appimage, etc.)
    remove_zen_appimages_in_dir(&appimages_dir, &queue);
    // Remove previous Zen icons
    remove_icons(&icons_dir, "zen", &queue);
    // Force database update before integrating
    let _ = std::process::Command::new("update-desktop-database")
        .arg(apps_dir.to_str().unwrap_or(""))
        .output();

    // ── Integrate with GearLever ──────────────────────────────────────────
    // The temp path is passed: GearLever copies the AppImage to ~/AppImages/
    // and generates the .desktop pointing to that persistent location.
    // (The temp directory remains alive until the end of this function.)
    log!(LogLevel::Info, "{}", t!("install.integrating"));
    progress!(0.85);

    let zen_path_str = zen_path.to_string_lossy().to_string();
    use std::process::{Command, Stdio};

    match Command::new(&gear_lever_path)
        .arg("--integrate")
        .arg(&zen_path_str)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(b"y\n");
            }
            match child.wait() {
                Ok(status) if status.success() => {
                    log!(LogLevel::Success, "{}", t!("install.success"));
                    progress!(0.95);
                }
                Ok(_) => {
                    push(&queue, Message::Error(
                        t!("install.error_gearlever_exit").to_string(),
                    ));
                    return;
                }
                Err(e) => {
                    push(&queue, Message::Error(t!("install.error_gearlever_wait", error = e.to_string()).to_string()));
                    return;
                }
            }
        }
        Err(e) => {
            push(&queue, Message::Error(t!("install.error_run_gearlever", error = e.to_string()).to_string()));
            return;
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────
    log!(LogLevel::Info, "{}", t!("install.cleaning_tmp"));
    // temp_dir is removed when it goes out of scope

    log!(
        LogLevel::Success,
        "{}", t!("install.complete")
    );
    push(&queue, Message::Done);
}

// ── Get GearLever URL ───────────────────────────────────────────────────────

fn get_gear_lever_url(queue: &Arc<Mutex<VecDeque<Message>>>) -> Result<String, String> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    for attempt in 1u32..=5 {
        push(queue, Message::Log(
            LogLevel::Info,
            t!("gearlever.attempt", n = attempt).to_string(),
        ));

        match client
            .get(GEAR_LEVER_API_URL)
            .header("Accept", "application/vnd.github+json")
            .send()
        {
            Ok(response) => match response.json::<serde_json::Value>() {
                Ok(json) => {
                    if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
                        for asset in assets {
                            if let Some(url) = asset
                                .get("browser_download_url")
                                .and_then(|u| u.as_str())
                            {
                                if url.ends_with("x86_64.AppImage") {
                                    return Ok(url.to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    push(queue, Message::Log(
                        LogLevel::Warning,
                        t!("gearlever.error_parse", error = e.to_string()).to_string(),
                    ));
                }
            },
            Err(e) => {
                push(queue, Message::Log(
                    LogLevel::Warning,
                    t!("gearlever.error_network", error = e.to_string()).to_string(),
                ));
            }
        }

        if attempt < 5 {
            let wait = (attempt * 3) as u64;
            push(queue, Message::Log(
                LogLevel::Warning,
                t!("gearlever.retry_wait", secs = wait).to_string(),
            ));
            thread::sleep(Duration::from_secs(wait));
        }
    }

    Err(t!("gearlever.error_all_attempts").to_string())
}

// ── Download with retries ───────────────────────────────────────────────────

fn download_with_retry(
    url: &str,
    output: &Path,
    queue: &Arc<Mutex<VecDeque<Message>>>,
    max_attempts: u32,
    progress_range: (f64, f64),
) -> Result<(), String> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut wait_time = 3u64;

    for attempt in 1..=max_attempts {
        push(queue, Message::Log(
            LogLevel::Info,
            t!("download.attempt", n = attempt, total = max_attempts).to_string(),
        ));

        match client
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
        {
            Ok(mut response) if response.status().is_success() => {
                let total_bytes = response.content_length();

                match File::create(output) {
                    Ok(mut file) => {
                        let mut downloaded: u64 = 0;
                        let mut buf = vec![0u8; 65_536]; // 64 KB chunks
                        let (p_start, p_end) = progress_range;

                        // Speed tracking with a 400 ms sliding window
                        let mut speed_bytes: u64 = 0;
                        let mut speed_instant = Instant::now();
                        let mut current_speed: f64 = 0.0;

                        loop {
                            match response.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    file.write_all(&buf[..n])
                                        .map_err(|e| e.to_string())?;
                                    downloaded += n as u64;
                                    speed_bytes += n as u64;

                                    // Update speed every 400 ms
                                    let elapsed = speed_instant.elapsed();
                                    if elapsed >= Duration::from_millis(400) {
                                        current_speed = speed_bytes as f64 / elapsed.as_secs_f64();
                                        speed_bytes = 0;
                                        speed_instant = Instant::now();
                                    }

                                    // GTK progress bar
                                    if let Some(total) = total_bytes {
                                        if total > 0 {
                                            let frac = downloaded as f64 / total as f64;
                                            let p = p_start + frac * (p_end - p_start);
                                            push(queue, Message::Progress(p));
                                        }
                                    } else {
                                        push(queue, Message::Pulse);
                                    }

                                    // Log progress bar
                                    push(queue, Message::DownloadProgress {
                                        downloaded,
                                        total: total_bytes,
                                        speed_bps: current_speed,
                                    });
                                }
                                Err(e) => return Err(e.to_string()),
                            }
                        }

                        let mb = downloaded as f64 / (1024.0 * 1024.0);
                        push(queue, Message::Log(
                            LogLevel::Success,
                            t!("download.success", mb = format!("{:.1}", mb)).to_string(),
                        ));
                        push(queue, Message::Progress(p_end));
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(t!("download.error_create_file", error = e.to_string()).to_string());
                    }
                }
            }
            Ok(response) => {
                push(queue, Message::Log(
                    LogLevel::Warning,
                    t!("download.error_http", status = response.status().to_string()).to_string(),
                ));
            }
            Err(e) => {
                push(queue, Message::Log(
                    LogLevel::Warning,
                    t!("download.error_network", error = e.to_string()).to_string(),
                ));
            }
        }

        if attempt < max_attempts {
            push(queue, Message::Log(
                LogLevel::Warning,
                t!("download.retry_wait", secs = wait_time).to_string(),
            ));
            thread::sleep(Duration::from_secs(wait_time));
            wait_time *= 2;
        }
    }

    Err(t!("download.error_all_attempts", n = max_attempts).to_string())
}

// ── Download progress bar in log ────────────────────────────────────────────────

fn format_download_bar(downloaded: u64, total: Option<u64>, speed_bps: f64) -> String {
    const BAR_WIDTH: usize = 20;
    let speed_str = format_speed(speed_bps);
    let dl_mb = downloaded as f64 / (1024.0 * 1024.0);

    if let Some(total) = total {
        let total_mb = total as f64 / (1024.0 * 1024.0);
        let frac = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
        let filled = (frac * BAR_WIDTH as f64).round() as usize;
        let filled = filled.min(BAR_WIDTH);
        let bar: String = "█".repeat(filled) + &"░".repeat(BAR_WIDTH - filled);
        let pct = (frac * 100.0) as u32;
        format!("[{}] {}% • {:.1}/{:.1} MB • {}", bar, pct, dl_mb, total_mb, speed_str)
    } else {
        // No Content-Length: dot spinner
        let dots = (downloaded / 65_536) as usize % BAR_WIDTH;
        let bar: String = "░".repeat(dots) + "•" + &"░".repeat(BAR_WIDTH.saturating_sub(dots + 1));
        format!("[{}] {:.1} MB • {}", bar, dl_mb, speed_str)
    }
}

fn format_speed(bps: f64) -> String {
    if bps >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bps / (1024.0 * 1024.0))
    } else if bps >= 1024.0 {
        format!("{:.1} KB/s", bps / 1024.0)
    } else if bps > 0.0 {
        format!("{:.0} B/s", bps)
    } else {
        "-- B/s".to_string()
    }
}

// ── Uninstallation ──────────────────────────────────────────────────────────────

fn run_uninstall(queue: Arc<Mutex<VecDeque<Message>>>) {
    macro_rules! log {
        ($level:expr, $($arg:tt)*) => {
            push(&queue, Message::Log($level, format!($($arg)*)));
        };
    }

    log!(LogLevel::Info, "{}", t!("uninstall.header"));

    let home = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => {
            push(&queue, Message::Error(
                t!("uninstall.error_no_home").to_string(),
            ));
            return;
        }
    };

    let apps_dir = home.join(".local/share/applications");
    let icons_dir = home.join(".local/share/icons");

    // ── Search for Zen .desktop files ──────────────────────────────────────────
    log!(LogLevel::Info, "{}", t!("uninstall.looking_desktop"));

    let desktop_entries = find_zen_desktop_files(&apps_dir);

    // Directorio donde GearLever guarda las AppImages
    let appimages_dir = home.join("AppImages");

    if desktop_entries.is_empty() {
        log!(
            LogLevel::Warning,
            "{}", t!("uninstall.not_found")
        );
    } else {
        log!(
            LogLevel::Info,
            "{}", t!("uninstall.found_entries", n = desktop_entries.len())
        );
    }

    let total = desktop_entries.len();
    let mut removed_count = 0usize;

    for (i, desktop_path) in desktop_entries.iter().enumerate() {
        let progress_val = (i as f64 + 0.1) / total as f64;
        push(&queue, Message::Progress(progress_val));

        log!(
            LogLevel::Info,
            "{}", t!("uninstall.processing", path = desktop_path.display().to_string())
        );

        // Read the .desktop to extract AppImage and icon paths
        let appimage_path = read_exec_path(desktop_path);
        let icon_name = read_icon_name(desktop_path);

        // Remove the AppImage
        if let Some(ref path) = appimage_path {
            let p = std::path::Path::new(path);
            if p.exists() {
                match fs::remove_file(p) {
                    Ok(_) => { log!(LogLevel::Success, "{}", t!("uninstall.appimage_removed", path = path)); }
                    Err(e) => { log!(LogLevel::Warning, "{}", t!("uninstall.error_remove_appimage", path = path, error = e.to_string())); }
                }
            } else {
                log!(LogLevel::Warning, "{}", t!("uninstall.appimage_not_found", path = path));
            }
        }

        // Remove the .desktop file
        match fs::remove_file(desktop_path) {
            Ok(_) => {
                log!(LogLevel::Success, "{}", t!("uninstall.desktop_removed", path = desktop_path.display().to_string()));
                removed_count += 1;
            }
            Err(e) => {
                log!(
                    LogLevel::Warning,
                    "{}", t!("uninstall.error_remove_desktop", path = desktop_path.display().to_string(), error = e.to_string())
                );
            }
        }

        // Remove icons for this specific .desktop
        if let Some(ref name) = icon_name {
            remove_icons(&icons_dir, name, &queue);
        }

        push(&queue, Message::Progress((i as f64 + 0.9) / total as f64));
    }

    // ── Remove persistent AppImage (always, regardless of .desktop) ──────────
    // GearLever may assign different names (zen.AppImage, zen_browser.appimage…)
    // depending on internal metadata. We scan the entire directory.
    log!(LogLevel::Info, "{}", t!("uninstall.looking_appimage"));
    if appimages_dir.is_dir() {
        remove_zen_appimages_in_dir(&appimages_dir, &queue);
    } else {
        log!(LogLevel::Warning, "{}", t!("uninstall.no_appimages_dir"));
    }
    // Search in alternative locations in case it was installed differently
    for search_dir in [
        home.join("Applications"),
        home.join(".local/bin"),
    ] {
        if search_dir.is_dir() {
            remove_zen_appimages_in_dir(&search_dir, &queue);
        }
    }

    // ── Generic Zen icons ─────────────────────────────────────────────────
    remove_icons(&icons_dir, "zen", &queue);

    // Update desktop database
    log!(LogLevel::Info, "{}", t!("uninstall.updating_db"));
    let _ = std::process::Command::new("update-desktop-database")
        .arg(apps_dir.to_str().unwrap_or(""))
        .output();

    push(&queue, Message::Progress(1.0));

    if removed_count > 0 {
        log!(
            LogLevel::Success,
            "{}", t!("uninstall.success")
        );
    } else {
        log!(
            LogLevel::Warning,
            "{}", t!("uninstall.nothing_found")
        );
    }

    push(&queue, Message::Uninstalled);
}

// Returns all Zen .desktop files in `dir`
fn find_zen_desktop_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return results;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        // Check if the filename or content mentions Zen
        let is_zen = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase().contains("zen"))
            .unwrap_or(false)
            || fs::read_to_string(&path)
                .map(|c| c.contains("Zen Browser") || c.contains("zen-browser") || c.contains("zen_browser") || c.contains("zen.AppImage"))
                .unwrap_or(false);
        if is_zen {
            results.push(path);
        }
    }
    results
}

// Extracts the Exec= field value from a .desktop file (strips flags like %u, %f).
// Also handles the form: Exec=env KEY=VALUE /path/to/app %u
fn read_exec_path(desktop_path: &std::path::Path) -> Option<String> {
    let content = fs::read_to_string(desktop_path).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Exec=") {
            // Iterate tokens skipping "env" and any KEY=VALUE until
            // the first token that looks like a file path is found.
            for token in rest.split_whitespace() {
                if token == "env" {
                    continue; // env command
                }
                if token.starts_with('%') {
                    break; // .desktop placeholder, no more path
                }
                if token.contains('=') {
                    continue; // environment variable KEY=VALUE
                }
                return Some(token.to_string());
            }
        }
    }
    None
}

// Extracts the Icon= field value from a .desktop file
fn read_icon_name(desktop_path: &std::path::Path) -> Option<String> {
    let content = fs::read_to_string(desktop_path).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Icon=") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

// Recursively removes icons whose filename (without extension) matches `name`
fn remove_icons(
    icons_dir: &std::path::Path,
    name: &str,
    queue: &Arc<Mutex<VecDeque<Message>>>,
) {
    let Ok(iter) = walkdir(icons_dir) else {
        return;
    };
    for path in iter {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if stem.to_lowercase().contains(&name.to_lowercase()) {
            match fs::remove_file(&path) {
                Ok(_) => push(queue, Message::Log(
                    LogLevel::Success,
                    t!("icon.removed", path = path.display().to_string()).to_string(),
                )),
                Err(e) => push(queue, Message::Log(
                    LogLevel::Warning,
                    t!("icon.error_remove", path = path.display().to_string(), error = e.to_string()).to_string(),
                )),
            }
        }
    }
}

// Traverses a directory tree returning only regular files
fn walkdir(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    fn recurse(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(d)?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                recurse(&p, out)?;
            } else {
                out.push(p);
            }
        }
        Ok(())
    }
    recurse(dir, &mut files)?;
    Ok(files)
}

// Removes Zen *.AppImage files in `dir` (non-recursive)
fn remove_zen_appimages_in_dir(
    dir: &std::path::Path,
    queue: &Arc<Mutex<VecDeque<Message>>>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ext_lower = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext_lower != "appimage" {
            continue;
        }
        let name_lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if name_lower.contains("zen") {
            match fs::remove_file(&path) {
                Ok(_) => push(queue, Message::Log(
                    LogLevel::Success,
                    t!("uninstall.appimage_removed", path = path.display().to_string()).to_string(),
                )),
                Err(e) => push(queue, Message::Log(
                    LogLevel::Warning,
                    t!("uninstall.error_remove_appimage", path = path.display().to_string(), error = e.to_string()).to_string(),
                )),
            }
        }
    }
}
