package digital.kuduy.kudownloader.ui.screens

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.BatteryChargingFull
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.SettingsEthernet
import androidx.compose.material.icons.filled.SmartDisplay
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.I18n
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.ui.ACCENTS
import digital.kuduy.kudownloader.ui.Card
import digital.kuduy.kudownloader.ui.ChoiceRow
import digital.kuduy.kudownloader.ui.ClickRow
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.SectionTitle
import digital.kuduy.kudownloader.ui.SwitchRow
import digital.kuduy.kudownloader.ui.TextRow
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

private data class Section(val id: String, val title: String, val subtitle: String, val icon: androidx.compose.ui.graphics.vector.ImageVector)

@Composable
fun SettingsScreen() {
    val section = UiState.settingsSection
    val sections = listOf(
        Section("appearance", t("Appearance"), t("Theme, colour, language"), Icons.Filled.Palette),
        Section("downloads", t("Downloads"), t("Folder, parallel downloads, file names"), Icons.Filled.Download),
        Section("mobile", t("Mobile data and battery"), t("Wi-Fi only, battery saver"), Icons.Filled.BatteryChargingFull),
        Section("connection", t("Connection"), t("Proxy, retries, timeouts"), Icons.Filled.SettingsEthernet),
        Section("speed", t("Speed limits"), t("Bandwidth profiles"), Icons.Filled.Speed),
        Section("media", t("Video and music"), t("Quality, formats, yt-dlp"), Icons.Filled.SmartDisplay),
        Section("torrent", t("Torrents"), t("Seeding, peers, DHT"), Icons.Filled.Hub),
        Section("browser", t("Browser"), t("Ad blocking, KuDownload button, search"), Icons.Filled.Public),
        Section("notifications", t("Notifications"), t("Finished, failed, queues"), Icons.Filled.Notifications),
        Section("advanced", t("Advanced"), t("Video tools, yt-dlp, log"), Icons.Filled.Build),
    )
    BackHandler(section != null) { UiState.settingsSection = null }
    val current = sections.firstOrNull { it.id == section }
    KuScaffold(current?.title ?: t("Settings"), back = true) { pad ->
        Column(Modifier.fillMaxSize().padding(pad).verticalScroll(rememberScrollState())) {
            when (section) {
                null -> sections.forEach { s -> ClickRow(s.title, s.subtitle, s.icon) { UiState.settingsSection = s.id } }
                "appearance" -> Appearance()
                "downloads" -> DownloadsSettings()
                "mobile" -> MobileSettings()
                "connection" -> ConnectionSettings()
                "speed" -> SpeedSettings()
                "media" -> MediaSettings()
                "torrent" -> TorrentSettings()
                "browser" -> BrowserSettings()
                "notifications" -> NotificationSettings()
                "advanced" -> AdvancedSettings()
            }
            Spacer(Modifier.height(32.dp))
        }
    }
}

/** Current settings as a live value plus a saver. */
@Composable
private fun rememberSettings(): Pair<JsonObject, (Map<String, Any?>) -> Unit> {
    val s by Ku.settings.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    return s to { patch -> scope.act { Ku.saveSettings(patch) } }
}

private fun JsonObject.str(k: String, def: String = "") = (this[k] as? JsonPrimitive)?.contentOrNull ?: def
private fun JsonObject.bool(k: String) = (this[k] as? JsonPrimitive)?.contentOrNull == "true"
private fun JsonObject.num(k: String) = (this[k] as? JsonPrimitive)?.longOrNull ?: 0
private fun JsonObject.dbl(k: String) = (this[k] as? JsonPrimitive)?.contentOrNull?.toDoubleOrNull() ?: 0.0

