package digital.kuduy.kudownloader.browser

import android.app.DownloadManager
import android.content.Context
import android.content.MutableContextWrapper
import android.graphics.Bitmap
import android.net.Uri
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.Message
import android.view.View
import android.webkit.DownloadListener
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import digital.kuduy.kudownloader.core.GrabLink
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Native
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.GrabPrefill
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.ByteArrayInputStream
import java.util.Locale

private val main = Handler(Looper.getMainLooper())
private fun ui(block: () -> Unit) = main.post(block)

private val MEDIA = Regex("""\.(m3u8|mpd|mp4|webm|m4v|mov|mkv|flv|m4a|mp3|aac|ogg|opus|flac|wav)(?:$|[?#])""", RegexOption.IGNORE_CASE)
private val SEGMENT = Regex("""\.(ts|m4s|aac|cmfv|cmfa)(?:$|[?#])|/seg[-_]?\d+|[?&](range|bytestart)=""", RegexOption.IGNORE_CASE)
private val IMAGE = Regex("""\.(png|jpe?g|gif|webp|svg|ico|bmp|avif)(?:$|[?#])""", RegexOption.IGNORE_CASE)
private val FONT = Regex("""\.(woff2?|ttf|otf|eot)(?:$|[?#])""", RegexOption.IGNORE_CASE)

/** What a pill tap or a media entry hands to the browser screen. */
data class PillPick(val page: String, val src: String?, val title: String)

/** Page-level state the browser screen reacts to. */
object BrowserSignals {
    var pill by mutableStateOf<PillPick?>(null)
    var fullscreen by mutableStateOf<Pair<View, WebChromeClient.CustomViewCallback>?>(null)
    var chooser by mutableStateOf<Pair<ValueCallback<Array<Uri>>, WebChromeClient.FileChooserParams>?>(null)
}

object PageScript {
    @Volatile private var source: String? = null

    fun get(ctx: Context): String = source ?: ctx.assets.open("browser/page.js").bufferedReader().use { it.readText() }.also { source = it }

    fun inject(view: WebView, tab: Tab) {
        val js = get(view.context)
            .replace("__KU_TOKEN__", tab.token)
            .replace("__KU_PILL__", Prefs.pill.value.toString())
            .replace("__KU_LABEL__", org.json.JSONObject.quote(t("Download")))
        view.evaluateJavascript(js, null)
        if (Prefs.adblock.value) cosmetic(view, tab)
    }

    /** Element hiding for this page (the generic part is asked for by the page script). */
    private fun cosmetic(view: WebView, tab: Tab) {
        val url = view.url ?: return
        Thread {
            val c = runCatching { Ku.json.parseToJsonElement(Native.cosmetic(url)).jsonObject }.getOrNull() ?: return@Thread
            val hide = c["hide"]?.jsonArray?.mapNotNull { it.jsonPrimitive.contentOrNull }.orEmpty()
            val generic = c["generic"]?.jsonPrimitive?.contentOrNull == "true"
            val exceptions = c["exceptions"] ?: JsonArray(emptyList())
            val script = c["script"]?.jsonPrimitive?.contentOrNull.orEmpty()
            val css = hide.chunked(200).joinToString("\n") { it.joinToString(",") + "{display:none!important}" }
            val js = "window.__kuCosmetic&&window.__kuCosmetic(${org.json.JSONObject.quote(css)},$generic,${org.json.JSONObject.quote(exceptions.toString())});"
            ui {
                if (tab.view == view) {
                    view.evaluateJavascript(js, null)
                    if (script.isNotBlank()) view.evaluateJavascript(script, null)
                }
            }
        }.start()
    }
}

