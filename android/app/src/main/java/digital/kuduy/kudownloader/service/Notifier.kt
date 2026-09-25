package digital.kuduy.kudownloader.service

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import digital.kuduy.kudownloader.MainActivity
import digital.kuduy.kudownloader.R
import digital.kuduy.kudownloader.core.AirMessage
import digital.kuduy.kudownloader.core.AirRequest
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import java.io.File

object Notifier {
    const val CH_PROGRESS = "progress"
    const val CH_DONE = "done"
    const val CH_ERRORS = "errors"
    const val CH_AIRSEND = "airsend"
    const val ID_SERVICE = 1

    fun channels(ctx: Context) {
        if (Build.VERSION.SDK_INT < 26) return
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannels(
            listOf(
                NotificationChannel(CH_PROGRESS, t("Downloads in progress"), NotificationManager.IMPORTANCE_LOW).apply { setShowBadge(false) },
                NotificationChannel(CH_DONE, t("Finished downloads"), NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CH_ERRORS, t("Download problems"), NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CH_AIRSEND, "KuAirSend", NotificationManager.IMPORTANCE_HIGH),
            ),
        )
    }

    fun allowed(ctx: Context): Boolean =
        Build.VERSION.SDK_INT < 33 || ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun post(ctx: Context, id: Int, n: android.app.Notification) {
        if (!allowed(ctx)) return
        runCatching { NotificationManagerCompat.from(ctx).notify(id, n) }
    }

    private fun flags() = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE

    fun openApp(ctx: Context, action: String? = null, extra: String? = null): PendingIntent {
        val i = Intent(ctx, MainActivity::class.java).apply {
            this.action = action ?: Intent.ACTION_MAIN
            extra?.let { putExtra("id", it) }
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        }
        return PendingIntent.getActivity(ctx, (action + extra).hashCode(), i, flags())
    }

    fun broadcast(ctx: Context, action: String, extra: String? = null, extra2: Boolean = false): PendingIntent {
        val i = Intent(ctx, ActionReceiver::class.java).apply {
            this.action = action
            extra?.let { putExtra("id", it) }
            putExtra("flag", extra2)
        }
        return PendingIntent.getBroadcast(ctx, (action + extra + extra2).hashCode(), i, flags())
    }

    fun completed(ctx: Context, id: String, name: String, path: String?) {
        val b = NotificationCompat.Builder(ctx, CH_DONE)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(t("Download complete"))
            .setContentText(name)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setContentIntent(openApp(ctx, MainActivity.ACTION_SHOW_DOWNLOAD, id))
        val file = path?.let { File(it) }?.takeIf { it.isFile }
        if (file != null) {
            Files.viewIntent(ctx, file)?.let { b.setContentIntent(PendingIntent.getActivity(ctx, id.hashCode(), it, flags())) }
            Files.shareIntent(ctx, file)?.let { b.addAction(0, t("Share"), PendingIntent.getActivity(ctx, id.hashCode() + 1, it, flags())) }
        }
        b.addAction(0, t("Show"), openApp(ctx, MainActivity.ACTION_SHOW_DOWNLOAD, id))
        post(ctx, id.hashCode(), b.build())
    }

    fun problem(ctx: Context, title: String, message: String, id: String?) {
        val b = NotificationCompat.Builder(ctx, CH_ERRORS)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx, MainActivity.ACTION_SHOW_DOWNLOAD, id))
        if (id != null) b.addAction(0, t("Retry"), broadcast(ctx, ActionReceiver.RESUME, id))
        post(ctx, (id ?: title).hashCode() + 7, b.build())
    }

    fun info(ctx: Context, title: String, message: String) {
        val b = NotificationCompat.Builder(ctx, CH_DONE)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(message)
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx))
        post(ctx, (title + message).hashCode(), b.build())
    }

    fun airRequest(ctx: Context, r: AirRequest) {
        val text = if (r.fileCount == 1) r.files.firstOrNull()?.name ?: "" else tf("{count} files", "count" to r.fileCount)
        val b = NotificationCompat.Builder(ctx, CH_AIRSEND)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(tf("{name} wants to send you files", "name" to r.peer))
            .setContentText("$text · ${Fmt.size(r.total)}")
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx, MainActivity.ACTION_AIRSEND, r.id))
            .addAction(0, t("Accept"), broadcast(ctx, ActionReceiver.AIR_ACCEPT, r.id))
            .addAction(0, t("Decline"), broadcast(ctx, ActionReceiver.AIR_DECLINE, r.id))
        post(ctx, r.id.hashCode(), b.build())
    }

    fun airDownload(ctx: Context, r: digital.kuduy.kudownloader.core.AirDownloadRequest) {
        val at = r.download.at?.takeIf { it > System.currentTimeMillis() + 30_000 }
        val text = r.download.filename ?: r.download.url
        val b = NotificationCompat.Builder(ctx, CH_AIRSEND)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(if (at != null) tf("{name} asks this phone to download at {time}", "name" to r.peer, "time" to Fmt.date(at)) else tf("{name} asks this phone to download", "name" to r.peer))
            .setContentText(text)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx, MainActivity.ACTION_AIRSEND, r.id))
            .addAction(0, t("Accept"), broadcast(ctx, ActionReceiver.AIR_ACCEPT, r.id))
            .addAction(0, t("Decline"), broadcast(ctx, ActionReceiver.AIR_DECLINE, r.id))
        post(ctx, r.id.hashCode(), b.build())
    }

    fun airMessage(ctx: Context, m: AirMessage) {
        val b = NotificationCompat.Builder(ctx, CH_AIRSEND)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(tf("Message from {name}", "name" to m.peer))
            .setContentText(m.text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(m.text))
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx, MainActivity.ACTION_AIRSEND, m.id))
            .addAction(0, t("Copy"), broadcast(ctx, ActionReceiver.COPY, m.text))
        if (m.text.trim().let { it.startsWith("http://") || it.startsWith("https://") || it.startsWith("magnet:") }) {
            b.addAction(0, t("Download"), openApp(ctx, MainActivity.ACTION_ADD_URL, m.text.trim()))
        }
        post(ctx, m.id.hashCode(), b.build())
    }

    fun cancel(ctx: Context, id: String) = NotificationManagerCompat.from(ctx).cancel(id.hashCode())
}
