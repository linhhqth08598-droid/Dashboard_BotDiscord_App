package vn.io.linhhq.dashboardbot.media

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.webkit.CookieManager
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

/**
 * Bộ phát nhạc NATIVE chạy trong dịch vụ nền, dùng khi người dùng vuốt đóng app.
 *
 * Bình thường nhạc phát trong WebView (trang web). Vuốt đóng app thì WebView bị hủy kéo theo nhạc tắt. Lúc đó dịch vụ nền
 * (vẫn sống nhờ thông báo nhạc) tiếp quản: tự phát đúng bài, đúng giây theo "phòng nhạc" trên server (cùng nguyên tắc đồng bộ
 * với trang web: server giữ MỐC (position, anchorAt, paused), thiết bị chỉ làm cho tiếng khớp mốc), dùng chung phiên đăng nhập
 * (cookie của WebView). Nút trên thông báo / tai nghe gọi thẳng API của server. Mở lại app và trang web phát tiếng lại thì
 * dịch vụ nhả quyền phát về cho trang (xem MediaPlaybackService.applyState).
 *
 * Mọi thao tác với MediaPlayer chạy trên luồng chính; gọi mạng chạy ở luồng riêng rồi đẩy kết quả về luồng chính.
 */
class NativeEngine(
    private val ctx: Context,
    private val baseUrl: String,
    private val guildId: String,
    private val artwork: String,
    private val listener: Listener
) {
    interface Listener {
        /** Có thay đổi cần cập nhật thông báo (đổi bài / phát / dừng / tua). Gọi trên luồng chính. */
        fun onTrack(args: MediaArgs)

        /** Hết nhạc (server dừng / hết hàng đợi / mất phiên đăng nhập): dịch vụ nên tắt. Gọi trên luồng chính. */
        fun onFinished()
    }

    private class HttpResult(val code: Int, val body: String)

    private val main = Handler(Looper.getMainLooper())
    private val poll = Executors.newSingleThreadExecutor()
    private val cmdIo = Executors.newSingleThreadExecutor()
    private val audioManager = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    @Volatile
    private var running = false

    // Đồng hồ: (giờ server) - (giờ máy), lấy lần đo có độ trễ mạng nhỏ nhất trong vài lần gần nhất
    private val clockSamples = ArrayList<Pair<Long, Long>>() // (rtt, offset)
    @Volatile
    private var offsetMs = 0L

    // Mốc của server
    private var rev = -1L
    private var paused = true
    private var position = 0.0
    private var anchorAt = 0L
    private var duration = 0.0
    private var volume = 0.8f
    private var title = ""
    private var artist = ""

    // Trình phát
    private var mp: MediaPlayer? = null
    private var loadedId = ""
    private var prepared = false
    private var completed = false
    private var completedAt = 0L
    private var errStreak = 0
    private var lastSeekAt = 0L
    private var focusLost = false
    private var focusReq: AudioFocusRequest? = null

    // Lần cuối báo cho thông báo
    private var sentKey = ""
    private var sentPos = 0.0
    private var sentAt = 0L
    private var sentPlaying = false

    private val attrs: AudioAttributes = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
        .build()

    private val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
        main.post {
            if (!running) return@post
            when (change) {
                AudioManager.AUDIOFOCUS_LOSS -> {
                    // App khác (YouTube, Spotify...) chiếm hẳn tiếng: tạm dừng phòng nhạc luôn
                    focusLost = false
                    command("pause", 0.0)
                }
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                    focusLost = true
                    reconcile()
                }
                AudioManager.AUDIOFOCUS_GAIN -> {
                    if (focusLost) {
                        focusLost = false
                        reconcile()
                    }
                }
            }
        }
    }

    /* ---------------- Vòng đời ---------------- */

    fun start() {
        if (running) return
        running = true
        poll.execute {
            var fails = 0
            while (running) {
                try {
                    val t0 = System.currentTimeMillis()
                    val r = http("GET", api("/state"), null)
                    val t1 = System.currentTimeMillis()
                    if (r.code == 401 || r.code == 403) {
                        main.post { finish() }
                        return@execute
                    }
                    if (r.code in 200..299) {
                        val st = JSONObject(r.body).optJSONObject("state")
                        if (st != null) {
                            val sn = st.optLong("serverNow", 0L)
                            if (sn > 0) sampleClock(t0, t1, sn)
                            main.post { if (running) applyState(st) }
                        }
                        fails = 0
                    } else {
                        fails++
                    }
                } catch (_: Throwable) {
                    fails++
                }
                if (!running) break
                try {
                    Thread.sleep(if (fails > 0) 4000L else 2000L)
                } catch (_: InterruptedException) {
                    break
                }
            }
        }
        main.postDelayed(tick, 1000)
    }

    /** Dừng hẳn bộ phát (không báo cho dịch vụ). Gọi lại nhiều lần cũng được. */
    fun stop() {
        if (!running) return
        running = false
        main.removeCallbacksAndMessages(null)
        try { poll.shutdownNow() } catch (_: Throwable) { }
        try { cmdIo.shutdown() } catch (_: Throwable) { }
        releasePlayer()
        abandonFocus()
    }

    private fun finish() {
        if (!running) return
        stop()
        listener.onFinished()
    }

    private val tick = object : Runnable {
        override fun run() {
            if (!running) return
            reconcile()
            // Bài đã hết mà lệnh "next" chưa tới được server (mất mạng...): thử lại
            if (completed && SystemClock.elapsedRealtime() - completedAt > 5000) {
                completedAt = SystemClock.elapsedRealtime()
                send("/next", JSONObject().put("expectId", loadedId).toString())
            }
            // Máy khác vừa tua: báo lại cho thanh tiến trình trên thông báo
            if (loadedId.isNotEmpty()) {
                val now = SystemClock.elapsedRealtime()
                val shown = sentPos + if (sentPlaying) (now - sentAt) / 1000.0 else 0.0
                if (Math.abs(expected() - shown) > 1.5) publish(true)
            }
            main.postDelayed(this, 1000)
        }
    }

    /* ---------------- Mạng ---------------- */

    private fun api(path: String) = "$baseUrl/api/guilds/$guildId/webmusic$path"

    /** Cookie đăng nhập đang dùng trong WebView (cùng tiến trình nên dùng chung được). */
    private fun cookie(): String = try {
        CookieManager.getInstance().getCookie(baseUrl) ?: ""
    } catch (_: Throwable) {
        ""
    }

    private fun http(method: String, url: String, body: String?): HttpResult {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = method
            c.connectTimeout = 8000
            c.readTimeout = 15000
            c.setRequestProperty("Cookie", cookie())
            c.setRequestProperty("Cache-Control", "no-store")
            if (body != null) {
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            }
            val code = c.responseCode
            val stream = if (code in 200..299) c.inputStream else c.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() } ?: ""
            return HttpResult(code, text)
        } finally {
            c.disconnect()
        }
    }

    /** Gửi lệnh lên server rồi áp trạng thái mới trả về. */
    private fun send(path: String, body: String) {
        try {
            cmdIo.execute {
                try {
                    val r = http("POST", api(path), body)
                    if (r.code in 200..299) {
                        val st = JSONObject(r.body).optJSONObject("state")
                        if (st != null) main.post { if (running) applyState(st) }
                    }
                } catch (_: Throwable) {
                }
            }
        } catch (_: Throwable) {
            // bộ phát vừa dừng
        }
    }

    private fun sampleClock(t0: Long, t1: Long, serverNow: Long) {
        val rtt = Math.max(0L, t1 - t0)
        val offset = serverNow - (t0 + rtt / 2)
        synchronized(clockSamples) {
            clockSamples.add(Pair(rtt, offset))
            while (clockSamples.size > 8) clockSamples.removeAt(0)
            offsetMs = clockSamples.minByOrNull { it.first }?.second ?: offset
        }
    }

    /* ---------------- Áp trạng thái của server ---------------- */

    private fun applyState(st: JSONObject) {
        val r = st.optLong("rev", 0L)
        if (r < rev) return // bản cũ
        rev = r
        val cur = st.optJSONObject("current")
        if (cur == null) {
            finish()
            return
        }
        val id = cur.optString("id")
        paused = st.optBoolean("paused", true)
        position = st.optDouble("position", 0.0)
        anchorAt = st.optLong("anchorAt", 0L)
        volume = st.optDouble("volume", 0.8).toFloat().coerceIn(0f, 1f)
        duration = cur.optDouble("duration", 0.0)
        title = cur.optString("title")
        artist = cur.optString("author")
        try { mp?.setVolume(volume, volume) } catch (_: Throwable) { }

        if (id != loadedId) load(id) else reconcile()
        publish(false)
    }

    /** Giây lẽ ra phải đang phát (theo mốc server + giờ server ước tính). */
    private fun expected(): Double {
        var pos = position
        if (!paused) {
            val now = System.currentTimeMillis() + offsetMs
            if (now > anchorAt) pos += (now - anchorAt) / 1000.0
        }
        if (duration > 0) pos = Math.min(pos, duration)
        return Math.max(0.0, pos)
    }

    /* ---------------- Trình phát ---------------- */

    private fun load(id: String) {
        releasePlayer()
        loadedId = id
        prepared = false
        completed = false
        val p = MediaPlayer()
        mp = p
        try {
            p.setAudioAttributes(attrs)
            p.setWakeMode(ctx, PowerManager.PARTIAL_WAKE_LOCK)
            p.setVolume(volume, volume)
            p.setDataSource(ctx, Uri.parse(api("/stream/" + Uri.encode(id))), mapOf("Cookie" to cookie()))
            p.setOnPreparedListener {
                if (mp === p) {
                    prepared = true
                    errStreak = 0
                    reconcile()
                    publish(true)
                }
            }
            p.setOnCompletionListener { if (mp === p) onCompleted() }
            p.setOnErrorListener { _, _, _ ->
                if (mp === p) onPlayerError()
                true
            }
            p.prepareAsync()
        } catch (_: Throwable) {
            onPlayerError()
        }
    }

    private fun releasePlayer() {
        val p = mp ?: return
        mp = null
        prepared = false
        try { p.setOnPreparedListener(null) } catch (_: Throwable) { }
        try { p.setOnCompletionListener(null) } catch (_: Throwable) { }
        try { p.setOnErrorListener(null) } catch (_: Throwable) { }
        try { p.reset() } catch (_: Throwable) { }
        try { p.release() } catch (_: Throwable) { }
    }

    /** So tiếng đang phát với mốc của server rồi sửa cho khớp (nhảy khi lệch rõ, không đổi tốc độ nên không giật). */
    private fun reconcile() {
        val p = mp ?: return
        if (!prepared || completed) return
        try {
            val exp = expected()
            val cur = p.currentPosition / 1000.0
            if (paused || focusLost) {
                if (p.isPlaying) p.pause()
                if (paused && Math.abs(cur - exp) > 0.5) seek(p, exp)
                return
            }
            if (!p.isPlaying) {
                requestFocus()
                if (Math.abs(cur - exp) > 0.3) seek(p, exp)
                p.start()
                return
            }
            if (Math.abs(cur - exp) > 1.0 && SystemClock.elapsedRealtime() - lastSeekAt > 4000) seek(p, exp)
        } catch (_: IllegalStateException) {
            // trình phát đang chuyển trạng thái: lần tick sau làm lại
        }
    }

    private fun seek(p: MediaPlayer, sec: Double) {
        lastSeekAt = SystemClock.elapsedRealtime()
        p.seekTo((sec * 1000).toInt())
    }

    private fun onCompleted() {
        completed = true
        completedAt = SystemClock.elapsedRealtime()
        // Hết bài: báo server chuyển bài (nhiều thiết bị cùng báo thì server chỉ nhận lần đầu nhờ expectId)
        send("/next", JSONObject().put("expectId", loadedId).toString())
    }

    private fun onPlayerError() {
        val id = loadedId
        errStreak++
        if (errStreak == 1) {
            // Lần đầu: có thể mạng chớp, nạp lại đúng bài đó
            main.postDelayed({ if (running && loadedId == id) load(id) }, 800)
        } else {
            errStreak = 0
            send("/next", JSONObject().put("expectId", id).toString())
        }
    }

    /* ---------------- Âm thanh của hệ thống ---------------- */

    private fun requestFocus() {
        try {
            if (Build.VERSION.SDK_INT >= 26) {
                val r = focusReq ?: AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(attrs)
                    .setOnAudioFocusChangeListener(focusListener)
                    .build()
                    .also { focusReq = it }
                audioManager.requestAudioFocus(r)
            } else {
                @Suppress("DEPRECATION")
                audioManager.requestAudioFocus(focusListener, AudioManager.STREAM_MUSIC, AudioManager.AUDIOFOCUS_GAIN)
            }
        } catch (_: Throwable) {
        }
    }

    private fun abandonFocus() {
        try {
            if (Build.VERSION.SDK_INT >= 26) {
                focusReq?.let { audioManager.abandonAudioFocusRequest(it) }
            } else {
                @Suppress("DEPRECATION")
                audioManager.abandonAudioFocus(focusListener)
            }
        } catch (_: Throwable) {
        }
    }

    /* ---------------- Thông báo ---------------- */

    private fun publish(force: Boolean) {
        if (loadedId.isEmpty()) return
        val key = "$loadedId|$paused|${duration.toInt()}"
        if (!force && key == sentKey) return
        sentKey = key
        val pos = expected()
        sentPos = pos
        sentAt = SystemClock.elapsedRealtime()
        sentPlaying = !paused
        val a = MediaArgs()
        a.active = true
        a.playing = !paused
        a.title = title
        a.artist = artist
        a.album = "Dashboard Bot"
        a.artwork = artwork
        a.duration = duration
        a.position = pos
        a.guildId = guildId
        a.baseUrl = baseUrl
        listener.onTrack(a)
    }

    /* ---------------- Lệnh từ thông báo / tai nghe ---------------- */

    /** Cùng tên lệnh với trang web (`window.__dashMediaControl`): play, pause, toggle, next, prev, stop, seekto, seekby. */
    fun command(action: String, value: Double) {
        if (!running) return
        val id = loadedId
        fun control(act: String, extra: JSONObject.() -> Unit = {}) {
            send("/control", JSONObject().put("action", act).put("itemId", id).apply(extra).toString())
        }
        when (action) {
            "play" -> control("resume")
            "pause" -> control("pause")
            "toggle" -> control(if (paused) "resume" else "pause")
            "next" -> send("/next", "{}")
            "prev" -> send("/prev", "{}")
            "seekto" -> control("seek") { put("position", Math.max(0.0, value)) }
            "seekby" -> control("seek") { put("position", Math.max(0.0, expected() + value)) }
            "stop" -> {
                try {
                    cmdIo.execute {
                        try { http("POST", api("/stop"), "{}") } catch (_: Throwable) { }
                        main.post { finish() }
                    }
                } catch (_: Throwable) {
                    finish()
                }
            }
        }
    }
}
