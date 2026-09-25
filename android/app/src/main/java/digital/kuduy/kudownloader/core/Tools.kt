package digital.kuduy.kudownloader.core

import android.content.Context
import android.util.Log
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * yt-dlp (run by its bundled Python), FFmpeg and aria2 come from
 * youtubedl-android: the programs sit in the app's library folder (the only
 * place Android lets an app execute files) and their support files are
 * unpacked on first start. KuCore runs them itself; this finds where they are,
 * and repairs a missing yt-dlp by downloading it.
 */
object Tools {
    data class Found(
        val python: String?,
        val ytdlp: String?,
        val ffmpeg: String?,
        val aria2: String?,
        val env: Map<String, String>,
        val problems: List<String>,
    ) {
        val videoReady get() = python != null && ytdlp != null
    }

    private const val TAG = "KuTools"
    private const val YTDLP_URL = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"

    private fun base(ctx: Context) = File(ctx.noBackupFilesDir, "youtubedl-android")
    private fun ytdlpFile(ctx: Context) = File(base(ctx), "yt-dlp/yt-dlp")

    fun prepare(ctx: Context): Found {
        val problems = mutableListOf<String>()
        runCatching { YoutubeDL.getInstance().init(ctx) }.onFailure { problems += "yt-dlp: ${causes(it)}"; Log.w(TAG, "yt-dlp", it) }
        runCatching { FFmpeg.getInstance().init(ctx) }.onFailure { problems += "FFmpeg: ${causes(it)}"; Log.w(TAG, "ffmpeg", it) }
        runCatching { Aria2c.getInstance().init(ctx) }.onFailure { problems += "aria2: ${causes(it)}"; Log.w(TAG, "aria2", it) }
        return locate(ctx, problems)
    }

    private fun causes(t: Throwable): String = generateSequence(t) { it.cause }.take(3).mapNotNull { it.message }.distinct().joinToString(": ")

    private fun locate(ctx: Context, problems: MutableList<String>): Found {
        val lib = File(ctx.applicationInfo.nativeLibraryDir)
        val exe = { name: String -> File(lib, name).takeIf { it.isFile }?.absolutePath }
        val base = base(ctx)
        val packages = File(base, "packages")
        val python = File(packages, "python/usr").takeIf { File(it, "lib").isDirectory }
        val ytdlp = ytdlpFile(ctx).takeIf { it.isFile && it.length() > 100_000 }
        // Every unpacked package's usr/lib (Python, FFmpeg, aria2 libraries).
        val libs = packages.listFiles().orEmpty().map { File(it, "usr/lib") }.filter { it.isDirectory }.map { it.absolutePath }
        val cert = python?.let { File(it, "etc/tls/cert.pem") }?.takeIf { it.isFile }

        val env = buildMap {
            python?.let {
                put("PYTHONHOME", it.absolutePath)
            }
            put("LD_LIBRARY_PATH", (libs + lib.absolutePath).joinToString(":"))
            cert?.let { put("SSL_CERT_FILE", it.absolutePath) }
            put("TMPDIR", ctx.cacheDir.absolutePath)
            put("HOME", ctx.filesDir.absolutePath)
            exe("libpython.so")?.let { put("KU_PYTHON", it) }
        }
        if (exe("libpython.so") == null) problems += "Python is missing from this build (${android.os.Build.SUPPORTED_ABIS.firstOrNull()})"
        else if (python == null) problems += "Python's files were not unpacked"
        if (ytdlp == null) problems += "yt-dlp is not installed"
        val found = Found(exe("libpython.so")?.takeIf { python != null }, ytdlp?.absolutePath, exe("libffmpeg.so"), exe("libaria2c.so"), env, problems.distinct())
        Log.i(TAG, "python=${found.python} ytdlp=${found.ytdlp} ffmpeg=${found.ffmpeg} aria2=${found.aria2} problems=${found.problems}")
        return found
    }

    /**
     * Unpack the tools again and, when yt-dlp is still missing, download the
     * official release. [progress] gets (done, total) bytes.
     */
    fun repair(ctx: Context, progress: (Long, Long) -> Unit = { _, _ -> }): Found {
        var found = prepare(ctx)
        if (found.ytdlp == null) {
            runCatching { download(ytdlpFile(ctx), progress) }.onFailure { Log.w(TAG, "yt-dlp download", it) }
            found = locate(ctx, mutableListOf())
        }
        return found
    }

    private fun download(target: File, progress: (Long, Long) -> Unit) {
        target.parentFile?.mkdirs()
        val part = File(target.path + ".part")
        var url = URL(YTDLP_URL)
        var conn: HttpURLConnection
        var hops = 0
        while (true) {
            conn = url.openConnection() as HttpURLConnection
            conn.instanceFollowRedirects = false
            conn.connectTimeout = 20_000
            conn.readTimeout = 30_000
            conn.setRequestProperty("User-Agent", "KuDownloader")
            val code = conn.responseCode
            if (code in 300..399 && hops++ < 5) {
                url = URL(url, conn.getHeaderField("Location"))
                conn.disconnect()
                continue
            }
            if (code != 200) throw IllegalStateException("HTTP $code")
            break
        }
        val total = conn.contentLengthLong
        conn.inputStream.use { input ->
            part.outputStream().use { out ->
                val buf = ByteArray(64 * 1024)
                var done = 0L
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    done += n
                    progress(done, total)
                }
            }
        }
        // yt-dlp's Unix release is a Python zip app: it starts with a shebang and contains a zip.
        if (part.length() < 1_000_000) throw IllegalStateException("download too small")
        if (!part.renameTo(target)) {
            part.copyTo(target, overwrite = true)
            part.delete()
        }
    }
}
