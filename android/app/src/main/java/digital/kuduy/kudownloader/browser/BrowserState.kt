package digital.kuduy.kudownloader.browser

import android.annotation.SuppressLint
import android.content.Context
import android.content.MutableContextWrapper
import android.os.Build
import android.webkit.CookieManager
import android.webkit.WebSettings
import android.webkit.WebStorage
import android.webkit.WebView
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature
import digital.kuduy.kudownloader.core.BrowserCookie
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import java.net.URI
import java.util.UUID

@Serializable
data class Bookmark(val url: String, val title: String, val at: Long = System.currentTimeMillis())

/** A media stream a page loaded (found by watching its requests). */
data class FoundMedia(val url: String, val kind: String, val page: String, val title: String)

class Tab(val id: String = UUID.randomUUID().toString()) {
    var url by mutableStateOf("")
    var title by mutableStateOf("")
    var progress by mutableIntStateOf(100)
    var canBack by mutableStateOf(false)
    var canForward by mutableStateOf(false)
    var blocked by mutableIntStateOf(0)
    val media = mutableStateListOf<FoundMedia>()
    /** The page shows a <video> (reported by the page script). */
    var hasVideo by mutableStateOf(false)
    /** Something was opened in this tab (a popup gets content before any address). */
    var started by mutableStateOf(false)
    var view: WebView? = null
    /** Secret the page script must pass back, so other scripts can't drive the bridge. */
    val token: String = UUID.randomUUID().toString().replace("-", "")
    val isStart get() = !started && url.isBlank()
    /** Offer "Download video": a known video page, a player on the page, or media it loaded. */
    val canDownload get() = started && (hasVideo || media.isNotEmpty() || isVideoPage(url))
}

/**
 * Browser tabs and their WebViews. They live outside the composition (and
 * survive screen switches); each WebView sits in a context wrapper that is
 * pointed at the current activity.
 */
object BrowserState {
    val tabs = mutableStateListOf<Tab>()
    var current by mutableStateOf<Tab?>(null)
    val bookmarks = mutableStateListOf<Bookmark>()
    val history = mutableStateListOf<Bookmark>()
    private var loaded = false

    const val MOBILE_UA_SUFFIX = " KuDownloader"

    fun load() {
        if (loaded) return
        loaded = true
        runCatching { bookmarks.addAll(Ku.json.decodeFromString<List<Bookmark>>(Prefs.bookmarks.value)) }
        runCatching { history.addAll(Ku.json.decodeFromString<List<Bookmark>>(Prefs.history.value)) }
        if (tabs.isEmpty()) newTab()
    }

    fun newTab(url: String? = null): Tab {
        val t = Tab()
        tabs.add(t)
        current = t
        if (url != null) t.url = url
        return t
    }

    fun close(t: Tab) {
        val i = tabs.indexOf(t)
        tabs.remove(t)
        t.view?.let {
            it.stopLoading()
            it.destroy()
        }
        t.view = null
        if (tabs.isEmpty()) newTab()
        if (current == t) current = tabs.getOrNull((i - 1).coerceAtLeast(0)) ?: tabs.first()
    }

