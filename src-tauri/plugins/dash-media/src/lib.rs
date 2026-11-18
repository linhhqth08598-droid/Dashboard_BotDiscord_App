//! Cầu nối từ trang dashboard (web) tới điều khiển media của hệ điều hành.
//!
//! Trang gọi MỘT lệnh duy nhất: `plugin:dash-media|update` với trạng thái bài đang phát.
//!  - Windows: đưa vào SMTC (souvlaki) -> ô nhạc Windows, phím media, nút tai nghe, màn hình khóa.
//!  - Android: chuyển cho `MediaPlugin.kt` -> dịch vụ nền + thông báo nhạc + MediaSession.
//! Chiều ngược lại (bấm next / pause trên tai nghe...) native gọi `window.__dashMediaControl(action, value)` trong trang.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

#[cfg(windows)]
mod smtc;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaState {
    /// false = xóa ô điều khiển (hết nhạc / dừng hẳn)
    pub active: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: String,
    /// giây
    pub duration: f64,
    /// giây, tại thời điểm gửi
    pub position: f64,
}

#[cfg(target_os = "android")]
struct AndroidMedia<R: Runtime>(tauri::plugin::PluginHandle<R>);

#[cfg(target_os = "android")]
fn dispatch<R: Runtime>(app: &AppHandle<R>, state: MediaState) -> Result<(), String> {
    use tauri::Manager;
    app.state::<AndroidMedia<R>>()
        .0
        .run_mobile_plugin::<()>("update", state)
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn dispatch<R: Runtime>(app: &AppHandle<R>, state: MediaState) -> Result<(), String> {
    smtc::update(app, state)
}

#[cfg(not(any(target_os = "android", windows)))]
fn dispatch<R: Runtime>(_app: &AppHandle<R>, _state: MediaState) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn update<R: Runtime>(app: AppHandle<R>, state: MediaState) -> Result<(), String> {
    dispatch(&app, state)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("dash-media")
        .invoke_handler(tauri::generate_handler![update])
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("vn.io.linhhq.dashboardbot.media", "MediaPlugin")?;
                _app.manage(AndroidMedia(handle));
            }
            Ok(())
        })
        .build()
}
