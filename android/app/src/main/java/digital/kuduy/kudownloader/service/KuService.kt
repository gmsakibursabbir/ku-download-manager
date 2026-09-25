package digital.kuduy.kudownloader.service

import android.annotation.SuppressLint
import android.app.Notification
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import digital.kuduy.kudownloader.MainActivity
import digital.kuduy.kudownloader.R
import digital.kuduy.kudownloader.core.AirMessage
import digital.kuduy.kudownloader.core.AirRequest
import digital.kuduy.kudownloader.core.AirTransfer
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonPrimitive

/**
 * Foreground service: runs while something downloads, a queue waits, or
 * KuAirSend is on. Holds the wake, Wi-Fi and multicast locks those need and
 * shows live progress in the notification shade.
 */
class KuService : LifecycleService() {
    private var wake: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null
    private var multicast: WifiManager.MulticastLock? = null

    override fun onCreate() {
        super.onCreate()
        running = true
        startInForeground(build())
        lifecycleScope.launch {
            combine(Ku.downloads, Ku.airStatus) { d, a -> d to a }.collect { (map, air) ->
                val running = map.values.any { it.isRunning }
                val pending = map.values.any { it.status == "queued" }
                locks(running, air.running)
                if (!running && !pending && !air.running) {
                    stop()
                    return@collect
                }
                notifyProgress()
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        return START_STICKY
    }

    private fun startInForeground(n: Notification) {
        try {
            ServiceCompat.startForeground(
                this,
                Notifier.ID_SERVICE,
                n,
                if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0,
            )
        } catch (e: Exception) {
            Log.w("KuService", "foreground", e)
            stopSelf()
        }
    }

    private var lastPost = 0L

    private fun notifyProgress() {
        val now = System.currentTimeMillis()
        if (now - lastPost < 900) return
        lastPost = now
        if (!Notifier.allowed(this)) return
        runCatching { androidx.core.app.NotificationManagerCompat.from(this).notify(Notifier.ID_SERVICE, build()) }
    }

    private fun build(): Notification {
        val all = Ku.downloads.value.values
        val running = all.filter { it.isRunning }
        val queued = all.count { it.status == "queued" }
        val air = Ku.airStatus.value.running
        val b = NotificationCompat.Builder(this, Notifier.CH_PROGRESS)
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .setContentIntent(Notifier.openApp(this))
        if (running.isNotEmpty()) {
            val done = running.sumOf { it.done }
            val total = running.sumOf { it.total }
            val speed = running.sumOf { it.speed }
            val eta = if (speed > 0 && total > done) (total - done) / speed else null
            val title = if (running.size == 1) running[0].name.ifBlank { t("Downloading") } else tf("Downloading {count} files", "count" to running.size)
            val parts = mutableListOf(Fmt.speed(speed))
            if (total > 0) parts += "${Fmt.size(done)} / ${Fmt.size(total)}"
            eta?.let { parts += tf("{time} left", "time" to Fmt.duration(it)) }
            if (queued > 0) parts += tf("{count} waiting", "count" to queued)
            b.setContentTitle(title).setContentText(parts.joinToString(" · "))
            if (total > 0) b.setProgress(1000, ((done * 1000) / total).toInt(), false) else b.setProgress(0, 0, true)
            b.addAction(0, t("Pause all"), Notifier.broadcast(this, ActionReceiver.PAUSE_ALL))
        } else if (queued > 0) {
            b.setContentTitle(tf("{count} waiting", "count" to queued)).setContentText(t("Downloads start when their queue runs."))
            b.addAction(0, t("Start all"), Notifier.broadcast(this, ActionReceiver.RESUME_ALL))
        } else {
            b.setContentTitle(t("KuAirSend is on")).setContentText(t("Nearby KuDownloader devices can find this phone."))
        }
        if (air) b.addAction(0, t("Turn off KuAirSend"), Notifier.broadcast(this, ActionReceiver.AIR_OFF))
        return b.build()
    }

    @SuppressLint("WakelockTimeout")
    private fun locks(downloading: Boolean, air: Boolean) {
        val pm = getSystemService(PowerManager::class.java)
        val wm = applicationContext.getSystemService(WifiManager::class.java)
        if (downloading && wake == null) {
            wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "KuDownloader:downloads").apply { setReferenceCounted(false); acquire() }
        } else if (!downloading && !air) {
            wake?.takeIf { it.isHeld }?.release()
            wake = null
        }
        val wantWifi = (downloading || air) && Prefs.highPerfWifi.value
        if (wantWifi && wifi == null) {
            @Suppress("DEPRECATION")
            val mode = if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
            wifi = wm.createWifiLock(mode, "KuDownloader:wifi").apply { setReferenceCounted(false); acquire() }
        } else if (!wantWifi) {
            wifi?.takeIf { it.isHeld }?.release()
            wifi = null
        }
        // Multicast packets (KuAirSend discovery) are filtered out without this.
        if (air && multicast == null) {
            multicast = wm.createMulticastLock("KuDownloader:airsend").apply { setReferenceCounted(false); acquire() }
        } else if (!air) {
            multicast?.takeIf { it.isHeld }?.release()
            multicast = null
        }
    }

    private fun stop() {
        locks(false, false)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        locks(false, false)
        running = false
        super.onDestroy()
    }

    companion object {
        @Volatile var running = false
            private set

        /** Start the service if there is work for it (safe to call often). */
        fun ensure(ctx: Context) {
            if (running) return
            val work = Ku.downloads.value.values.any { it.isRunning || it.status == "queued" } || Ku.airStatus.value.running
            if (!work) return
            try {
                ContextCompat.startForegroundService(ctx, Intent(ctx, KuService::class.java))
                running = true
            } catch (e: Exception) {
                // Android 12+ refuses to start it from the background; the
                // next time the app or a notification action runs, it will.
                Log.w("KuService", "start refused", e)
            }
        }

        /** App-wide: start the service when work appears, post notices for events. */
        fun watch(ctx: Context) {
            val app = ctx.applicationContext
            Ku.launch {
                combine(Ku.downloads.map { m -> m.values.any { it.isRunning || it.status == "queued" } }, Ku.airStatus.map { it.running }) { a, b -> a || b }
                    .distinctUntilChanged()
                    .collect { work -> if (work) ensure(app) }
            }
            Ku.launch {
                Ku.events.collect { e ->
                    val type = e["type"]?.jsonPrimitive?.contentOrNull
                    val str = { k: String -> e[k]?.jsonPrimitive?.contentOrNull }
                    when (type) {
                        "completed" -> {
                            str("path")?.let { Files.scan(app, it) }
                            if (Ku.settingBool("notifyComplete", true)) Notifier.completed(app, str("id") ?: "", str("name") ?: "", str("path"))
                        }
                        "notice" -> if (str("level") == "error" && Ku.settingBool("notifyError", true) && !MainActivity.visible) {
                            Notifier.problem(app, te(str("title") ?: ""), te(str("message") ?: ""), str("downloadId"))
                        }
                        "queueDone" -> if (Ku.settingBool("notifyQueueDone", true)) {
                            Notifier.info(app, t("Queue finished"), tf("All downloads in {name} are done.", "name" to (str("name") ?: "")))
                        }
                        "airSendRequest" -> if (!MainActivity.visible) {
                            Notifier.airRequest(app, Ku.json.decodeFromJsonElement<AirRequest>(e["request"]!!))
                        }
                        "airSendDownload" -> if (!MainActivity.visible) {
                            Notifier.airDownload(app, Ku.json.decodeFromJsonElement(e["request"]!!))
                        }
                        "airSendMessage" -> if (!MainActivity.visible) {
                            Notifier.airMessage(app, Ku.json.decodeFromJsonElement<AirMessage>(e["message"]!!))
                        }
                        "airSendTransfer" -> {
                            // Copies of shared files are only needed while sending.
                            val tr = Ku.json.decodeFromJsonElement<AirTransfer>(e["transfer"]!!)
                            if (tr.direction == "send" && !tr.isOpen && Ku.airTransfers.value.none { it.isOpen && it.direction == "send" }) {
                                Files.cleanAirSendCache(app)
                            }
                        }
                    }
                }
            }
        }
    }
}
