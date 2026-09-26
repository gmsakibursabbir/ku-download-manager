package digital.kuduy.kudownloader.core

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import digital.kuduy.kudownloader.BuildConfig
import digital.kuduy.kudownloader.i18n.t
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * App updates from the GitHub releases (the Play Store does not allow video
 * downloaders): checked at start, the APK for this phone is downloaded in the
 * app and handed to the system installer.
 */
object AppUpdate {
    private const val API = "https://api.github.com/repos/kuduyDigital/ku-download-manager/releases/latest"

    data class Release(val version: String, val apk: String?, val page: String)

    /** A newer release, once found. */
    val available = MutableStateFlow<Release?>(null)
    /** 0..1 while the APK downloads, null otherwise. */
    val progress = MutableStateFlow<Float?>(null)

    /** The latest release (null when offline); [available] is set when it is newer. */
    suspend fun check(): Release? = withContext(Dispatchers.IO) {
        val r = runCatching { fetch() }.getOrNull() ?: return@withContext null
        available.value = r.takeIf { isNewer(it.version, BuildConfig.VERSION_NAME) }
        r
    }

    private fun fetch(): Release? {
        val c = URL(API).openConnection() as HttpURLConnection
        c.setRequestProperty("Accept", "application/vnd.github+json")
        c.connectTimeout = 10_000
        c.readTimeout = 10_000
        val o = Ku.json.parseToJsonElement(c.inputStream.bufferedReader().use { it.readText() }).jsonObject
        val tag = o["tag_name"]?.jsonPrimitive?.contentOrNull ?: return null
        val names = o["assets"]?.jsonArray?.map { it.jsonObject }.orEmpty()
        fun asset(part: String) = names.firstOrNull { a ->
            val n = a["name"]?.jsonPrimitive?.contentOrNull.orEmpty()
            n.endsWith(".apk") && n.contains(part)
        }?.get("browser_download_url")?.jsonPrimitive?.contentOrNull
        // This phone's ABI first, then the universal APK.
        val apk = Build.SUPPORTED_ABIS.firstNotNullOfOrNull { asset("android-$it.apk") } ?: asset("universal")
        val page = o["html_url"]?.jsonPrimitive?.contentOrNull ?: "https://github.com/kuduyDigital/ku-download-manager/releases/latest"
        return Release(tag.removePrefix("v"), apk, page)
    }

    /**
     * Downloads the APK and opens the installer. Returns an error message, or
     * null when the installer (or the permission screen it needs) was opened.
     */
    suspend fun install(ctx: Context, r: Release): String? {
        val url = r.apk ?: return openPage(ctx, r)
        // Android 8+: the user allows "install unknown apps" for KuDownloader once.
        if (Build.VERSION.SDK_INT >= 26 && !ctx.packageManager.canRequestPackageInstalls()) {
            val i = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            return if (runCatching { ctx.startActivity(i) }.isSuccess) null else openPage(ctx, r)
        }
        val file = File(ctx.cacheDir, "update/KuDownloader-${r.version}.apk")
        val ok = withContext(Dispatchers.IO) {
            runCatching {
                file.parentFile?.listFiles()?.forEach { if (it != file) it.delete() }
                file.parentFile?.mkdirs()
                progress.value = 0f
                val c = URL(url).openConnection() as HttpURLConnection
                c.connectTimeout = 15_000
                c.readTimeout = 30_000
                val total = c.contentLengthLong
                val tmp = File(file.path + ".part")
                c.inputStream.use { input ->
                    tmp.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        var done = 0L
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            out.write(buf, 0, n)
                            done += n
                            if (total > 0) progress.value = done.toFloat() / total
                        }
                    }
                }
                if (total > 0 && tmp.length() != total) error("incomplete")
                tmp.renameTo(file)
            }.getOrDefault(false)
        }
        progress.value = null
        if (!ok) return t("Could not download the update. Check the connection and try again.")
        return if (Files.open(ctx, file)) null else openPage(ctx, r)
    }

    private fun openPage(ctx: Context, r: Release): String? {
        val i = Intent(Intent.ACTION_VIEW, Uri.parse(r.apk ?: r.page)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return if (runCatching { ctx.startActivity(i) }.isSuccess) null else t("Could not open the download page.")
    }

    fun isNewer(candidate: String, current: String): Boolean {
        fun parts(v: String) = v.removePrefix("v").substringBefore('-').split('.').map { it.toIntOrNull() ?: 0 }
        if (candidate.contains('-')) return false
        val a = parts(candidate)
        val b = parts(current)
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }
}
