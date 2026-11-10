use tauri::{WebviewUrl, WebviewWindowBuilder, webview::NewWindowResponse};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Cửa sổ chính: tải trang chờ (app-shell/index.html), trang đó sẽ tự chuyển
            // sang dashboard web khi máy chủ Render sẵn sàng.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Discord Bot Dashboard")
                .inner_size(1360.0, 860.0)
                .min_inner_size(900.0, 600.0)
                // Các liên kết mở thẻ mới (target="_blank", window.open) như "Mời Bot Vào Server",
                // "Mở trong Discord"... sẽ mở bằng trình duyệt mặc định thay vì bị chặn.
                .on_new_window(|url, _features| {
                    if matches!(url.scheme(), "http" | "https") {
                        open_in_browser(url.as_str());
                    }
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Lỗi khi khởi chạy Tauri");
}
