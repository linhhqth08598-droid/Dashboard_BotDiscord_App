#[cfg(desktop)]
use tauri::webview::NewWindowResponse;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;
#[cfg(desktop)]
use tauri::Manager;

/// Mã định danh app (phải trùng `identifier` trong tauri.conf.json). Windows dùng làm AppUserModelID.
#[cfg(windows)]
const APP_ID: &str = "vn.io.linhhq.dashboardbot";
#[cfg(windows)]
const APP_NAME: &str = "Discord Bot Dashboard";

/// Windows: khai báo "danh tính" của app. Không có bước này, ô nhạc của Windows (phím media, màn hình khóa...)
/// không biết nhạc đến từ app nào nên ghi "Ứng dụng không xác định".
#[cfg(windows)]
fn register_windows_identity() {
    use windows::{core::w, Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID};
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("vn.io.linhhq.dashboardbot"));
    }

    let _ = (|| -> std::io::Result<()> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))?;
        key.set_value("DisplayName", &APP_NAME)?;
        // Icon hiện cạnh tên app: ghi file icon ra thư mục dữ liệu rồi trỏ tới
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            let dir = std::path::PathBuf::from(dir).join(APP_ID);
            if std::fs::create_dir_all(&dir).is_ok() {
                let icon = dir.join("icon.png");
                if std::fs::write(&icon, include_bytes!("../icons/128x128.png")).is_ok() {
                    key.set_value("IconUri", &icon.to_string_lossy().to_string())?;
                }
            }
        }
        Ok(())
    })();
}

