package digital.kuduy.kudownloader.ui.screens

import android.app.Activity
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.browser.BrowserSignals
import digital.kuduy.kudownloader.browser.BrowserState
import digital.kuduy.kudownloader.browser.FoundMedia
import digital.kuduy.kudownloader.browser.Tab
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.ClickRow
import digital.kuduy.kudownloader.ui.MediaPrefill
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Sites on the start page. */
private val QUICK = listOf(
    "YouTube" to "https://m.youtube.com",
    "Instagram" to "https://www.instagram.com",
    "TikTok" to "https://www.tiktok.com",
    "Facebook" to "https://m.facebook.com",
    "X" to "https://x.com",
    "Vimeo" to "https://vimeo.com",
    "Reddit" to "https://www.reddit.com",
    "SoundCloud" to "https://m.soundcloud.com",
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BrowserScreen() {
    val ctx = LocalContext.current
    LaunchedEffect(Unit) { BrowserState.load() }
    val tab = BrowserState.current ?: return
    val focus = LocalFocusManager.current
    var address by remember(tab.id) { mutableStateOf(tab.url) }
    var editing by remember { mutableStateOf(false) }
    // Follow the page's address, but never while the user is typing.
    LaunchedEffect(tab.url, editing) { if (!editing) address = tab.url }
    var menu by remember { mutableStateOf(false) }
    var tabsOpen by remember { mutableStateOf(false) }
    var mediaOpen by remember { mutableStateOf(false) }
    var historyOpen by remember { mutableStateOf(false) }
    val adblock by Prefs.adblock.state.collectAsStateWithLifecycle()

    // A link opened from elsewhere in the app.
    LaunchedEffect(UiState.browserUrl) {
        UiState.browserUrl?.let { u ->
            val t = if (tab.isStart) tab else BrowserState.newTab()
            open(t, u)
            UiState.browserUrl = null
        }
    }
    // Filters are needed before the first page when blocking is on.
    LaunchedEffect(adblock) {
        if (adblock) {
            val loaded = runCatching { Ku.call("adblockStatus").jsonObject["loaded"]?.jsonPrimitive?.contentOrNull == "true" }.getOrDefault(true)
            if (!loaded) runCatching { Ku.call("adblockUpdate") }
        }
    }

    // Pages stop playing and running scripts while another screen is shown.
    DisposableEffect(tab) {
        tab.view?.onResume()
        onDispose {
            tab.view?.onPause()
            // Keep sign-ins when the app is closed right after.
            android.webkit.CookieManager.getInstance().flush()
        }
    }

    BackHandler(enabled = tab.canBack || !tab.isStart) {
        val v = tab.view
        if (v != null && v.canGoBack()) v.goBack() else open(tab, "")
    }

    val chooser = BrowserSignals.chooser
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        BrowserSignals.chooser?.first?.onReceiveValue(uris.toTypedArray())
        BrowserSignals.chooser = null
    }
    LaunchedEffect(chooser) {
        chooser?.let { (_, params) ->
            // Pages may ask for ".jpg" or "image/*,.pdf": the picker needs one MIME type.
            val types = params.acceptTypes.orEmpty().flatMap { it.split(",") }.map { it.trim() }.filter { it.isNotEmpty() }
            val type = types.singleOrNull { "/" in it } ?: types.firstOrNull { "/" in it }?.let { t -> if (types.all { it.substringBefore("/") == t.substringBefore("/") && "/" in it }) t.substringBefore("/") + "/*" else "*/*" } ?: "*/*"
            runCatching { filePicker.launch(type) }.onFailure {
                BrowserSignals.chooser?.first?.onReceiveValue(null)
                BrowserSignals.chooser = null
            }
        }
    }

    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        // Address bar
        Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            if (!editing && !tab.isStart) {
                IconButton({ val v = tab.view; if (v != null && v.canGoBack()) v.goBack() else open(tab, "") }) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, t("Back"))
                }
            }
            TextField(
                if (editing) address else if (tab.isStart) "" else host(tab.url),
                { address = it },
                placeholder = { Text(t("Search or type an address")) },
                singleLine = true,
                leadingIcon = {
                    if (!editing && tab.url.startsWith("https://")) Icon(Icons.Filled.Lock, null, Modifier.size(16.dp))
                    else if (editing || tab.isStart) Icon(Icons.Filled.Search, null, Modifier.size(18.dp))
                },
                trailingIcon = {
                    if (editing && address.isNotEmpty()) IconButton({ address = "" }) { Icon(Icons.Filled.Close, t("Clear")) }
                    else if (!editing && !tab.isStart) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (adblock && tab.blocked > 0) {
                                BadgedBox(badge = { Badge { Text(if (tab.blocked > 99) "99+" else "${tab.blocked}") } }, modifier = Modifier.padding(end = 6.dp)) {
                                    Icon(Icons.Filled.Shield, t("Ads blocked"), tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
                                }
                            }
                            val loading = tab.progress in 1..99
                            IconButton({ if (loading) tab.view?.stopLoading() else tab.view?.reload() }) {
                                Icon(if (loading) Icons.Filled.Close else Icons.Filled.Refresh, if (loading) t("Stop") else t("Reload"), Modifier.size(20.dp))
                            }
                        }
                    }
                },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                keyboardActions = KeyboardActions(onGo = {
                    open(tab, BrowserState.resolve(address))
                    focus.clearFocus()
                }),
                shape = RoundedCornerShape(24.dp),
                colors = TextFieldDefaults.colors(
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                    unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                ),
                modifier = Modifier.weight(1f).height(52.dp).onFocusChanged {
                    editing = it.isFocused
                    if (it.isFocused) address = tab.url
                },
            )
            IconButton({ tabsOpen = true }) {
                Box(Modifier.size(24.dp).clip(RoundedCornerShape(6.dp)).background(MaterialTheme.colorScheme.surfaceContainerHighest), contentAlignment = Alignment.Center) {
                    Text("${BrowserState.tabs.size}", style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                }
            }
            Box {
                IconButton({ menu = true }) { Icon(Icons.Filled.MoreVert, t("More")) }
                BrowserMenu(tab, menu, { menu = false }, onHistory = { historyOpen = true })
            }
        }
        if (tab.progress in 1..99) LinearProgressIndicator(progress = { tab.progress / 100f }, modifier = Modifier.fillMaxWidth().height(2.dp))

        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (tab.isStart) {
                StartPage { open(tab, it) }
            } else {
                androidx.compose.runtime.key(tab.id) {
                    AndroidView(
                        factory = { c ->
                            FrameLayout(c).apply {
                                val v = BrowserState.webView(tab, (c as? Activity) ?: ctx)
                                (v.parent as? ViewGroup)?.removeView(v)
                                addView(v, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                            }
                        },
                        onRelease = { frame -> frame.removeAllViews() },
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                // Always reachable, whatever the page does: a known video page,
                // a player on the page, or a stream it loaded.
                androidx.compose.animation.AnimatedVisibility(
                    tab.canDownload,
                    Modifier.align(Alignment.BottomCenter).padding(bottom = 18.dp),
                    enter = androidx.compose.animation.fadeIn() + androidx.compose.animation.slideInVertically { it },
                    exit = androidx.compose.animation.fadeOut() + androidx.compose.animation.slideOutVertically { it },
                ) {
                    ExtendedFloatingActionButton(
                        onClick = { mediaOpen = true },
                        icon = { Icon(Icons.Filled.Download, null) },
                        text = { Text(if (tab.media.size > 1) tf("Download video ({count})", "count" to tab.media.size) else t("Download video"), fontWeight = FontWeight.SemiBold) },
                        containerColor = MaterialTheme.colorScheme.primary,
                        contentColor = MaterialTheme.colorScheme.onPrimary,
                    )
                }
            }
        }
    }

    BrowserSignals.pill?.let { p -> PillSheet(p.page, p.src, p.title) { BrowserSignals.pill = null } }
    if (mediaOpen) MediaSheet(tab) { mediaOpen = false }
    if (tabsOpen) TabsSheet { tabsOpen = false }
    if (historyOpen) HistorySheet({ historyOpen = false }) { open(tab, it) }
    Fullscreen()
}

