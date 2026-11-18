//! Windows: System Media Transport Controls qua `souvlaki`.
//! MediaControls chỉ được dùng trên luồng giao diện (main thread), nên giữ trong thread_local và mọi lệnh đều chạy qua run_on_main_thread.

use std::{cell::RefCell, ffi::c_void, time::Duration};

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::MediaState;

thread_local! {
    static CONTROLS: RefCell<Option<MediaControls>> = const { RefCell::new(None) };
    /// (tiêu đề, nghệ sĩ, album, ảnh bìa, độ dài ms) của bài đang hiện: không đổi thì khỏi đặt lại (đỡ tải lại ảnh bìa).
    static LAST_META: RefCell<Option<(String, String, String, String, u64)>> = const { RefCell::new(None) };
}

fn secs(v: f64) -> Duration {
    if v.is_finite() && v > 0.0 {
        Duration::from_secs_f64(v.min(1.0e7))
    } else {
        Duration::ZERO
    }
}

pub fn update<R: Runtime>(app: &AppHandle<R>, state: MediaState) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        CONTROLS.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                if !state.active {
                    return; // chưa từng phát gì: không cần tạo ô điều khiển
                }
                match create(&handle) {
                    Ok(c) => *slot = Some(c),
                    Err(e) => {
                        eprintln!("[dash-media] Không tạo được SMTC: {e}");
                        return;
                    }
                }
            }
            if let Some(controls) = slot.as_mut() {
                apply(controls, &state);
            }
        });
    })
    .map_err(|e| e.to_string())
}

fn create<R: Runtime>(app: &AppHandle<R>) -> Result<MediaControls, String> {
    let win = app.get_webview_window("main").ok_or("không thấy cửa sổ main")?;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let mut controls = MediaControls::new(PlatformConfig {
        dbus_name: "vn.io.linhhq.dashboardbot",
        display_name: "Discord Bot Dashboard",
        hwnd: Some(hwnd.0 as *mut c_void),
    })
    .map_err(|e| format!("{e:?}"))?;

    let h = app.clone();
    controls
        .attach(move |event| on_event(&h, event))
        .map_err(|e| format!("{e:?}"))?;
    Ok(controls)
}

fn apply(controls: &mut MediaControls, s: &MediaState) {
    if !s.active {
        let _ = controls.set_playback(MediaPlayback::Stopped);
        let _ = controls.set_metadata(MediaMetadata::default());
        LAST_META.with(|m| *m.borrow_mut() = None);
        return;
    }

    let meta_key = (
        s.title.clone(),
        s.artist.clone(),
        s.album.clone(),
        s.artwork.clone(),
        (secs(s.duration).as_millis()) as u64,
    );
    let changed = LAST_META.with(|m| m.borrow().as_ref() != Some(&meta_key));
    if changed {
        let _ = controls.set_metadata(MediaMetadata {
            title: Some(&s.title),
            artist: Some(&s.artist),
            album: Some(&s.album),
            cover_url: if s.artwork.is_empty() { None } else { Some(&s.artwork) },
            duration: if s.duration > 0.0 { Some(secs(s.duration)) } else { None },
        });
        LAST_META.with(|m| *m.borrow_mut() = Some(meta_key));
    }

    let progress = Some(MediaPosition(secs(s.position)));
    let _ = controls.set_playback(if s.playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    });
}

/// Phím media / nút tai nghe / ô nhạc Windows bấm -> báo cho trang web.
fn on_event<R: Runtime>(app: &AppHandle<R>, event: MediaControlEvent) {
    let sign = |d: &SeekDirection| if matches!(d, SeekDirection::Forward) { 1.0 } else { -1.0 };
    let (action, value): (&str, f64) = match event {
        MediaControlEvent::Play => ("play", 0.0),
        MediaControlEvent::Pause => ("pause", 0.0),
        MediaControlEvent::Toggle => ("toggle", 0.0),
        MediaControlEvent::Next => ("next", 0.0),
        MediaControlEvent::Previous => ("prev", 0.0),
        MediaControlEvent::Stop => ("stop", 0.0),
        MediaControlEvent::SetPosition(MediaPosition(d)) => ("seekto", d.as_secs_f64()),
        MediaControlEvent::SeekBy(dir, d) => ("seekby", sign(&dir) * d.as_secs_f64()),
        MediaControlEvent::Seek(dir) => ("seekby", sign(&dir) * 10.0),
        _ => return,
    };
    if let Some(win) = app.get_webview_window("main") {
        let js = format!(
            "window.__dashMediaControl&&window.__dashMediaControl({},{})",
            serde_json_string(action),
            value
        );
        let _ = win.eval(js);
    }
}

/// Chuỗi JS an toàn (action chỉ là chữ thường nên đơn giản; vẫn escape cho chắc).
fn serde_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
