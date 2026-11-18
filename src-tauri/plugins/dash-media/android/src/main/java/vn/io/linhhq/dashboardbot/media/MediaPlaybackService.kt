package vn.io.linhhq.dashboardbot.media

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.drawable.Icon
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import java.net.HttpURLConnection
import java.net.URL

/**
 * Dịch vụ nền giữ nhạc "sống" khi tắt màn hình / chuyển app, đồng thời:
 *  - hiện thông báo nhạc kiểu Spotify (tên bài, ảnh, thanh tua, prev / pause / next),
 *  - giữ MediaSession => nút tai nghe, màn hình khóa, Liên kết điện thoại (Phone Link) điều khiển được.
 * Nhạc vẫn phát trong WebView của app; dịch vụ chỉ lo phần hiển thị + giữ tiến trình sống + chuyển lệnh về trang web.
 */
class MediaPlaybackService : Service() {

    companion object {
        @Volatile
        var instance: MediaPlaybackService? = null

        @Volatile
        var pending: MediaArgs? = null

        private const val CHANNEL_ID = "dash_music"
        private const val NOTIF_ID = 4711
        private const val ACT_PREV = "vn.io.linhhq.dashboardbot.media.PREV"
        private const val ACT_NEXT = "vn.io.linhhq.dashboardbot.media.NEXT"
        private const val ACT_TOGGLE = "vn.io.linhhq.dashboardbot.media.TOGGLE"
        private const val ACT_CLOSE = "vn.io.linhhq.dashboardbot.media.CLOSE"
    }