/// Đưa cửa sổ chính lên (từ khay hệ thống, hoặc khi mở app lần 2).
#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Biểu tượng ở khay hệ thống (giống Discord): bấm trái = mở lại cửa sổ, bấm phải = menu
/// (Dashboard / Kiểm tra cập nhật... / Web / Thoát Dashboard). Chỉ "Thoát Dashboard" mới tắt app (và nhạc).
#[cfg(desktop)]
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::menu::{IconMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    // Dòng đầu: logo + tên app, chữ xám (không bấm được) - giống dòng "Discord" ở menu khay của Discord.
    let title = IconMenuItemBuilder::with_id("title", "Dashboard")
        .icon(Image::from_bytes(include_bytes!("../icons/32x32.png"))?)
        .enabled(false)
        .build(app)?;
    let check = MenuItemBuilder::with_id("check_update", "Kiểm tra cập nhật...").build(app)?;
    let web = MenuItemBuilder::with_id("web", format!("Web ({DASHBOARD_HOST})")).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Thoát Dashboard").build(app)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = MenuBuilder::new(app)
        .item(&title)
        .item(&sep1)
        .item(&check)
        .item(&web)
        .item(&sep2)
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/32x32.png"))?)
        .tooltip("Discord Bot Dashboard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "check_update" => check_for_updates(app.clone(), true),
            "web" => {
                if let Ok(url) = tauri::Url::parse(&format!("https://{DASHBOARD_HOST}")) {
                    open_external(app, &url);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(desktop)]
const FULLSCREEN_JS: &str = r#"
if (window.top === window) {
  window.addEventListener('keydown', function (e) {
    if (e.key !== 'F11') return;
    e.preventDefault();
    if (document.fullscreenElement) document.exitFullscreen();
    else document.documentElement.requestFullscreen();
  }, true);
}
"#;

/// Địa chỉ (host) của dashboard: chỉ trang này và trang đăng nhập Discord mới được mở bên trong app.
const DASHBOARD_HOST: &str = "linhhq.io.vn";

/// Android: WebView của app không tự mở được liên kết `target="_blank"` / `window.open(...)`
/// (nút "Thêm Vào Server", nút website của bot...). Script này đổi chúng thành điều hướng thường,
/// rồi `on_navigation` bên dưới chặn và mở bằng trình duyệt của điện thoại.
#[cfg(mobile)]
const MOBILE_LINKS_JS: &str = r#"
(function () {
  if (window.top !== window || window.__dashLinks) return;
  window.__dashLinks = true;
  function go(u) {
    try { window.location.assign(new URL(String(u), window.location.href).href); } catch (e) {}
  }
  window.open = function (u) { if (u) go(u); return null; };
  document.addEventListener('click', function (e) {
    var el = e.target;
    var a = el && el.closest ? el.closest('a[href]') : null;
    if (!a) return;
    var t = (a.getAttribute('target') || '').toLowerCase();
    if (t === '_blank' || t === '_new') { e.preventDefault(); go(a.href); }
  }, true);
})();
"#;

/// Có cho trang này tải ngay trong app không?
/// - Dashboard, và trang nội bộ của app: có.
/// - Discord: có cho đăng nhập (OAuth/login), nhưng link MỜI BOT (có scope=bot, permissions=, guild_id=) thì mở ngoài.
/// - Mọi trang khác (website của bot, GitHub...): mở bằng trình duyệt.
fn allow_in_app(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "about" | "data" | "blob" | "file" => return true,
        "http" | "https" => {}
        _ => return false,
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host == DASHBOARD_HOST
        || host == "tauri.localhost"
        || host == "localhost"
        || host.ends_with(".localhost")
    {
        return true;
    }
    let is_discord = host == "discord.com"
        || host.ends_with(".discord.com")
        || host == "discordapp.com"
        || host.ends_with(".discordapp.com");
    if is_discord {
        let q = url.query().unwrap_or("").to_ascii_lowercase();
        let is_invite =
            q.contains("scope=bot") || q.contains("permissions=") || q.contains("guild_id=");
        return !is_invite;
    }
    false
}

/// Mở địa chỉ web bằng trình duyệt mặc định của máy (Windows / Android).
fn open_external(app: &tauri::AppHandle, url: &tauri::Url) {
    if !matches!(url.scheme(), "http" | "https" | "mailto" | "tel") {
        return;
    }
    // on_navigation chạy trên luồng giao diện của WebView; gọi plugin opener (cần chờ phía Android trả lời)
    // ngay tại đây có thể bị treo/không mở được => chuyển sang một luồng riêng.
    let app = app.clone();
    let url = url.to_string();
    std::thread::spawn(move || {
        use tauri_plugin_opener::OpenerExt;
        if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
            eprintln!("Không mở được liên kết: {e}");
        }
    });
}

/// Trang phát hành: nơi tải bộ cài thủ công khi tự cập nhật lỗi.
#[cfg(desktop)]
const RELEASES_URL: &str = "https://github.com/linhhqth08598-droid/Dashboard_BotDiscord_App/releases/latest";

/// Đang có một lần kiểm tra / cập nhật chạy: tránh hiện 2 hộp thoại cùng lúc.
#[cfg(desktop)]
static UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Phiên bản người dùng đã bấm "Để sau": lần tự kiểm tra định kỳ không hỏi lại bản đó nữa (đến khi mở lại app).
#[cfg(desktop)]
static DECLINED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

#[cfg(desktop)]
fn update_message(app: &tauri::AppHandle, text: impl Into<String>) {
    let _ = app.dialog().message(text.into()).title("Cập nhật ứng dụng").blocking_show();
}

/// Lỗi cập nhật: báo rõ lý do (trước đây im lặng nên tưởng "bấm mà không có gì xảy ra") và mời tải bộ cài thủ công.
#[cfg(desktop)]
fn update_failed(app: &tauri::AppHandle, what: &str, err: &str) {
    eprintln!("{what}: {err}");
    let open = app
        .dialog()
        .message(format!(
            "{what}.\n\nChi tiết: {err}\n\nBạn có thể tải bộ cài mới và cài đè lên bản đang dùng (không cần gỡ bản cũ). Mở trang tải ngay?"
        ))
        .title("Cập nhật ứng dụng")
        .buttons(MessageDialogButtons::OkCancelCustom("Mở trang tải".to_string(), "Đóng".to_string()))
        .blocking_show();
    if open {
        if let Ok(url) = tauri::Url::parse(RELEASES_URL) {
            open_external(app, &url);
        }
    }
}

/// Kiểm tra bản mới; nếu có thì hỏi người dùng rồi tải và cài (chỉ máy tính;
/// bản Android xem check_for_updates_mobile).
/// `manual` = bấm "Kiểm tra cập nhật..." ở khay: khi không có bản mới hoặc bị lỗi thì báo lại cho người dùng
/// (kiểm tra tự động khi mở app / định kỳ thì im lặng nếu không có gì mới).
#[cfg(desktop)]
fn check_for_updates(app: tauri::AppHandle, manual: bool) {
    use std::sync::atomic::Ordering;
    if UPDATING.swap(true, Ordering::SeqCst) {
        if manual {
            let _ = app.dialog().message("Đang kiểm tra / tải cập nhật rồi, vui lòng chờ chút.").title("Cập nhật ứng dụng").show(|_| {});
        }
        return;
    }
    tauri::async_runtime::spawn(async move {
        run_update(&app, manual).await;
        UPDATING.store(false, Ordering::SeqCst);
    });
}

#[cfg(desktop)]
async fn run_update(app: &tauri::AppHandle, manual: bool) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            if manual { update_failed(app, "Không khởi tạo được bộ cập nhật", &e.to_string()); }
            else { eprintln!("Không khởi tạo được updater: {e}"); }
            return;
        }
    };
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            if manual {
                update_message(app, format!("Bạn đang dùng phiên bản mới nhất ({}).", app.package_info().version));
            }
            return;
        }
        Err(e) => {
            if manual { update_failed(app, "Không kiểm tra được bản cập nhật", &e.to_string()); }
            else { eprintln!("Không kiểm tra được cập nhật: {e}"); }
            return;
        }
    };

    // Kiểm tra định kỳ: bản đã bấm "Để sau" thì không làm phiền lại
    if !manual && DECLINED.lock().map(|d| *d == update.version).unwrap_or(false) {
        return;
    }

    let agree = app
        .dialog()
        .message(format!(
            "Đã có phiên bản mới {} (bạn đang dùng {}). Cập nhật ngay?\n\nỨng dụng (và nhạc đang phát) sẽ tự đóng để cài, rồi tự mở lại.",
            update.version, update.current_version
        ))
        .title("Cập nhật ứng dụng")
        .buttons(MessageDialogButtons::OkCancelCustom("Cập nhật".to_string(), "Để sau".to_string()))
        .blocking_show();
    if !agree {
        if let Ok(mut d) = DECLINED.lock() { *d = update.version.clone(); }
        return;
    }

    // Báo đang tải (trước đây bấm xong im lặng nên tưởng không chạy): tiến độ hiện ở tooltip của biểu tượng khay.
    let tray = app.tray_by_id("main");
    if let Some(t) = &tray { let _ = t.set_tooltip(Some("Đang tải bản cập nhật... 0%")); }
    let _ = app
        .dialog()
        .message("Đang tải bản cập nhật ở nền. Tải xong ứng dụng sẽ tự đóng, cài đặt rồi mở lại. Tiến độ hiện khi rê chuột vào biểu tượng ở khay hệ thống.")
        .title("Cập nhật ứng dụng")
        .show(|_| {});

    let mut downloaded: u64 = 0;
    let mut last_pct: u64 = 0;
    let tray_for_progress = tray.clone();
    let result = update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                if let (Some(total), Some(t)) = (total, &tray_for_progress) {
                    if total > 0 {
                        let pct = downloaded * 100 / total;
                        if pct >= last_pct + 5 || pct == 100 {
                            last_pct = pct;
                            let _ = t.set_tooltip(Some(format!("Đang tải bản cập nhật... {pct}%")));
                        }
                    }
                }
            },
            || {},
        )
        .await;

    if let Some(t) = &tray { let _ = t.set_tooltip(Some("Discord Bot Dashboard")); }
    if let Err(e) = result {
        update_failed(app, "Tải / cài bản cập nhật thất bại", &e.to_string());
        return;
    }
    // Windows: bộ cài đã chạy và app thường đã tự thoát trước dòng này; các hệ khác thì khởi động lại tại đây.
    app.restart();
}

