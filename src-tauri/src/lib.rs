use tauri::{WebviewUrl, WebviewWindowBuilder, webview::NewWindowResponse};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::UpdaterExt;

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

/// Mở một địa chỉ web bằng trình duyệt mặc định của hệ điều hành.
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

/// Kiểm tra bản mới khi mở app; nếu có thì hỏi người dùng rồi tải và cài.
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Discord Bot Dashboard")
                .inner_size(1360.0, 860.0)
                .min_inner_size(900.0, 600.0)
                .initialization_script(FULLSCREEN_JS)
                .on_new_window(|url, _features| {
                    if matches!(url.scheme(), "http" | "https") {
                        open_in_browser(url.as_str());
                    }
                    NewWindowResponse::Deny
                })
                .build()?;

            if !cfg!(debug_assertions) {
                check_for_updates(app.handle().clone());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Lỗi khi khởi chạy Tauri");
}