    @SuppressLint("SetJavaScriptEnabled")
    fun webView(t: Tab, ctx: Context): WebView {
        t.view?.let { v ->
            (v.context as? MutableContextWrapper)?.baseContext = ctx
            return v
        }
        val v = WebView(MutableContextWrapper(ctx))
        v.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            loadWithOverviewMode = true
            useWideViewPort = true
            builtInZoomControls = true
            displayZoomControls = false
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = false
            mediaPlaybackRequiresUserGesture = true
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
            userAgentString = userAgent(v.settings.userAgentString)
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING) && Build.VERSION.SDK_INT >= 33) {
            WebSettingsCompat.setAlgorithmicDarkeningAllowed(v.settings, true)
        }
        CookieManager.getInstance().setAcceptThirdPartyCookies(v, true)
        v.addJavascriptInterface(PageBridge(t), "KuBridge")
        v.webViewClient = KuWebClient(t)
        v.webChromeClient = KuChromeClient(t)
        v.setDownloadListener(KuDownloads(t))
        t.view = v
        if (t.url.isNotBlank()) v.loadUrl(t.url)
        return v
    }

    private var baseUa: String? = null

    fun userAgent(default: String): String {
        if (baseUa == null) baseUa = default
        val ua = baseUa ?: default
        return if (Prefs.desktopMode.value) {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
        } else ua
    }

    fun applyDesktopMode() {
        tabs.forEach { t ->
            t.view?.let {
                it.settings.userAgentString = userAgent(it.settings.userAgentString)
                it.reload()
            }
        }
    }

    fun addHistory(url: String, title: String) {
        if (url.isBlank() || url.startsWith("about:")) return
        history.removeAll { it.url == url }
        history.add(0, Bookmark(url, title))
        while (history.size > 500) history.removeAt(history.lastIndex)
        Prefs.history.value = Ku.json.encodeToString(history.toList())
    }

    fun toggleBookmark(url: String, title: String) {
        if (bookmarks.any { it.url == url }) bookmarks.removeAll { it.url == url } else bookmarks.add(0, Bookmark(url, title.ifBlank { url }))
        Prefs.bookmarks.value = Ku.json.encodeToString(bookmarks.toList())
    }

    fun clearHistory() {
        history.clear()
        Prefs.history.value = "[]"
    }

    fun clearData() {
        clearHistory()
        CookieManager.getInstance().removeAllCookies(null)
        WebStorage.getInstance().deleteAllData()
        tabs.forEach { it.view?.clearCache(true) }
    }

    /** The page's cookies for [url], in the engine's shape (sent with downloads). */
    fun cookies(url: String): List<BrowserCookie> {
        val raw = CookieManager.getInstance().getCookie(url) ?: return emptyList()
        val host = runCatching { URI(url).host }.getOrNull() ?: return emptyList()
        val secure = url.startsWith("https:")
        return raw.split(';').mapNotNull { part ->
            val i = part.indexOf('=')
            if (i <= 0) return@mapNotNull null
            BrowserCookie(name = part.substring(0, i).trim(), value = part.substring(i + 1).trim(), domain = host, path = "/", secure = secure, hostOnly = true)
        }
    }

    /** Typed text → address: a URL as is, anything else searched. */
    fun resolve(input: String): String {
        val s = input.trim()
        if (s.isEmpty()) return s
        if (Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://").containsMatchIn(s) || s.startsWith("about:")) return s
        if (!s.contains(' ') && (s.contains('.') || s.startsWith("localhost")) && !s.endsWith('.')) return "https://$s"
        val engine = digital.kuduy.kudownloader.ui.screens.SEARCH_ENGINES.firstOrNull { it.key == Prefs.searchEngine.value }
            ?: digital.kuduy.kudownloader.ui.screens.SEARCH_ENGINES.first()
        return engine.url.replace("%s", java.net.URLEncoder.encode(s, "UTF-8"))
    }
}

private val VIDEO_PAGE = Regex(
    """^https?://(?:[a-z0-9-]+.)*(?:youtube.com/(?:watch|shorts/|live/|embed/)|youtu.be/|tiktok.com/@[^/]+/video/|instagram.com/(?:reel|reels|p|tv)/|facebook.com/.*(?:/videos/|/reel/|watch)|fb.watch/|(?:x|twitter).com/[^/]+/status/|vimeo.com/d|dailymotion.com/video/|twitch.tv/(?:videos/|[^/]+/clip/)|reddit.com/r/[^/]+/comments/|soundcloud.com/[^/]+/[^/?#]+|bilibili.com/video/|pinterest.[a-z.]+/pin/)""",
    RegexOption.IGNORE_CASE,
)

/** Pages yt-dlp knows as a single video or track. */
fun isVideoPage(url: String) = VIDEO_PAGE.containsMatchIn(url)