private fun host(url: String) = runCatching { java.net.URI(url).host?.removePrefix("www.") }.getOrNull() ?: url

private fun open(tab: Tab, url: String) {
    tab.url = url
    tab.started = url.isNotBlank()
    if (url.isBlank()) {
        tab.title = ""
        tab.media.clear()
        tab.hasVideo = false
        return
    }
    tab.view?.loadUrl(url)
}

@Composable
private fun BrowserMenu(tab: Tab, expanded: Boolean, onDismiss: () -> Unit, onHistory: () -> Unit) {
    val ctx = LocalContext.current
    val desktop by Prefs.desktopMode.state.collectAsStateWithLifecycle()
    val adblock by Prefs.adblock.state.collectAsStateWithLifecycle()
    DropdownMenu(expanded, onDismiss) {
        Row {
            IconButton({ onDismiss(); tab.view?.goBack() }, enabled = tab.canBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, t("Back")) }
            IconButton({ onDismiss(); tab.view?.goForward() }, enabled = tab.canForward) { Icon(Icons.AutoMirrored.Filled.ArrowForward, t("Forward")) }
            IconButton({ onDismiss(); tab.view?.reload() }, enabled = !tab.isStart) { Icon(Icons.Filled.Refresh, t("Reload")) }
            val marked = BrowserState.bookmarks.any { it.url == tab.url }
            IconButton({ onDismiss(); BrowserState.toggleBookmark(tab.url, tab.title) }, enabled = !tab.isStart) {
                Icon(if (marked) Icons.Filled.Bookmark else Icons.Filled.BookmarkBorder, t("Bookmark"))
            }
        }
        HorizontalDivider()
        DropdownMenuItem({ Text(t("Start page")) }, { onDismiss(); open(tab, "") }, leadingIcon = { Icon(Icons.Filled.Home, null) })
        DropdownMenuItem({ Text(t("New tab")) }, { onDismiss(); BrowserState.newTab() }, leadingIcon = { Icon(Icons.Filled.Add, null) })
        if (!tab.isStart) {
            DropdownMenuItem({ Text(t("Download video on this page")) }, {
                onDismiss()
                UiState.media = MediaPrefill(tab.url, BrowserState.cookies(tab.url), tab.url, tab.title)
                UiState.go(Screen.Video)
            }, leadingIcon = { Icon(Icons.Filled.Download, null) })
            DropdownMenuItem({ Text(t("Download all links…")) }, { onDismiss(); tab.view?.evaluateJavascript("window.__kuLinks&&window.__kuLinks()", null) })
            DropdownMenuItem({ Text(t("Share page")) }, { onDismiss(); Files.shareText(ctx, tab.url) })
        }
        DropdownMenuItem({ Text(t("History")) }, { onDismiss(); onHistory() }, leadingIcon = { Icon(Icons.Filled.History, null) })
        DropdownMenuItem(
            { Text(t("Desktop site")) },
            { onDismiss(); Prefs.desktopMode.value = !desktop; BrowserState.applyDesktopMode() },
            trailingIcon = { androidx.compose.material3.Checkbox(desktop, null) },
        )
        DropdownMenuItem(
            { Text(t("Block ads")) },
            { onDismiss(); Prefs.adblock.value = !adblock; tab.view?.reload() },
            trailingIcon = { androidx.compose.material3.Checkbox(adblock, null) },
        )
        DropdownMenuItem({ Text(t("Browser settings")) }, {
            onDismiss()
            UiState.settingsSection = "browser"
            UiState.go(Screen.Settings)
        })
    }
}

