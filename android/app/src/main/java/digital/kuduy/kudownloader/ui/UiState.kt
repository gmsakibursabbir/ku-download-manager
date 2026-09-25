package digital.kuduy.kudownloader.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import digital.kuduy.kudownloader.core.AirMessage
import digital.kuduy.kudownloader.core.AirRequest
import digital.kuduy.kudownloader.core.BrowserCookie
import digital.kuduy.kudownloader.core.GrabLink
import digital.kuduy.kudownloader.core.TorrentInfo

enum class Screen(val top: Boolean = false) {
    Downloads(true), Browser(true), AirSend(true), Video(true), More(true),
    Batch, Fetch, Queues, Settings, About,
}

/** What the Add sheet starts with (a link from the browser, a share, a notification…). */
data class AddPrefill(
    val url: String = "",
    val filename: String? = null,
    val referer: String? = null,
    val userAgent: String? = null,
    val cookies: List<BrowserCookie> = emptyList(),
    val size: Long? = null,
    val mime: String? = null,
    val torrent: TorrentInfo? = null,
    /** Base64 .torrent contents. */
    val torrentData: String? = null,
    val source: String = "android",
)

data class MediaPrefill(
    val url: String,
    val cookies: List<BrowserCookie> = emptyList(),
    val referer: String? = null,
    val title: String? = null,
    val auto: Boolean = true,
)

/** Links to hand to another KuDownloader (a PC or laptop) to download there. */
data class RemoteSend(
    val urls: List<String>,
    val filename: String? = null,
    val referer: String? = null,
    val cookies: List<BrowserCookie> = emptyList(),
)

data class GrabPrefill(
    val pageUrl: String?,
    val links: List<GrabLink>,
    val cookies: List<BrowserCookie> = emptyList(),
    val userAgent: String? = null,
)

/** Interface state that outlives a single screen. */
object UiState {
    val stack = mutableStateListOf(Screen.Downloads)
    val screen get() = stack.last()

    var add by mutableStateOf<AddPrefill?>(null)
    var media by mutableStateOf<MediaPrefill?>(null)
    var grab by mutableStateOf<GrabPrefill?>(null)
    var batch by mutableStateOf<List<String>?>(null)
    var details by mutableStateOf<String?>(null)
    var browserUrl by mutableStateOf<String?>(null)
    /** Files shared to KuDownloader, waiting for the user to pick a device. */
    var airShare by mutableStateOf<List<String>>(emptyList())
    /** Text (links) waiting to be sent with KuAirSend. */
    var airShareText by mutableStateOf<String?>(null)
    var settingsSection by mutableStateOf<String?>(null)
    /** A copied link, offered when the app comes to the front. */
    var clipboardOffer by mutableStateOf<String?>(null)

    val airRequests = mutableStateListOf<AirRequest>()
    val airMessages = mutableStateListOf<AirMessage>()
    val airDownloads = mutableStateListOf<digital.kuduy.kudownloader.core.AirDownloadRequest>()
    var remoteSend by mutableStateOf<RemoteSend?>(null)

    /** Short messages at the bottom of the screen. */
    val toasts = mutableStateListOf<String>()

    fun go(s: Screen) {
        if (s.top) {
            stack.clear()
            stack.add(s)
        } else {
            stack.remove(s)
            stack.add(s)
        }
    }

    fun back(): Boolean {
        if (stack.size > 1) {
            stack.removeAt(stack.lastIndex)
            return true
        }
        if (stack.last() != Screen.Downloads) {
            stack[0] = Screen.Downloads
            return true
        }
        return false
    }

    fun toast(msg: String) {
        toasts.add(msg)
    }
}
