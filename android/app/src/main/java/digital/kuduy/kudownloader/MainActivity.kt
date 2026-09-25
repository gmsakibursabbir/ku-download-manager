package digital.kuduy.kudownloader

import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Base64
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.content.IntentCompat
import androidx.lifecycle.lifecycleScope
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.KuRoot
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.UiState
import digital.kuduy.kudownloader.ui.looksLikeLink
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent { KuRoot() }
        if (savedInstanceState == null) handle(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    override fun onResume() {
        super.onResume()
        visible = true
        KuService.ensure(this)
    }

    override fun onPause() {
        visible = false
        super.onPause()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        // Android only lets the focused app read the clipboard.
        if (hasFocus) offerClipboard()
    }

    private fun offerClipboard(force: Boolean = false) {
        if (!force && (!Prefs.clipboardOffer.value || !Ku.settingBool("clipboardMonitor", true))) return
        val cm = getSystemService(ClipboardManager::class.java) ?: return
        val text = runCatching { cm.primaryClip?.getItemAt(0)?.coerceToText(this)?.toString() }.getOrNull()?.trim() ?: return
        if (!looksLikeLink(text)) {
            if (force) UiState.toast(t("The clipboard has no link."))
            return
        }
        if (!force && text == Prefs.lastClipboard.value) return
        Prefs.lastClipboard.value = text
        if (force) UiState.add = AddPrefill(url = text) else UiState.clipboardOffer = text
    }

    private fun handle(intent: Intent?) {
        intent ?: return
        val id = intent.getStringExtra("id")
        when (intent.action) {
            ACTION_SHOW_DOWNLOAD -> if (id != null) {
                UiState.go(Screen.Downloads)
                UiState.details = id
            }
            ACTION_AIRSEND -> UiState.go(Screen.AirSend)
            ACTION_ADD_URL -> if (id != null) UiState.add = AddPrefill(url = id)
            ACTION_ADD_CLIPBOARD -> window.decorView.post { offerClipboard(force = true) }
            Intent.ACTION_PROCESS_TEXT -> {
                val text = intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString().orEmpty()
                links(text)
            }
            Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE -> shared(intent)
            Intent.ACTION_VIEW -> viewed(intent.data)
        }
    }

    /** Text shares carry links; file shares go to KuAirSend. */
    private fun shared(intent: Intent) {
        val streams: List<Uri> = if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
            IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        } else {
            listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
        }
        if (streams.isNotEmpty()) {
            val torrent = streams.singleOrNull()?.takeIf { (Files.displayName(this, it) ?: "").endsWith(".torrent", true) }
            if (torrent != null) return viewed(torrent)
            lifecycleScope.launch {
                val paths = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { streams.mapNotNull { Files.toLocalPath(this@MainActivity, it) } }
                if (paths.isEmpty()) {
                    UiState.toast(t("These files could not be read."))
                    return@launch
                }
                UiState.airShare = paths
                UiState.go(Screen.AirSend)
            }
            return
        }
        val text = intent.getStringExtra(Intent.EXTRA_TEXT).orEmpty() + " " + intent.getStringExtra(Intent.EXTRA_SUBJECT).orEmpty()
        links(text)
    }

    private fun links(text: String) {
        val found = Regex("""(?:https?|ftp|sftp)://[^\s<>"']+|magnet:\?[^\s<>"']+""").findAll(text).map { it.value.trimEnd('.', ',', ')', ']') }.distinct().toList()
        when {
            found.isEmpty() -> UiState.toast(t("No link was found in the shared text."))
            found.size == 1 -> UiState.add = AddPrefill(url = found[0], source = "share")
            else -> {
                UiState.batch = found
                UiState.go(Screen.Batch)
            }
        }
    }

    private fun viewed(uri: Uri?) {
        uri ?: return
        when (uri.scheme) {
            "magnet", "http", "https", "ftp" -> UiState.add = AddPrefill(url = uri.toString(), source = "link")
            "content", "file" -> lifecycleScope.launch {
                Ku.phase.first { it != Ku.Phase.Starting }
                val bytes = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { Files.readBytes(this@MainActivity, uri) }
                if (bytes == null) {
                    UiState.toast(t("This file could not be read."))
                    return@launch
                }
                val b64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
                runCatching { Ku.torrentInfo(b64) }
                    .onSuccess { (info, data) -> UiState.add = AddPrefill(url = "magnet:?xt=urn:btih:${info.infoHash}", filename = info.name, torrent = info, torrentData = data, size = info.total, source = "torrent") }
                    .onFailure { UiState.toast(it.message ?: t("This is not a valid .torrent file.")) }
            }
        }
    }

    companion object {
        const val ACTION_SHOW_DOWNLOAD = "digital.kuduy.kudownloader.SHOW_DOWNLOAD"
        const val ACTION_AIRSEND = "digital.kuduy.kudownloader.AIRSEND"
        const val ACTION_ADD_URL = "digital.kuduy.kudownloader.ADD_URL"
        const val ACTION_ADD_CLIPBOARD = "digital.kuduy.kudownloader.ADD_CLIPBOARD"

        @Volatile var visible = false
            private set

        val sdk = Build.VERSION.SDK_INT
    }
}