class KuWebClient(private val tab: Tab) : WebViewClient() {
    override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
        tab.url = url
        tab.progress = 5
        tab.blocked = 0
        tab.media.clear()
        tab.canBack = view.canGoBack()
        tab.canForward = view.canGoForward()
    }

    override fun onPageCommitVisible(view: WebView, url: String) {
        PageScript.inject(view, tab)
    }

    override fun onPageFinished(view: WebView, url: String) {
        tab.url = url
        tab.title = view.title ?: url
        tab.progress = 100
        tab.canBack = view.canGoBack()
        tab.canForward = view.canGoForward()
        PageScript.inject(view, tab)
        BrowserState.addHistory(url, tab.title)
    }

    override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
        // Single-page sites (YouTube) change the address without loading a page.
        tab.url = url
        tab.canBack = view.canGoBack()
        tab.canForward = view.canGoForward()
    }

    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        val u = request.url
        return when (u.scheme?.lowercase(Locale.ROOT)) {
            "http", "https", "about", "data", "blob", "javascript" -> false
            "magnet" -> {
                UiState.add = AddPrefill(url = u.toString(), referer = tab.url, source = "browser")
                true
            }
            "intent", "market", "tel", "mailto", "sms", "geo", "whatsapp", "tg" -> {
                runCatching {
                    val i = if (u.scheme == "intent") android.content.Intent.parseUri(u.toString(), android.content.Intent.URI_INTENT_SCHEME) else android.content.Intent(android.content.Intent.ACTION_VIEW, u)
                    i.addCategory(android.content.Intent.CATEGORY_BROWSABLE)
                    i.component = null
                    i.selector = null
                    view.context.startActivity(i.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
                }
                true
            }
            else -> true
        }
    }

    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url.toString()
        if (!url.startsWith("http")) return null
        val page = tab.url
        if (Prefs.detectMedia.value && !request.isForMainFrame) detect(url, page)
        if (Prefs.adblock.value && !request.isForMainFrame) {
            val kind = kindOf(request, url)
            if (Native.shouldBlock(url, page.ifBlank { url }, kind)) {
                ui {
                    tab.blocked++
                    Prefs.adsBlocked.value = Prefs.adsBlocked.value + 1
                }
                return WebResourceResponse("text/plain", "utf-8", ByteArrayInputStream(ByteArray(0)))
            }
        }
        return null
    }

    private fun detect(url: String, page: String) {
        val m = MEDIA.find(url) ?: return
        if (SEGMENT.containsMatchIn(url) || url.contains("googlevideo.com/videoplayback")) return
        val kind = m.groupValues[1].lowercase(Locale.ROOT)
        val key = url.substringBefore('?')
        ui {
            if (tab.media.none { it.url.substringBefore('?') == key }) {
                tab.media.add(FoundMedia(url, kind, page, tab.title))
                if (tab.media.size > 60) tab.media.removeAt(0)
            }
        }
    }

    private fun kindOf(r: WebResourceRequest, url: String): String {
        val accept = r.requestHeaders["Accept"].orEmpty()
        val path = url.substringBefore('?').lowercase(Locale.ROOT)
        return when {
            path.endsWith(".js") || path.endsWith(".mjs") || accept.contains("javascript") -> "script"
            path.endsWith(".css") || accept.startsWith("text/css") -> "stylesheet"
            IMAGE.containsMatchIn(path) || accept.startsWith("image/") -> "image"
            FONT.containsMatchIn(path) || accept.startsWith("font/") -> "font"
            MEDIA.containsMatchIn(path) -> "media"
            accept.contains("text/html") -> "sub_frame"
            else -> "xmlhttprequest"
        }
    }
}

class KuChromeClient(private val tab: Tab) : WebChromeClient() {
    override fun onProgressChanged(view: WebView, newProgress: Int) {
        tab.progress = newProgress
    }

    override fun onReceivedTitle(view: WebView, title: String?) {
        tab.title = title.orEmpty()
    }

    override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
        if (Prefs.blockPopups.value && !isUserGesture) return false
        val ctx = (view.context as? MutableContextWrapper)?.baseContext ?: view.context
        val t = BrowserState.newTab()
        val nv = BrowserState.webView(t, ctx)
        (resultMsg.obj as? WebView.WebViewTransport)?.webView = nv
        resultMsg.sendToTarget()
        return true
    }

    override fun onCloseWindow(window: WebView) {
        BrowserState.tabs.firstOrNull { it.view == window }?.let { BrowserState.close(it) }
    }

    override fun onShowCustomView(view: View, callback: CustomViewCallback) {
        BrowserSignals.fullscreen = view to callback
    }

    override fun onHideCustomView() {
        BrowserSignals.fullscreen?.second?.onCustomViewHidden()
        BrowserSignals.fullscreen = null
    }

    override fun onShowFileChooser(webView: WebView, filePathCallback: ValueCallback<Array<Uri>>, fileChooserParams: FileChooserParams): Boolean {
        BrowserSignals.chooser?.first?.onReceiveValue(null)
        BrowserSignals.chooser = filePathCallback to fileChooserParams
        return true
    }

    override fun onPermissionRequest(request: PermissionRequest) {
        // Camera, microphone and the like are not offered to pages.
        request.deny()
    }

    override fun onGeolocationPermissionsShowPrompt(origin: String, callback: android.webkit.GeolocationPermissions.Callback) {
        callback.invoke(origin, false, false)
    }
}