@Composable
private fun Appearance() {
    val (s, save) = rememberSettings()
    val ctx = LocalContext.current
    val dynamic by Prefs.dynamicColor.state.collectAsStateWithLifecycle()
    ChoiceRow(t("Theme"), s.str("theme", "system"), listOf("system" to t("System"), "light" to t("Light"), "dark" to t("Dark"))) { save(mapOf("theme" to it)) }
    SectionTitle(t("Accent colour"))
    Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        ACCENTS.forEach { (name, color) ->
            val chosen = s.str("accent", "blue") == name && !dynamic
            Box(
                Modifier.size(40.dp).clip(CircleShape).background(color)
                    .border(if (chosen) 3.dp else 0.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                    .clickable {
                        Prefs.dynamicColor.value = false
                        save(mapOf("accent" to name))
                    },
                contentAlignment = Alignment.Center,
            ) { if (chosen) Icon(Icons.Filled.Check, null, tint = Color.White) }
        }
    }
    if (Build.VERSION.SDK_INT >= 31) SwitchRow(t("Colours from the wallpaper"), dynamic, t("Material You")) { Prefs.dynamicColor.value = it }
    ChoiceRow(t("Dark palette"), s.str("darkPalette", "default"), listOf("default" to t("Default"), "midnight" to t("Midnight"), "black" to t("Black (OLED)"))) { save(mapOf("darkPalette" to it)) }
    SectionTitle(t("Language"))
    ChoiceRow(
        t("Language"),
        s.str("language", "system"),
        listOf("system" to t("System")) + I18n.LANGUAGES,
    ) { code ->
        save(mapOf("language" to code))
        I18n.load(ctx, code)
        I18n.pushToEngine()
        digital.kuduy.kudownloader.service.Notifier.channels(ctx)
        (ctx as? Activity)?.recreate()
    }
    Text(t("Translations are shared with KuDownloader for the desktop."), Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun DownloadsSettings() {
    val (s, save) = rememberSettings()
    val ctx = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let { Files.treeToPath(it) }?.let { save(mapOf("downloadDir" to it)) }
    }
    var anywhere by remember { mutableStateOf(Files.canWriteAnywhere()) }
    LaunchedEffect(Unit) { anywhere = Files.canWriteAnywhere() }
    ClickRow(t("Download folder"), s.str("downloadDir")) { picker.launch(null) }
    TextButton({ save(mapOf("downloadDir" to Ku.downloadDir().absolutePath)) }, Modifier.padding(horizontal = 8.dp)) { Text(t("Use Download/KuDownloader")) }
    if (Build.VERSION.SDK_INT >= 30) {
        SwitchRow(
            t("Save to any folder"),
            anywhere,
            t("Without this, Android only lets KuDownloader save inside the Download folder."),
        ) {
            runCatching { ctx.startActivity(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + ctx.packageName))) }
                .onFailure { ctx.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)) }
        }
    }
    SwitchRow(t("Sort into folders by file type"), s.bool("useCategories"), t("Video, Music, Documents, Archives…")) { save(mapOf("useCategories" to it)) }
    ChoiceRow(t("Downloads at the same time"), s.num("maxConcurrent").toString(), (1..8).map { it.toString() to it.toString() }) { save(mapOf("maxConcurrent" to it.toInt())) }
    ChoiceRow(
        t("Connections per download"),
        s.num("defaultConnections").toString(),
        listOf("0" to t("Smart")) + listOf(1, 2, 4, 8, 12, 16).map { it.toString() to it.toString() },
    ) { save(mapOf("defaultConnections" to it.toInt())) }
    ChoiceRow(
        t("When the file already exists"),
        s.str("fileExists", "rename"),
        listOf("rename" to t("Keep both (rename)"), "overwrite" to t("Replace"), "skip" to t("Skip")),
    ) { save(mapOf("fileExists" to it)) }
    ChoiceRow(
        t("HTTP engine"),
        s.str("httpEngine", "kuhttp"),
        listOf("kuhttp" to t("KuHTTP (built in, recommended)"), "aria2" to "aria2"),
    ) { save(mapOf("httpEngine" to it)) }
    SwitchRow(t("Offer copied links"), Prefs.clipboardOffer.state.collectAsStateWithLifecycle().value, t("When you open KuDownloader with a link on the clipboard.")) { Prefs.clipboardOffer.value = it }
}

