package digital.kuduy.kudownloader.core

import digital.kuduy.kudownloader.i18n.I18n
import digital.kuduy.kudownloader.i18n.t
import java.text.DateFormat
import java.text.NumberFormat
import java.util.Date
import java.util.Locale

/** Sizes, speeds, durations and dates as the interface shows them. */
object Fmt {
    private fun locale(): Locale = Locale.forLanguageTag(I18n.lang)

    fun size(bytes: Long?): String {
        if (bytes == null || bytes < 0) return "—"
        if (bytes < 1024) return "$bytes B"
        val units = listOf("KB", "MB", "GB", "TB")
        var v = bytes / 1024.0
        var i = 0
        while (v >= 1024 && i < units.lastIndex) {
            v /= 1024
            i++
        }
        val nf = NumberFormat.getNumberInstance(locale()).apply { maximumFractionDigits = if (v < 10) 2 else if (v < 100) 1 else 0 }
        return "${nf.format(v)} ${units[i]}"
    }

    fun speed(bps: Long): String = if (bps <= 0) "0 B/s" else "${size(bps)}/s"

    /** "1h 04m", "3m 12s", "42s" — whole units kept short on a phone. */
    fun duration(seconds: Long?): String {
        if (seconds == null || seconds < 0) return "—"
        val h = seconds / 3600
        val m = (seconds % 3600) / 60
        val s = seconds % 60
        return when {
            h > 99 -> "∞"
            h > 0 -> "${h}h ${"%02d".format(m)}m"
            m > 0 -> "${m}m ${"%02d".format(s)}s"
            else -> "${s}s"
        }
    }

    fun clock(seconds: Double?): String {
        if (seconds == null) return ""
        val total = seconds.toLong()
        val h = total / 3600
        val m = (total % 3600) / 60
        val s = total % 60
        return if (h > 0) "%d:%02d:%02d".format(h, m, s) else "%d:%02d".format(m, s)
    }

    fun date(ms: Long?): String {
        if (ms == null || ms <= 0) return ""
        return DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, locale()).format(Date(ms))
    }

    fun relative(ms: Long?): String {
        if (ms == null || ms <= 0) return ""
        val diff = (System.currentTimeMillis() - ms) / 1000
        return when {
            diff < 60 -> t("just now")
            diff < 3600 -> I18n.tf("{n} min ago", "n" to diff / 60)
            diff < 86400 -> I18n.tf("{n} h ago", "n" to diff / 3600)
            else -> DateFormat.getDateInstance(DateFormat.MEDIUM, locale()).format(Date(ms))
        }
    }

    fun percent(p: Float): String = NumberFormat.getPercentInstance(locale()).format(p.toDouble())

    fun status(d: Download): String = when (d.status) {
        "queued" -> t("Queued")
        "downloading" -> t("Downloading")
        "processing" -> t("Processing")
        "paused" -> t("Paused")
        "seeding" -> t("Seeding")
        "completed" -> t("Completed")
        "error" -> t("Error")
        else -> d.status
    }

    fun extension(name: String): String = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
}
