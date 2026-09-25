package digital.kuduy.kudownloader.service

import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.service.quicksettings.TileService
import digital.kuduy.kudownloader.MainActivity
import digital.kuduy.kudownloader.core.Ku
import kotlinx.coroutines.flow.first

/** Notification buttons: pause/resume, KuAirSend answers, copy. */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val id = intent.getStringExtra("id")
        val pending = goAsync()
        Ku.launch {
            try {
                Ku.phase.first { it != Ku.Phase.Starting }
                when (intent.action) {
                    PAUSE_ALL -> Ku.pauseAll()
                    RESUME_ALL -> {
                        Ku.resumeAll()
                        KuService.ensure(ctx.applicationContext)
                    }
                    RESUME -> id?.let {
                        Ku.resume(listOf(it))
                        KuService.ensure(ctx.applicationContext)
                    }
                    PAUSE -> id?.let { Ku.pause(listOf(it)) }
                    AIR_ACCEPT -> id?.let {
                        Ku.airDecide(it, accept = true, trust = false)
                        Notifier.cancel(ctx, it)
                    }
                    AIR_DECLINE -> id?.let {
                        Ku.airDecide(it, accept = false, trust = false)
                        Notifier.cancel(ctx, it)
                    }
                    AIR_OFF -> Ku.airSetEnabled(false)
                    COPY -> id?.let {
                        ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("KuAirSend", it))
                    }
                }
            } catch (_: Exception) {
            } finally {
                pending.finish()
            }
        }
    }

    companion object {
        const val PAUSE_ALL = "digital.kuduy.kudownloader.PAUSE_ALL"
        const val RESUME_ALL = "digital.kuduy.kudownloader.RESUME_ALL"
        const val RESUME = "digital.kuduy.kudownloader.RESUME"
        const val PAUSE = "digital.kuduy.kudownloader.PAUSE"
        const val AIR_ACCEPT = "digital.kuduy.kudownloader.AIR_ACCEPT"
        const val AIR_DECLINE = "digital.kuduy.kudownloader.AIR_DECLINE"
        const val AIR_OFF = "digital.kuduy.kudownloader.AIR_OFF"
        const val COPY = "digital.kuduy.kudownloader.COPY"
    }
}

/**
 * After a restart or an app update: KuCore resumes the downloads that were
 * running, and schedules are set again.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        // KuApp.onCreate has started KuCore by now; boot broadcasts may start
        // a foreground service.
        val pending = goAsync()
        Ku.launch {
            try {
                Ku.phase.first { it != Ku.Phase.Starting }
                KuService.ensure(ctx.applicationContext)
                ScheduleAlarm.arm(ctx.applicationContext)
            } finally {
                pending.finish()
            }
        }
    }
}

/** Quick Settings tile: add the copied link. */
class AddTileService : TileService() {
    @android.annotation.SuppressLint("StartActivityAndCollapseDeprecated") // only below Android 14
    override fun onClick() {
        super.onClick()
        val i = Intent(this, MainActivity::class.java).setAction(MainActivity.ACTION_ADD_CLIPBOARD).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            val pi = android.app.PendingIntent.getActivity(this, 0, i, android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT)
            startActivityAndCollapse(pi)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(i)
        }
    }
}