@Composable
private fun MobileSettings() {
    val ctx = LocalContext.current
    val wifi by Prefs.wifiOnly.state.collectAsStateWithLifecycle()
    val saver by Prefs.pauseOnBatterySaver.state.collectAsStateWithLifecycle()
    val perf by Prefs.highPerfWifi.state.collectAsStateWithLifecycle()
    SwitchRow(t("Download on Wi-Fi only"), wifi, t("Downloads pause on mobile data and continue on Wi-Fi.")) { Prefs.wifiOnly.value = it }
    SwitchRow(t("Pause in battery saver"), saver) { Prefs.pauseOnBatterySaver.value = it }
    SwitchRow(t("Full Wi-Fi speed"), perf, t("Keeps Wi-Fi at full speed while downloading (uses a little more battery).")) { Prefs.highPerfWifi.value = it }
    val pm = ctx.getSystemService(PowerManager::class.java)
    var exempt by remember { mutableStateOf(pm.isIgnoringBatteryOptimizations(ctx.packageName)) }
    LaunchedEffect(Unit) { exempt = pm.isIgnoringBatteryOptimizations(ctx.packageName) }
    SwitchRow(
        t("Run in the background without limits"),
        exempt,
        t("Some phones stop long downloads to save battery. Allow KuDownloader to keep running."),
    ) {
        runCatching {
            @Suppress("BatteryLife")
            ctx.startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + ctx.packageName)))
        }.onFailure { ctx.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
    }
}

@Composable
private fun ConnectionSettings() {
    val (s, save) = rememberSettings()
    TextRow(t("Proxy"), s.str("proxy"), placeholder = "http://host:port, socks5://host:port") { save(mapOf("proxy" to it)) }
    TextRow(t("Proxy user name"), s.str("proxyUser")) { save(mapOf("proxyUser" to it)) }
    TextRow(t("Proxy password"), s.str("proxyPass"), subtitle = if (s.str("proxyPass").isEmpty()) t("Not set") else "••••••") { save(mapOf("proxyPass" to it)) }
    TextRow(t("No proxy for"), s.str("noProxy"), placeholder = "localhost, 192.168.0.0/16") { save(mapOf("noProxy" to it)) }
    TextRow(t("User agent"), s.str("userAgent"), placeholder = t("Default")) { save(mapOf("userAgent" to it)) }
    TextRow(t("Tries per download"), s.num("maxTries").toString(), number = true) { save(mapOf("maxTries" to (it.toLongOrNull() ?: 5))) }
    TextRow(t("Wait between tries (seconds)"), s.num("retryWait").toString(), number = true) { save(mapOf("retryWait" to (it.toLongOrNull() ?: 5))) }
    TextRow(t("Automatic retries after a failure"), s.num("autoRetry").toString(), number = true) { save(mapOf("autoRetry" to (it.toLongOrNull() ?: 0))) }
    TextRow(t("Timeout (seconds)"), s.num("timeout").toString(), number = true) { save(mapOf("timeout" to (it.toLongOrNull() ?: 60))) }
    TextRow(t("Connect timeout (seconds)"), s.num("connectTimeout").toString(), number = true) { save(mapOf("connectTimeout" to (it.toLongOrNull() ?: 30))) }
    SwitchRow(t("Check server certificates"), s.bool("checkCertificate"), t("Turn off only for servers you trust.")) { save(mapOf("checkCertificate" to it)) }
}

@Composable
private fun SpeedSettings() {
    val (s, save) = rememberSettings()
    val profiles = (s["profiles"] as? JsonArray)?.map { it.jsonObject }.orEmpty()
    val active = s.str("activeProfile", "unlimited")
    ChoiceRow(t("Active profile"), active, profiles.map { (it.str("id")) to t(it.str("name")) }) { save(mapOf("activeProfile" to it)) }
    profiles.forEachIndexed { i, p ->
        SectionTitle(t(p.str("name")))
        fun update(key: String, kb: String) {
            val v = (kb.toLongOrNull() ?: 0) * 1024
            val list = profiles.mapIndexed { j, o -> if (j == i) JsonObject(o + (key to JsonPrimitive(v))) else o }
            save(mapOf("profiles" to JsonArray(list)))
        }
        TextRow(t("Download limit (KB/s)"), (p.num("download") / 1024).toString(), subtitle = if (p.num("download") == 0L) t("Unlimited") else Fmt.speed(p.num("download")), number = true) { update("download", it) }
        TextRow(t("Upload limit (KB/s)"), (p.num("upload") / 1024).toString(), subtitle = if (p.num("upload") == 0L) t("Unlimited") else Fmt.speed(p.num("upload")), number = true) { update("upload", it) }
    }
}

