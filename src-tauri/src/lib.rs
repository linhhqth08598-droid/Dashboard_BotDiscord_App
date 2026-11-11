use tauri::{WebviewUrl, WebviewWindowBuilder};
#[cfg(desktop)]
use tauri::webview::NewWindowResponse;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

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

/// Mở một địa chỉ web bằng trình duyệt mặc định của hệ điều hành (chỉ máy tính).
#[cfg(desktop)]
fn open_in_browser(url: &str) {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    let result = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();

    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();

    if let Err(e) = result {
        eprintln!("Không mở được trình duyệt: {e}");
    }
}

/// Kiểm tra bản mới khi mở app; nếu có thì hỏi người dùng rồi tải và cài (chỉ máy tính;
/// bản Android xem check_for_updates_mobile).
#[cfg(desktop)]
fn check_for_updates(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => {
                eprintln!("Không khởi tạo được updater: {e}");
                return;
            }
        };
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            Ok(None) => return,
            Err(e) => {
                eprintln!("Không kiểm tra được cập nhật: {e}");
                return;
            }
        };

        let agree = app
            .dialog()
            .message(format!(
                "Đã có phiên bản mới {} (bạn đang dùng {}). Cập nhật ngay?",
                update.version, update.current_version
            ))
            .title("Cập nhật ứng dụng")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Cập nhật".to_string(),
                "Để sau".to_string(),
            ))
            .blocking_show();
        if !agree {
            return;
        }

        if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
            eprintln!("Cập nhật thất bại: {e}");
            return;
        }
        app.restart();
    });
}

/// Địa chỉ file latest.json do GitHub Actions tạo ra mỗi lần phát hành bản mới.
#[cfg(mobile)]
const LATEST_JSON_URL: &str =
    "https://github.com/linhhqth08598-droid/Dashboard_BotDiscord_App/releases/latest/download/latest.json";

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
    Some((parts.next()??, parts.next().unwrap_or(Some(0))?, parts.next().unwrap_or(Some(0))?))
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

        let Some(latest_str) = json.get("version").and_then(|v| v.as_str()) else { return };
        let Some(latest) = parse_version(latest_str) else { return };
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
    // Hộp thoại + mở trang tải: dùng cho cả máy tính và Android.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    // Updater chỉ có trên máy tính (Windows/macOS/Linux).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|app| {
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()));

            // Kích thước cửa sổ, phím F11 và chặn cửa sổ mới: chỉ áp dụng trên máy tính.
            #[cfg(desktop)]
            let window = window
                .title("Discord Bot Dashboard")
                .inner_size(1360.0, 860.0)
                .min_inner_size(900.0, 600.0)
                .initialization_script(FULLSCREEN_JS)
                .on_new_window(|url, _features| {
                    if matches!(url.scheme(), "http" | "https") {
                        open_in_browser(url.as_str());
                    }
                    NewWindowResponse::Deny
                });

            window.build()?;

            #[cfg(desktop)]
            {
                if !cfg!(debug_assertions) {
                    check_for_updates(app.handle().clone());
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
