package digital.kuduy.kudownloader.service

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Schedule
import kotlinx.coroutines.flow.first
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneId

/**
 * KuCore runs schedules itself while the app is alive; this wakes the app
 * when the next one is due (the phone may have stopped it in the meantime).
 */
object ScheduleAlarm {
    fun watch(ctx: Context) {
        val app = ctx.applicationContext
        Ku.launch { Ku.schedules.collect { arm(app) } }
    }

    fun arm(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java) ?: return
        val pi = PendingIntent.getBroadcast(ctx, 42, Intent(ctx, Receiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val next = Ku.schedules.value.filter { it.enabled }.mapNotNull { next(it) }.minOrNull()
        if (next == null) {
            am.cancel(pi)
            return
        }
        val at = next.atZone(ZoneId.systemDefault()).toInstant().toEpochMilli()
        val exact = Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()
        if (exact) am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi) else am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
    }

    /** Next local start time of a schedule, or null when it will not run again. */
    fun next(s: Schedule, now: LocalDateTime = LocalDateTime.now()): LocalDateTime? {
        val time = runCatching { LocalTime.parse(s.start) }.getOrNull() ?: return null
        s.date?.takeIf { it.isNotBlank() }?.let { d ->
            val at = runCatching { LocalDate.parse(d) }.getOrNull()?.atTime(time) ?: return null
            return at.takeIf { it.isAfter(now) }
        }
        for (i in 0..7) {
            val day = now.toLocalDate().plusDays(i.toLong())
            val at = day.atTime(time)
            if (!at.isAfter(now)) continue
            if (s.days.isEmpty() || day.dayOfWeek.value in s.days) return at
        }
        return null
    }

    class Receiver : BroadcastReceiver() {
        override fun onReceive(ctx: Context, intent: Intent) {
            val pending = goAsync()
            Ku.launch {
                try {
                    Ku.phase.first { it != Ku.Phase.Starting }
                    // KuCore's scheduler starts the queue within the minute;
                    // the service keeps the app running while it does.
                    kotlinx.coroutines.delay(5_000)
                    KuService.ensure(ctx.applicationContext)
                    arm(ctx.applicationContext)
                } finally {
                    pending.finish()
                }
            }
        }
    }
}