@Composable
private fun MediaSettings() {
    val (s, save) = rememberSettings()
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var info by remember { mutableStateOf<JsonObject?>(null) }
    var updating by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { info = runCatching { Ku.call("engineInfo").jsonObject }.getOrNull() }
    ChoiceRow(t("Preferred video quality"), s.num("videoHeight").toString(), listOf(2160, 1440, 1080, 720, 480, 360).map { it.toString() to "${it}p" }) { save(mapOf("videoHeight" to it.toInt())) }
    ChoiceRow(t("Video format"), s.str("videoContainer", "mp4"), listOf("mp4" to t("MP4 (most compatible)"), "mkv" to "MKV", "webm" to "WebM")) { save(mapOf("videoContainer" to it)) }
    ChoiceRow(t("Audio format"), s.str("audioFormat", "m4a"), audioFormats()) { save(mapOf("audioFormat" to it)) }
    ChoiceRow(t("Audio bitrate"), s.num("audioBitrate").toString(), listOf(320, 256, 192, 128).map { it.toString() to "$it kbps" }) { save(mapOf("audioBitrate" to it.toInt())) }
    SwitchRow(t("Download subtitles"), s.bool("subtitles")) { save(mapOf("subtitles" to it)) }
    TextRow(t("Subtitle languages"), s.str("subLangs"), placeholder = "en, bn") { save(mapOf("subLangs" to it)) }
    SwitchRow(t("Embed the thumbnail as cover art"), s.bool("embedThumbnail")) { save(mapOf("embedThumbnail" to it)) }
    SectionTitle(t("Tools"))
    val yt = info?.get("ytdlp")?.jsonObject
    ClickRow("yt-dlp", yt?.get("version")?.jsonPrimitive?.contentOrNull ?: if (Ku.tools?.ytdlp == null) t("Unavailable") else t("Checking…")) {}
    ClickRow("FFmpeg", if (Ku.tools?.ffmpeg != null) t("Included") else t("Unavailable")) {}
    ClickRow("aria2", if (Ku.tools?.aria2 != null) t("Included") else t("Unavailable")) {}
    TextButton(
        {
            scope.launch {
                updating = true
                try {
                    UiState.toast(Ku.updateYtdlp(ctx, force = true)?.let { tf("yt-dlp {version} installed", "version" to it) } ?: t("yt-dlp is up to date."))
                    info = runCatching { Ku.call("engineInfo").jsonObject }.getOrNull()
                } catch (e: Exception) {
                    UiState.toast(e.message ?: "")
                } finally {
                    updating = false
                }
            }
        },
        enabled = !updating && Ku.tools?.ytdlp != null,
        modifier = Modifier.padding(horizontal = 8.dp),
    ) { Text(if (updating) t("Updating…") else t("Update yt-dlp")) }
    Text(t("Sites change often; update yt-dlp when a video stops downloading."), Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun TorrentSettings() {
    val (s, save) = rememberSettings()
    TextRow(t("Stop seeding at ratio"), s.dbl("seedRatio").toString(), placeholder = "1.0") { save(mapOf("seedRatio" to (it.toDoubleOrNull() ?: 1.0))) }
    TextRow(t("Stop seeding after (minutes)"), s.num("seedTime").toString(), number = true) { save(mapOf("seedTime" to (it.toLongOrNull() ?: 0))) }
    TextRow(t("Listening port"), s.str("btListenPort"), placeholder = "6881-6999") { save(mapOf("btListenPort" to it)) }
    TextRow(t("Maximum peers"), s.num("btMaxPeers").toString(), number = true) { save(mapOf("btMaxPeers" to (it.toLongOrNull() ?: 55))) }
    SwitchRow(t("DHT (find peers without trackers)"), s.bool("enableDht")) { save(mapOf("enableDht" to it)) }
}

@Composable
private fun BrowserSettings() {
    val scope = rememberCoroutineScope()
    val adblock by Prefs.adblock.state.collectAsStateWithLifecycle()
    var status by remember { mutableStateOf<JsonObject?>(null) }
    var updating by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { status = runCatching { Ku.call("adblockStatus").jsonObject }.getOrNull() }
    fun updateLists() {
        scope.launch {
            updating = true
            try {
                val r = Ku.call("adblockUpdate").jsonObject
                UiState.toast(tf("{count} filter rules loaded", "count" to (r["rules"]?.jsonPrimitive?.longOrNull ?: 0)))
                status = runCatching { Ku.call("adblockStatus").jsonObject }.getOrNull()
            } catch (e: Exception) {
                UiState.toast(e.message ?: "")
            } finally {
                updating = false
            }
        }
    }
    SwitchRow(t("Block ads and trackers"), adblock, t("EasyList, EasyPrivacy and annoyance filters.")) {
        Prefs.adblock.value = it
        if (it && status?.get("loaded")?.jsonPrimitive?.contentOrNull != "true") updateLists()
    }
    val rules = status?.get("rules")?.jsonPrimitive?.longOrNull ?: 0
    val updated = status?.get("updated")?.jsonPrimitive?.longOrNull
    ClickRow(t("Update filter lists"), if (updating) t("Updating…") else if (rules > 0) tf("{count} rules · {when}", "count" to rules, "when" to Fmt.relative(updated)) else t("Not downloaded yet")) { if (!updating) updateLists() }
    ClickRow(t("Ads blocked"), Prefs.adsBlocked.state.collectAsStateWithLifecycle().value.toString()) {}
    SwitchRow(t("KuDownload button on videos"), Prefs.pill.state.collectAsStateWithLifecycle().value) { Prefs.pill.value = it }
    SwitchRow(t("Detect media on pages"), Prefs.detectMedia.state.collectAsStateWithLifecycle().value, t("Lists the videos and audio a page plays.")) { Prefs.detectMedia.value = it }
    SwitchRow(t("Download files with KuDownloader"), Prefs.interceptDownloads.state.collectAsStateWithLifecycle().value, t("Files the browser would download open the Add sheet.")) { Prefs.interceptDownloads.value = it }
    SwitchRow(t("Block pop-ups"), Prefs.blockPopups.state.collectAsStateWithLifecycle().value) { Prefs.blockPopups.value = it }
    SwitchRow(t("Desktop sites"), Prefs.desktopMode.state.collectAsStateWithLifecycle().value) { Prefs.desktopMode.value = it }
    ChoiceRow(t("Search engine"), Prefs.searchEngine.state.collectAsStateWithLifecycle().value, SEARCH_ENGINES.map { it.key to it.name }) { Prefs.searchEngine.value = it }
    TextRow(t("Home page"), Prefs.homePage.state.collectAsStateWithLifecycle().value, placeholder = t("Start page")) { Prefs.homePage.value = it }
    ClickRow(t("Clear browsing data"), t("History, cookies, cache and site storage")) {
        digital.kuduy.kudownloader.browser.BrowserState.clearData()
        UiState.toast(t("Browsing data cleared"))
    }
}

@Composable
private fun NotificationSettings() {
    val (s, save) = rememberSettings()
    val ctx = LocalContext.current
    SwitchRow(t("When a download finishes"), s.bool("notifyComplete")) { save(mapOf("notifyComplete" to it)) }
    SwitchRow(t("When a download fails"), s.bool("notifyError")) { save(mapOf("notifyError" to it)) }
    SwitchRow(t("When a queue finishes"), s.bool("notifyQueueDone")) { save(mapOf("notifyQueueDone" to it)) }
    ClickRow(t("System notification settings"), t("Sounds, pop-ups and channels")) {
        val i = if (Build.VERSION.SDK_INT >= 26) Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, ctx.packageName)
        else Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + ctx.packageName))
        runCatching { ctx.startActivity(i) }
    }
}

