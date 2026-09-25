package digital.kuduy.kudownloader.core

import android.content.Context
import android.util.Log
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import java.io.File

/**
 * yt-dlp (run by its bundled Python), FFmpeg and aria2 come from
 * youtubedl-android: the programs sit in the app's library folder (the only
 * place Android lets an app execute files) and their support files are
 * unpacked on first start. KuCore runs them itself; this finds where they are.
 */
object Tools {
    data class Found(
        val python: String?,
        val ytdlp: String?,
        val ffmpeg: String?,
        val aria2: String?,
        val env: Map<String, String>,
        val problems: List<String>,
    )

    private const val TAG = "KuTools"

    fun prepare(ctx: Context): Found {
        val problems = mutableListOf<String>()
        runCatching { YoutubeDL.getInstance().init(ctx) }.onFailure { problems += "yt-dlp: ${it.message}"; Log.w(TAG, "yt-dlp", it) }
        runCatching { FFmpeg.getInstance().init(ctx) }.onFailure { problems += "FFmpeg: ${it.message}"; Log.w(TAG, "ffmpeg", it) }
        runCatching { Aria2c.getInstance().init(ctx) }.onFailure { problems += "aria2: ${it.message}"; Log.w(TAG, "aria2", it) }

        val lib = File(ctx.applicationInfo.nativeLibraryDir)
        val exe = { name: String -> File(lib, name).takeIf { it.isFile }?.absolutePath }
        val base = File(ctx.noBackupFilesDir, "youtubedl-android")
        val packages = File(base, "packages")
        val python = File(packages, "python/usr").takeIf { it.isDirectory } ?: find(base) { it.isDirectory && it.name == "usr" && File(it, "lib").list()?.any { n -> n.startsWith("python3") } == true }
        val ytdlp = File(base, "yt-dlp/yt-dlp").takeIf { it.isFile } ?: find(base) { it.isFile && it.name == "yt-dlp" }
        // Every unpacked package's usr/lib (Python, FFmpeg, aria2 libraries).
        val libs = packages.listFiles().orEmpty().map { File(it, "usr/lib") }.filter { it.isDirectory }.map { it.absolutePath }
        val cert = python?.let { File(it, "etc/tls/cert.pem") }?.takeIf { it.isFile }

        val env = buildMap {
            put("PYTHONHOME", python?.absolutePath ?: "")
            put("LD_LIBRARY_PATH", (libs + lib.absolutePath).joinToString(":"))
            cert?.let { put("SSL_CERT_FILE", it.absolutePath) }
            put("TMPDIR", ctx.cacheDir.absolutePath)
            put("HOME", ctx.filesDir.absolutePath)
            exe("libpython.so")?.let { put("KU_PYTHON", it) }
        }
        val found = Found(exe("libpython.so"), ytdlp?.absolutePath, exe("libffmpeg.so"), exe("libaria2c.so"), env, problems)
        if (found.python == null || found.ytdlp == null) problems += "yt-dlp is unavailable on this device"
        Log.i(TAG, "python=${found.python} ytdlp=${found.ytdlp} ffmpeg=${found.ffmpeg} aria2=${found.aria2}")
        return found
    }

    private fun find(root: File, depth: Int = 5, match: (File) -> Boolean): File? {
        if (depth < 0 || !root.isDirectory) return null
        for (f in root.listFiles().orEmpty()) {
            if (match(f)) return f
            if (f.isDirectory) find(f, depth - 1, match)?.let { return it }
        }
        return null
    }
}