/** Files the page wants to download go to KuDownloader (or the system when turned off). */
class KuDownloads(private val tab: Tab) : DownloadListener {
    override fun onDownloadStart(url: String, userAgent: String?, contentDisposition: String?, mimetype: String?, contentLength: Long) {
        if (!url.startsWith("http") && !url.startsWith("ftp")) {
            UiState.toast(t("This file is made by the page itself and can't be downloaded by its address."))
            return
        }
        val name = URLUtil.guessFileName(url, contentDisposition, mimetype)
        if (Prefs.interceptDownloads.value) {
            UiState.add = AddPrefill(
                url = url,
                filename = name,
                referer = tab.url.takeIf { it.isNotBlank() },
                userAgent = userAgent,
                cookies = BrowserState.cookies(url),
                size = contentLength.takeIf { it > 0 },
                mime = mimetype,
                source = "browser",
            )
        } else {
            val ctx = tab.view?.context ?: return
            runCatching {
                val req = DownloadManager.Request(Uri.parse(url))
                    .setMimeType(mimetype)
                    .addRequestHeader("User-Agent", userAgent)
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
                android.webkit.CookieManager.getInstance().getCookie(url)?.let { req.addRequestHeader("Cookie", it) }
                ctx.getSystemService(DownloadManager::class.java).enqueue(req)
            }
        }
    }
}

/** Called by the page script (assets/browser/page.js). */
class PageBridge(private val tab: Tab) {
    private fun ok(token: String) = token == tab.token

    @JavascriptInterface
    fun media(token: String, json: String) {
        if (!ok(token)) return
        val o = runCatching { Ku.json.parseToJsonElement(json).jsonObject }.getOrNull() ?: return
        val src = o["src"]?.jsonPrimitive?.contentOrNull?.takeIf { it.startsWith("http") }
        val page = o["page"]?.jsonPrimitive?.contentOrNull ?: tab.url
        val title = o["title"]?.jsonPrimitive?.contentOrNull ?: tab.title
        ui { BrowserSignals.pill = PillPick(page, src, title) }
    }

    @JavascriptInterface
    fun links(token: String, json: String) {
        if (!ok(token)) return
        val list = runCatching { Ku.json.parseToJsonElement(json).jsonArray }.getOrNull() ?: return
        val links = list.mapNotNull { e ->
            val o = e as? JsonObject ?: return@mapNotNull null
            val u = o["url"]?.jsonPrimitive?.contentOrNull ?: return@mapNotNull null
            val ext = u.substringBefore('?').substringBefore('#').substringAfterLast('/').substringAfterLast('.', "").lowercase(Locale.ROOT).take(8)
            GrabLink(u, o["text"]?.jsonPrimitive?.contentOrNull, ext)
        }.distinctBy { it.url }
        val page = tab.url
        ui {
            UiState.grab = GrabPrefill(page, links, BrowserState.cookies(page), tab.view?.settings?.userAgentString)
            UiState.go(Screen.Fetch)
        }
    }

    /** Generic element-hiding rules for the classes and ids on the page. */
    @JavascriptInterface
    fun hidden(token: String, json: String): String {
        if (!ok(token)) return ""
        return runCatching {
            val sels = Ku.json.parseToJsonElement(Native.hidden(json)).jsonArray.mapNotNull { it.jsonPrimitive.contentOrNull }
            sels.chunked(200).joinToString("\n") { it.joinToString(",") + "{display:none!important}" }
        }.getOrDefault("")
    }
}