    private val main = Handler(Looper.getMainLooper())
    private lateinit var session: MediaSession
    private var wakeLock: PowerManager.WakeLock? = null
    private var state = MediaArgs()
    private var art: Bitmap? = null
    private var artUrl = ""
    private var foreground = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        createChannel()
        session = MediaSession(this, "DashboardMusic").apply {
            @Suppress("DEPRECATION")
            setFlags(MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS)
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() = MediaBridge.sendToJs("play")
                override fun onPause() = MediaBridge.sendToJs("pause")
                override fun onStop() = MediaBridge.sendToJs("stop")
                override fun onSkipToNext() = MediaBridge.sendToJs("next")
                override fun onSkipToPrevious() = MediaBridge.sendToJs("prev")
                override fun onSeekTo(pos: Long) = MediaBridge.sendToJs("seekto", pos / 1000.0)
                override fun onFastForward() = MediaBridge.sendToJs("seekby", 10.0)
                override fun onRewind() = MediaBridge.sendToJs("seekby", -10.0)
            })
            isActive = true
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACT_PREV -> MediaBridge.sendToJs("prev")
            ACT_NEXT -> MediaBridge.sendToJs("next")
            ACT_TOGGLE -> MediaBridge.sendToJs(if (state.playing) "pause" else "play")
            ACT_CLOSE -> MediaBridge.sendToJs("stop")
            else -> pending?.let {
                pending = null
                applyNow(it)
            }
        }
        // startForegroundService() bắt buộc phải gọi startForeground() trong ~5 giây, kể cả khi chưa có gì để hiện
        main.post { if (!foreground) postNotification() }
        return START_NOT_STICKY
    }

    /** Gọi từ plugin mỗi khi trang gửi trạng thái mới (đổi bài / phát / dừng / tua). */
    fun applyState(s: MediaArgs) {
        main.post { applyNow(s) }
    }

    private fun applyNow(s: MediaArgs) {
        if (!s.active) {
            shutdown()
            return
        }
        state = s
        if (s.artwork.isNotEmpty() && s.artwork != artUrl) loadArt(s.artwork)
        updateSession()
        updateWakeLock()
        postNotification()
    }

    private fun updateSession() {
        val md = MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_TITLE, state.title)
            .putString(MediaMetadata.METADATA_KEY_ARTIST, state.artist)
            .putString(MediaMetadata.METADATA_KEY_ALBUM, state.album)
        if (state.duration > 0) md.putLong(MediaMetadata.METADATA_KEY_DURATION, (state.duration * 1000).toLong())
        art?.let {
            md.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, it)
            md.putBitmap(MediaMetadata.METADATA_KEY_ART, it)
        }
        session.setMetadata(md.build())

        val actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or
            PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS or PlaybackState.ACTION_SEEK_TO or
            PlaybackState.ACTION_STOP or PlaybackState.ACTION_FAST_FORWARD or PlaybackState.ACTION_REWIND
        val pb = PlaybackState.Builder()
            .setActions(actions)
            .setState(
                if (state.playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED,
                (state.position * 1000).toLong(),
                if (state.playing) 1f else 0f,
                SystemClock.elapsedRealtime()
            )
            .build()
        session.setPlaybackState(pb)
    }

    private fun pi(action: String, code: Int): PendingIntent =
        PendingIntent.getService(
            this, code,
            Intent(this, MediaPlaybackService::class.java).setAction(action),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

    private fun action(iconRes: Int, label: String, act: String, code: Int): Notification.Action =
        Notification.Action.Builder(Icon.createWithResource(this, iconRes), label, pi(act, code)).build()

    private fun buildNotification(): Notification {
        val b = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(this, CHANNEL_ID) else Notification.Builder(this)
        b.setSmallIcon(R.drawable.ic_stat_music)
            .setContentTitle(if (state.title.isNotEmpty()) state.title else "Dashboard Bot")
            .setContentText(state.artist)
            .setSubText(state.album)
            .setShowWhen(false)
            .setOnlyAlertOnce(true)
            .setOngoing(state.playing)
            .setCategory(Notification.CATEGORY_TRANSPORT)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(action(android.R.drawable.ic_media_previous, "Bài trước", ACT_PREV, 1))
            .addAction(
                if (state.playing) action(android.R.drawable.ic_media_pause, "Tạm dừng", ACT_TOGGLE, 2)
                else action(android.R.drawable.ic_media_play, "Phát", ACT_TOGGLE, 2)
            )
            .addAction(action(android.R.drawable.ic_media_next, "Bài sau", ACT_NEXT, 3))
            .addAction(action(android.R.drawable.ic_menu_close_clear_cancel, "Dừng", ACT_CLOSE, 4))
            .setStyle(
                Notification.MediaStyle()
                    .setMediaSession(session.sessionToken)
                    .setShowActionsInCompactView(0, 1, 2)
            )
        art?.let { b.setLargeIcon(it) }
        packageManager.getLaunchIntentForPackage(packageName)?.let {
            b.setContentIntent(PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
        }
        return b.build()
    }

    private fun postNotification() {
        val n = buildNotification()
        if (!foreground) {
            try {
                if (Build.VERSION.SDK_INT >= 29) {
                    startForeground(NOTIF_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
                } else {
                    startForeground(NOTIF_ID, n)
                }
                foreground = true
            } catch (_: Throwable) {
                // Android 12+ có thể từ chối bật dịch vụ nền khi app đang ở nền: thôi, nhạc vẫn phát trong app
                stopSelf()
            }
        } else {
            getSystemService(NotificationManager::class.java).notify(NOTIF_ID, n)
        }
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < 26) return
        val ch = NotificationChannel(CHANNEL_ID, "Phát nhạc", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Điều khiển nhạc đang phát (tạm dừng, chuyển bài, tua)"
            setShowBadge(false)
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        }
        getSystemService(NotificationManager::class.java).createNotificationChannel(ch)
    }

    /** Ảnh bìa: tải 1 lần rồi nhớ lại; lỗi thì thôi (thông báo vẫn hiện, chỉ không có ảnh). */
    private fun loadArt(url: String) {
        artUrl = url
        Thread {
            try {
                val conn = URL(url).openConnection() as HttpURLConnection
                conn.connectTimeout = 8000
                conn.readTimeout = 8000
                val bmp = conn.inputStream.use { BitmapFactory.decodeStream(it) } ?: return@Thread
                val scaled = Bitmap.createScaledBitmap(bmp, 256, 256, true)
                main.post {
                    if (artUrl == url) {
                        art = scaled
                        updateSession()
                        postNotification()
                    }
                }
            } catch (_: Throwable) {
                main.post { if (artUrl == url) artUrl = "" }
            }
        }.start()
    }

    /** Giữ CPU thức khi tắt màn hình để nhạc stream không bị ngắt; nhả ngay khi tạm dừng. */
    private fun updateWakeLock() {
        if (state.playing) {
            val wl = wakeLock ?: (getSystemService(POWER_SERVICE) as PowerManager)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "dashboardbot:music")
                .also { it.setReferenceCounted(false); wakeLock = it }
            wl.acquire(6 * 60 * 60 * 1000L)
        } else {
            releaseWakeLock()
        }
    }

    private fun releaseWakeLock() {
        try {
            wakeLock?.takeIf { it.isHeld }?.release()
        } catch (_: Throwable) {
        }
    }

    private fun shutdown() {
        releaseWakeLock()
        try {
            session.isActive = false
        } catch (_: Throwable) {
        }
        if (foreground) {
            if (Build.VERSION.SDK_INT >= 24) stopForeground(STOP_FOREGROUND_REMOVE) else @Suppress("DEPRECATION") stopForeground(true)
            foreground = false
        }
        stopSelf()
    }

    /** Vuốt app khỏi danh sách gần đây: WebView (nơi phát nhạc) chết theo, nên tắt luôn thông báo cho khỏi treo. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        MediaBridge.webView = null
        shutdown()
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        releaseWakeLock()
        try {
            session.release()
        } catch (_: Throwable) {
        }
        instance = null
        super.onDestroy()
    }
}
