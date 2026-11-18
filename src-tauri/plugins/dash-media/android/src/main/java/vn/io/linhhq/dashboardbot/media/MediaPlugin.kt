package vn.io.linhhq.dashboardbot.media

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.webkit.WebView
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/** Trạng thái bài đang phát do trang dashboard gửi xuống (khớp `MediaState` bên Rust / `NativeMediaState` bên web). */
@InvokeArg
class MediaArgs {
    var active: Boolean = false
    var playing: Boolean = false
    var title: String = ""
    var artist: String = ""
    var album: String = ""
    var artwork: String = ""
    var duration: Double = 0.0
    var position: Double = 0.0
}

/** Chiều ngược lại: nút tai nghe / thông báo -> gọi hàm JS của trang. */
object MediaBridge {
    @Volatile
    var webView: WebView? = null

    fun sendToJs(action: String, value: Double = 0.0) {
        val wv = webView ?: return
        val js = "window.__dashMediaControl&&window.__dashMediaControl(\"$action\",$value)"
        wv.post {
            try {
                wv.evaluateJavascript(js, null)
            } catch (_: Throwable) {
            }
        }
    }
}

@TauriPlugin
class MediaPlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: WebView) {
        super.load(webView)
        MediaBridge.webView = webView
        activity.runOnUiThread {
            try {
                // Cho phép play() khi trang ở nền (tai nghe bấm "phát", server đẩy bài mới...) mà không cần chạm màn hình.
                webView.settings.mediaPlaybackRequiresUserGesture = false
            } catch (_: Throwable) {
            }
            askNotificationPermission()
        }
    }

    /** Android 13+: không có quyền thông báo thì thông báo nhạc không hiện. */
    private fun askNotificationPermission() {
        if (Build.VERSION.SDK_INT < 33) return
        if (ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        try {
            activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 7001)
        } catch (_: Throwable) {
        }
    }

    @Command
    fun update(invoke: Invoke) {
        val args = invoke.parseArgs(MediaArgs::class.java)
        val service = MediaPlaybackService.instance
        if (service != null) {
            service.applyState(args)
        } else if (args.active) {
            // Chưa có dịch vụ nền: lưu trạng thái rồi bật dịch vụ (dịch vụ đọc `pending` khi khởi động)
            MediaPlaybackService.pending = args
            try {
                ContextCompat.startForegroundService(activity, Intent(activity, MediaPlaybackService::class.java))
            } catch (_: Throwable) {
                MediaPlaybackService.pending = null
            }
        }
        invoke.resolve()
    }
}