/// Địa chỉ file latest.json do GitHub Actions tạo ra mỗi lần phát hành bản mới.
#[cfg(mobile)]
const LATEST_JSON_URL: &str = "https://github.com/linhhqth08598-droid/Dashboard_BotDiscord_App/releases/latest/download/latest.json";

/// Trang tải bản mới (nơi có file APK). Bấm "Cập nhật" sẽ mở trang này trong trình duyệt.
#[cfg(mobile)]
const DOWNLOAD_PAGE_URL: &str =
    "https://github.com/linhhqth08598-droid/Dashboard_BotDiscord_App/releases/latest";

/// "0.1.5" hoặc "v0.1.5" -> (0, 1, 5). Phần không phải số (vd "-beta") bị bỏ qua.
#[cfg(mobile)]
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u64>()
            .ok()
    });
    Some((
        parts.next()??,
        parts.next().unwrap_or(Some(0))?,
        parts.next().unwrap_or(Some(0))?,
    ))
}

/// Android: khi mở app, hỏi GitHub bản mới nhất. Nếu mới hơn bản đang chạy thì hiện hộp thoại
/// "Cập nhật" / "Để sau". Bấm "Cập nhật" -> mở trang tải APK; "Để sau" -> lần mở app sau hỏi lại.
#[cfg(mobile)]
fn check_for_updates_mobile(app: tauri::AppHandle) {
    use tauri_plugin_opener::OpenerExt;

    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Không tạo được HTTP client: {e}");
                return;
            }
        };
        let json: serde_json::Value = match client.get(LATEST_JSON_URL).send().await {
            Ok(r) if r.status().is_success() => match r.json().await {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("latest.json không hợp lệ: {e}");
                    return;
                }
            },
            Ok(r) => {
                eprintln!("Không tải được latest.json: HTTP {}", r.status());
                return;
            }
            Err(e) => {
                eprintln!("Không kiểm tra được cập nhật: {e}");
                return;
            }
        };

        let Some(latest_str) = json.get("version").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(latest) = parse_version(latest_str) else {
            return;
        };
        let current_ver = &app.package_info().version;
        let current = (current_ver.major, current_ver.minor, current_ver.patch);
        if latest <= current {
            return;
        }

        let handle = app.clone();
        app.dialog()
            .message(format!(
                "Đã có phiên bản mới {} (bạn đang dùng {}). Cập nhật ngay?",
                latest_str.trim_start_matches(['v', 'V']),
                current_ver
            ))
            .title("Cập nhật ứng dụng")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Cập nhật".to_string(),
                "Để sau".to_string(),
            ))
            .show(move |agree| {
                if agree {
                    if let Err(e) = handle.opener().open_url(DOWNLOAD_PAGE_URL, None::<&str>) {
                        eprintln!("Không mở được trang tải: {e}");
                    }
                }
            });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Phải đặt TRƯỚC khi tạo cửa sổ / WebView, để Windows gắn nhạc với đúng tên app.
    #[cfg(windows)]
    register_windows_identity();

    // Máy tính: mở app lần 2 thì chỉ đưa cửa sổ đang chạy (có thể đang ẩn dưới khay) lên. Plugin này phải đăng ký đầu tiên.
    #[cfg(desktop)]
    let builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        show_main_window(app);
    }));
    #[cfg(not(desktop))]
    let builder = tauri::Builder::default();

    // Hộp thoại + mở trang tải + điều khiển media (SMTC / thông báo nhạc): dùng cho cả máy tính và Android.
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dash_media::init());

    // Máy tính: bấm X chỉ ẩn cửa sổ xuống khay, nhạc vẫn phát nền (giống Discord / Spotify). Tắt hẳn bằng menu khay.
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });

    // Updater chỉ có trên máy tính (Windows/macOS/Linux).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|app| {
            // Báo cho trang web biết đang chạy trong app và phiên bản app (hiện ở chân trang).
            let platform = if cfg!(mobile) { "android" } else { "desktop" };
            let app_info_js = format!(
                "window.__DASH_APP__ = {{ version: '{}', platform: '{}' }};",
                app.package_info().version,
                platform
            );

            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .initialization_script(app_info_js.as_str());

            // Liên kết ngoài (mời bot, website của bot, Discord hỗ trợ...) mở bằng trình duyệt mặc định,
            // cả trên máy tính lẫn Android. Trang dashboard / đăng nhập Discord vẫn mở trong app.
            let nav_handle = app.handle().clone();
            let window = window.on_navigation(move |url| {
                if allow_in_app(url) {
                    true
                } else {
                    open_external(&nav_handle, url);
                    false
                }
            });

            // Kích thước cửa sổ, phím F11 và chặn cửa sổ mới: chỉ áp dụng trên máy tính.
            #[cfg(desktop)]
            let window = {
                let new_win_handle = app.handle().clone();
                window
                    .title("Discord Bot Dashboard")
                    .inner_size(1360.0, 860.0)
                    .min_inner_size(900.0, 600.0)
                    .initialization_script(FULLSCREEN_JS)
                    .on_new_window(move |url, _features| {
                        open_external(&new_win_handle, &url);
                        NewWindowResponse::Deny
                    })
            };

            // Windows (WebView2):
            //  - MediaSessionService + HardwareMediaKeyHandling: tắt ô nhạc tự động của WebView (nó ghi "Ứng dụng không xác định"
            //    và tranh phím media); ô nhạc + phím media do app tự quản lý qua plugin dash-media (SMTC).
            //  - autoplay + *-throttling / backgrounding: nhạc vẫn chạy đúng khi cửa sổ bị ẩn xuống khay.
            //  (Giữ nguyên 3 mục msWebOOUI,msPdfOOUI,msSmartScreenProtection là mặc định của Tauri.)
            #[cfg(windows)]
            let window = window.additional_browser_args(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,MediaSessionService,HardwareMediaKeyHandling \
                 --autoplay-policy=no-user-gesture-required \
                 --disable-background-timer-throttling \
                 --disable-renderer-backgrounding \
                 --disable-backgrounding-occluded-windows",
            );

            // Android: tự đổi target="_blank" / window.open thành điều hướng thường để on_navigation ở trên bắt được.
            #[cfg(mobile)]
            let window = window.initialization_script(MOBILE_LINKS_JS);

            window.build()?;

            // Biểu tượng khay hệ thống (máy tính)
            #[cfg(desktop)]
            setup_tray(app.handle())?;

            #[cfg(desktop)]
            {
                if !cfg!(debug_assertions) {
                    check_for_updates(app.handle().clone(), false);
                    // App nằm ở khay chạy nền nhiều ngày nên "kiểm tra lúc mở app" không đủ: kiểm tra lại mỗi 4 giờ.
                    let handle = app.handle().clone();
                    std::thread::spawn(move || loop {
                        std::thread::sleep(std::time::Duration::from_secs(4 * 3600));
                        check_for_updates(handle.clone(), false);
                    });
                }
            }

            // Android: không có updater tự cài, nên chỉ báo có bản mới và dẫn tới trang tải.
            #[cfg(mobile)]
            check_for_updates_mobile(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Lỗi khi khởi chạy Tauri");
}
