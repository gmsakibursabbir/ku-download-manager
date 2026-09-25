package digital.kuduy.kudownloader.core

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.File
import java.util.Locale

/** Opening, sharing and handing over files on the phone. */
object Files {
    fun authority(ctx: Context) = "${ctx.packageName}.files"

    fun mime(file: File): String {
        val ext = file.extension.lowercase(Locale.ROOT)
        return when (ext) {
            "apk" -> "application/vnd.android.package-archive"
            "torrent" -> "application/x-bittorrent"
            else -> MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
        }
    }

    fun uri(ctx: Context, file: File): Uri? = runCatching { FileProvider.getUriForFile(ctx, authority(ctx), file) }.getOrNull()

    fun viewIntent(ctx: Context, file: File): Intent? {
        val u = uri(ctx, file) ?: return null
        return Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(u, mime(file))
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
    }

    fun shareIntent(ctx: Context, file: File): Intent? {
        val u = uri(ctx, file) ?: return null
        val send = Intent(Intent.ACTION_SEND).apply {
            type = mime(file)
            putExtra(Intent.EXTRA_STREAM, u)
            clipData = ClipData.newRawUri(file.name, u)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        return Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }

    /** Returns false when no app can open the file. */
    fun open(ctx: Context, file: File): Boolean {
        val i = viewIntent(ctx, file) ?: return false
        return try {
            ctx.startActivity(i)
            true
        } catch (_: ActivityNotFoundException) {
            false
        }
    }

    fun share(ctx: Context, file: File) {
        shareIntent(ctx, file)?.let { runCatching { ctx.startActivity(it) } }
    }

    fun shareText(ctx: Context, text: String) {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, text)
        }
        runCatching { ctx.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    /** Show the folder in the system Files app (best effort: Android has no standard way). */
    fun openFolder(ctx: Context, dir: File) {
        val downloads = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
        val rel = dir.absolutePath.removePrefix(Environment.getExternalStorageDirectory().absolutePath).trimStart('/')
        val docId = "primary:$rel"
        val candidates = listOf(
            Intent(Intent.ACTION_VIEW).setDataAndType(DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", docId), DocumentsContract.Document.MIME_TYPE_DIR),
            Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse("content://com.android.externalstorage.documents/document/${Uri.encode(docId)}"), "resource/folder"),
            Intent("android.intent.action.VIEW_DOWNLOADS"),
        )
        for (i in candidates) {
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
            try {
                ctx.startActivity(i)
                return
            } catch (_: Exception) {
            }
        }
        if (dir.absolutePath.startsWith(downloads.absolutePath)) {
            runCatching { ctx.startActivity(Intent("android.intent.action.VIEW_DOWNLOADS").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
        }
    }

    /** Let the gallery and music apps see a finished download. */
    fun scan(ctx: Context, path: String) {
        runCatching { MediaScannerConnection.scanFile(ctx, arrayOf(path), null, null) }
    }

    fun displayName(ctx: Context, uri: Uri): String? = runCatching {
        ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    }.getOrNull() ?: uri.lastPathSegment?.substringAfterLast('/')

    fun readBytes(ctx: Context, uri: Uri, limit: Int = 20 shl 20): ByteArray? = runCatching {
        ctx.contentResolver.openInputStream(uri)?.use { input ->
            val out = java.io.ByteArrayOutputStream()
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
                if (out.size() > limit) return@runCatching null
            }
            out.toByteArray()
        }
    }.getOrNull()

    /**
     * A real path for a shared file, for KuAirSend (which reads from disk).
     * Files that are only reachable through a content provider are copied to
     * the cache first.
     */
    fun toLocalPath(ctx: Context, uri: Uri): String? {
        if (uri.scheme == "file") return uri.path
        val name = (displayName(ctx, uri) ?: "file").replace('/', '_').ifBlank { "file" }
        val dir = File(ctx.cacheDir, "airsend/${System.nanoTime()}").apply { mkdirs() }
        val out = File(dir, name)
        return runCatching {
            ctx.contentResolver.openInputStream(uri)?.use { input -> out.outputStream().use { input.copyTo(it, 256 * 1024) } }
            out.absolutePath
        }.getOrNull()
    }

    /** Remove copies made for KuAirSend transfers that have ended. */
    fun cleanAirSendCache(ctx: Context) {
        runCatching { File(ctx.cacheDir, "airsend").deleteRecursively() }
    }

    fun canWriteAnywhere(): Boolean = Build.VERSION.SDK_INT >= 30 && Environment.isExternalStorageManager()

    /** A folder chosen with the system picker, as a file path (primary storage only). */
    fun treeToPath(uri: Uri): String? {
        val id = runCatching { DocumentsContract.getTreeDocumentId(uri) }.getOrNull() ?: return null
        val (vol, rel) = id.split(':', limit = 2).let { it[0] to it.getOrElse(1) { "" } }
        val root = if (vol == "primary") Environment.getExternalStorageDirectory().absolutePath else "/storage/$vol"
        return if (rel.isEmpty()) root else "$root/$rel"
    }
}