@Composable
private fun StartPage(onOpen: (String) -> Unit) {
    val blocked by Prefs.adsBlocked.state.collectAsStateWithLifecycle()
    LazyVerticalGrid(GridCells.Adaptive(88.dp), contentPadding = PaddingValues(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        item(span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }) {
            Column(Modifier.padding(vertical = 12.dp)) {
                Text(t("KuDownloader browser"), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
                Text(t("Tap the KuDownload button on any video to save it. Ads and trackers are blocked."), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (blocked > 0) Text(tf("{count} ads and trackers blocked so far", "count" to blocked), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 6.dp))
            }
        }
        val sites = BrowserState.bookmarks.map { it.title to it.url } + QUICK.filter { q -> BrowserState.bookmarks.none { it.url == q.second } }
        items(sites, key = { it.second }) { (name, url) ->
            Column(Modifier.clip(RoundedCornerShape(12.dp)).clickable { onOpen(url) }.padding(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Box(Modifier.size(52.dp).clip(CircleShape).background(MaterialTheme.colorScheme.primaryContainer), contentAlignment = Alignment.Center) {
                    Text(name.take(1).uppercase(), style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
                }
                Spacer(Modifier.height(6.dp))
                Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
            }
        }
    }
}

/** The KuDownload button was tapped on a video. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PillSheet(page: String, src: String?, title: String, onClose: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onClose) {
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 28.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title.ifBlank { host(page) }, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
            FilledTonalButton(
                {
                    onClose()
                    UiState.media = MediaPrefill(page, BrowserState.cookies(page), page, title)
                    UiState.go(Screen.Video)
                },
                Modifier.fillMaxWidth(),
            ) { Text(t("Choose quality")) }
            if (src != null) {
                OutlinedButton(
                    {
                        onClose()
                        UiState.add = AddPrefill(url = src, referer = page, cookies = BrowserState.cookies(src), source = "browser")
                    },
                    Modifier.fillMaxWidth(),
                ) { Text(t("Download this video file")) }
            }
            androidx.compose.material3.TextButton(
                {
                    onClose()
                    UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(listOf(src ?: page), referer = page, cookies = BrowserState.cookies(src ?: page))
                },
                Modifier.fillMaxWidth(),
            ) { Text(t("Download on a computer…")) }
            Text(t("Choose quality works on YouTube, Instagram, TikTok, X, Facebook and 1,000+ sites."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MediaSheet(tab: Tab, onClose: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onClose) {
        LazyColumn(contentPadding = PaddingValues(bottom = 28.dp)) {
            item {
                Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(tab.title.ifBlank { host(tab.url) }, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    androidx.compose.material3.Button(
                        {
                            onClose()
                            UiState.media = MediaPrefill(tab.url, BrowserState.cookies(tab.url), tab.url, tab.title)
                            UiState.go(Screen.Video)
                        },
                        Modifier.fillMaxWidth().height(52.dp),
                    ) {
                        Icon(Icons.Filled.Download, null)
                        Spacer(Modifier.width(8.dp))
                        Text(t("Choose quality"), fontWeight = FontWeight.SemiBold)
                    }
                    OutlinedButton(
                        {
                            onClose()
                            UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(listOf(tab.url), referer = tab.url, cookies = BrowserState.cookies(tab.url))
                        },
                        Modifier.fillMaxWidth(),
                    ) { Text(t("Download on a computer…")) }
                }
                if (tab.media.isNotEmpty()) {
                    HorizontalDivider()
                    Text(t("Streams on this page"), Modifier.padding(start = 20.dp, top = 14.dp, bottom = 4.dp), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                }
            }
            items(tab.media.toList().asReversed(), key = { it.url }) { m -> MediaRow(m, onClose) }
        }
    }
}

@Composable
private fun MediaRow(m: FoundMedia, onClose: () -> Unit) {
    val stream = m.kind == "m3u8" || m.kind == "mpd"
    ClickRow(
        m.url.substringBefore('?').substringAfterLast('/').ifBlank { m.url },
        (if (stream) t("Stream") else m.kind.uppercase()) + " · " + host(m.url),
    ) {
        onClose()
        if (stream) {
            UiState.media = MediaPrefill(m.url, BrowserState.cookies(m.page), m.page, m.title)
            UiState.go(Screen.Video)
        } else {
            UiState.add = AddPrefill(url = m.url, referer = m.page, cookies = BrowserState.cookies(m.url), source = "browser")
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TabsSheet(onClose: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onClose) {
        LazyColumn(contentPadding = PaddingValues(bottom = 28.dp)) {
            item {
                Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(tf("{count} tabs", "count" to BrowserState.tabs.size), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                    FilledTonalButton({ BrowserState.newTab(); onClose() }, colors = ButtonDefaults.filledTonalButtonColors()) {
                        Icon(Icons.Filled.Add, null)
                        Spacer(Modifier.width(6.dp))
                        Text(t("New tab"))
                    }
                }
            }
            items(BrowserState.tabs.toList(), key = { it.id }) { t0 ->
                Surface(
                    color = if (t0 == BrowserState.current) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent,
                    modifier = Modifier.fillMaxWidth().clickable { BrowserState.current = t0; onClose() },
                ) {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(t0.title.ifBlank { if (t0.isStart) t("Start page") else t0.url }, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (!t0.isStart) Text(host(t0.url), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        IconButton({ BrowserState.close(t0) }) { Icon(Icons.Filled.Close, t("Close tab")) }
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HistorySheet(onClose: () -> Unit, onOpen: (String) -> Unit) {
    ModalBottomSheet(onDismissRequest = onClose) {
        LazyColumn(contentPadding = PaddingValues(bottom = 28.dp)) {
            item {
                Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(t("History"), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                    androidx.compose.material3.TextButton({ BrowserState.clearHistory() }) { Text(t("Clear history")) }
                }
            }
            if (BrowserState.history.isEmpty()) item { Text(t("Nothing here yet."), Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            items(BrowserState.history.toList(), key = { it.url + it.at }) { h ->
                ClickRow(h.title.ifBlank { h.url }, h.url) {
                    onClose()
                    onOpen(h.url)
                }
            }
        }
    }
}

/** Full-screen video from a page. */
@Composable
private fun Fullscreen() {
    val fs = BrowserSignals.fullscreen ?: return
    val activity = LocalContext.current as? Activity
    DisposableEffect(fs) {
        val w = activity?.window
        val c = w?.let { WindowCompat.getInsetsController(it, it.decorView) }
        c?.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        c?.hide(WindowInsetsCompat.Type.systemBars())
        onDispose { c?.show(WindowInsetsCompat.Type.systemBars()) }
    }
    BackHandler {
        fs.second.onCustomViewHidden()
        BrowserSignals.fullscreen = null
    }
    Box(Modifier.fillMaxSize()) {
    AndroidView(
        factory = { c ->
            FrameLayout(c).apply {
                setBackgroundColor(android.graphics.Color.BLACK)
                (fs.first.parent as? ViewGroup)?.removeView(fs.first)
                addView(fs.first, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            }
        },
        modifier = Modifier.fillMaxSize().background(Color.Black),
    )
        // Download while watching full screen.
        BrowserState.current?.let { tab ->
            androidx.compose.material3.FilledTonalButton(
                {
                    fs.second.onCustomViewHidden()
                    BrowserSignals.fullscreen = null
                    UiState.media = MediaPrefill(tab.url, BrowserState.cookies(tab.url), tab.url, tab.title)
                    UiState.go(Screen.Video)
                },
                Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(12.dp),
                colors = ButtonDefaults.filledTonalButtonColors(containerColor = Color.Black.copy(alpha = 0.55f), contentColor = Color.White),
            ) {
                Icon(Icons.Filled.Download, null, Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text(t("Download"))
            }
        }
    }
}

