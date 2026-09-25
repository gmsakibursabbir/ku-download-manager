package digital.kuduy.kudownloader.service

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.PowerManager
import androidx.core.content.ContextCompat
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first

/**
 * "Wi-Fi only" and "pause in battery saver": downloads this pauses are
 * remembered and resumed when the condition clears (never ones the user paused).
 */
object NetworkWatch {
    private val pausedByUs = mutableSetOf<String>()
    @Volatile private var metered = false
    @Volatile private var saver = false

    fun start(ctx: Context) {
        val app = ctx.applicationContext
        val cm = app.getSystemService(ConnectivityManager::class.java)
        cm.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                val m = !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
                if (m != metered) {
                    metered = m
                    evaluate(app)
                }
            }

            override fun onLost(network: Network) {
                // No network: engines retry by themselves; nothing to pause.
            }
        })
        val pm = app.getSystemService(PowerManager::class.java)
        saver = pm.isPowerSaveMode
        ContextCompat.registerReceiver(
            app,
            object : BroadcastReceiver() {
                override fun onReceive(c: Context, i: Intent) {
                    saver = pm.isPowerSaveMode
                    evaluate(app)
                }
            },
            IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        Ku.launch {
            combine(Prefs.wifiOnly.state, Prefs.pauseOnBatterySaver.state) { a, b -> a to b }.collect { evaluate(app) }
        }
        // New downloads started while held back are paused too.
        Ku.launch {
            Ku.downloads.collect { if (shouldHold()) evaluate(app) }
        }
    }

    private fun shouldHold() = (Prefs.wifiOnly.value && metered) || (Prefs.pauseOnBatterySaver.value && saver)

    fun reason(): String? = when {
        Prefs.wifiOnly.value && metered -> t("Waiting for Wi-Fi")
        Prefs.pauseOnBatterySaver.value && saver -> t("Paused by the battery saver")
        else -> null
    }

    private fun evaluate(ctx: Context) {
        Ku.launch {
            Ku.phase.first { it == Ku.Phase.Ready }
            if (shouldHold()) {
                // Only HTTP-like transfers use mobile data heavily; seeding torrents are paused too.
                val ids = Ku.downloads.value.values.filter { it.isRunning || it.status == "queued" || it.status == "seeding" }.map { it.id }
                if (ids.isNotEmpty()) {
                    synchronized(pausedByUs) { pausedByUs += ids }
                    runCatching { Ku.pause(ids) }
                }
            } else {
                val ids = synchronized(pausedByUs) { pausedByUs.toList().also { pausedByUs.clear() } }
                val still = ids.filter { Ku.downloads.value[it]?.status == "paused" }
                if (still.isNotEmpty()) {
                    runCatching { Ku.resume(still) }
                    KuService.ensure(ctx)
                }
            }
        }
    }
}