data class SearchEngine(val key: String, val name: String, val url: String)

val SEARCH_ENGINES = listOf(
    SearchEngine("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    SearchEngine("google", "Google", "https://www.google.com/search?q=%s"),
    SearchEngine("bing", "Bing", "https://www.bing.com/search?q=%s"),
    SearchEngine("brave", "Brave Search", "https://search.brave.com/search?q=%s"),
    SearchEngine("startpage", "Startpage", "https://www.startpage.com/do/search?q=%s"),
    SearchEngine("yandex", "Yandex", "https://yandex.com/search/?text=%s"),
)


/** The video tools (yt-dlp with its Python, FFmpeg, aria2): status, repair, update; the log. */
@Composable
fun AdvancedSettings() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val tools by Ku.toolsState.collectAsStateWithLifecycle()
    val repair by Ku.toolsRepair.collectAsStateWithLifecycle()
    var version by remember { mutableStateOf<String?>(null) }
    var updating by remember { mutableStateOf(false) }
    LaunchedEffect(tools) {
        version = runCatching { Ku.call("engineInfo").jsonObject["ytdlp"]?.jsonObject?.get("version")?.jsonPrimitive?.contentOrNull }.getOrNull()
    }
    SectionTitle(t("Video tools"))
    val ok = t("Ready")
    val missing = t("Missing")
    ClickRow("yt-dlp", if (tools?.ytdlp != null) listOfNotNull(ok, version).joinToString(" · ") else missing) {}
    ClickRow("Python", if (tools?.python != null) ok else missing) {}
    ClickRow("FFmpeg", if (tools?.ffmpeg != null) ok else missing) {}
    ClickRow("aria2", if (tools?.aria2 != null) ok else missing) {}
    tools?.problems?.takeIf { it.isNotEmpty() && tools?.videoReady != true }?.let { p ->
        Text(p.joinToString("\n"), Modifier.padding(horizontal = 16.dp, vertical = 6.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
    }
    repair?.let { (done, total) ->
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
            Text(if (done > 0) t("Downloading yt-dlp…") + " " + Fmt.size(done) + (if (total > 0) " / " + Fmt.size(total) else "") else t("Checking…"), style = MaterialTheme.typography.bodySmall)
            if (total > 0) androidx.compose.material3.LinearProgressIndicator(progress = { (done.toFloat() / total).coerceIn(0f, 1f) }, modifier = Modifier.padding(top = 6.dp).fillMaxWidth())
            else androidx.compose.material3.LinearProgressIndicator(Modifier.padding(top = 6.dp).fillMaxWidth())
        }
    }
    Row(Modifier.padding(horizontal = 8.dp)) {
        TextButton(
            {
                scope.launch {
                    try {
                        val r = Ku.repairTools(ctx)
                        UiState.toast(if (r.videoReady) t("Video tools are ready.") else r.problems.joinToString("; "))
                    } catch (e: Exception) {
                        UiState.toast(e.message ?: "")
                    }
                }
            },
            enabled = repair == null,
        ) { Text(if (tools?.videoReady == true) t("Repair video tools") else t("Install video tools")) }
        TextButton(
            {
                scope.launch {
                    updating = true
                    try {
                        UiState.toast(Ku.updateYtdlp(ctx, force = true)?.let { tf("yt-dlp {version} installed", "version" to it) } ?: t("yt-dlp is up to date."))
                        version = runCatching { Ku.call("engineInfo").jsonObject["ytdlp"]?.jsonObject?.get("version")?.jsonPrimitive?.contentOrNull }.getOrNull()
                    } catch (e: Exception) {
                        UiState.toast(e.message ?: "")
                    } finally {
                        updating = false
                    }
                }
            },
            enabled = !updating && tools?.videoReady == true,
        ) { Text(if (updating) t("Updating…") else t("Update yt-dlp")) }
    }
    SectionTitle(t("Diagnostics"))
    ClickRow(t("Share the log"), t("Send it with a bug report")) {
        val log = java.io.File(ctx.filesDir, "kucore/kudownloader.log")
        if (log.isFile) Files.share(ctx, log) else UiState.toast(t("Nothing logged yet."))
    }
}

