package digital.kuduy.kudownloader.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.SmartDisplay
import androidx.compose.material.icons.filled.Videocam
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil.compose.AsyncImage
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.MediaInfo
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.EmptyState
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.UiState
import digital.kuduy.kudownloader.ui.normalizeUrl
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.put

fun audioFormats() = listOf(
    "m4a" to t("M4A (AAC) · fast"),
    "best" to t("Original · fastest"),
    "opus" to t("Opus · fast"),
    "mp3" to t("MP3 · converts, slower"),
    "flac" to t("FLAC · converts, slower"),
)

private fun pickDefault(info: MediaInfo, preferred: Int): Int? {
    val h = info.video.map { it.height }
    if (h.isEmpty()) return null
    return h.firstOrNull { it <= preferred } ?: h.last()
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun VideoScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val prefill = UiState.media
    val queues by Ku.queues.collectAsStateWithLifecycle()
    val tools by Ku.toolsState.collectAsStateWithLifecycle()
    val repair by Ku.toolsRepair.collectAsStateWithLifecycle()
    var url by rememberSaveable { mutableStateOf(prefill?.url ?: "") }
    var playlist by rememberSaveable { mutableStateOf(false) }
    var info by remember { mutableStateOf<MediaInfo?>(null) }
    var loading by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var mode by rememberSaveable { mutableStateOf("video") }
    var height by remember { mutableStateOf<Int?>(null) }
    var bitrate by remember { androidx.compose.runtime.mutableIntStateOf(Ku.settingLong("audioBitrate", 320).toInt()) }
    var container by remember { mutableStateOf(Ku.settingString("videoContainer", "mp4")) }
    var audioFormat by remember { mutableStateOf(Ku.settingString("audioFormat", "m4a")) }
    var subs by remember { mutableStateOf(Ku.settingBool("subtitles", false)) }
    val subLangs = remember { mutableStateListOf<String>().apply { addAll(Ku.settingString("subLangs", "en").split(',', ' ').filter { it.isNotBlank() }) } }
    var embedThumb by remember { mutableStateOf(Ku.settingBool("embedThumbnail", false)) }
    val items = remember { mutableStateListOf<Int>() }
    var busy by remember { mutableStateOf(false) }
    var queueId by remember { mutableStateOf<String?>(null) }

    fun analyze() {
        val u = normalizeUrl(url)
        if (u.isBlank()) return
        scope.launch {
            loading = true
            error = null
            info = null
            try {
                val i = Ku.analyze(u, playlist, prefill?.cookies.orEmpty(), prefill?.referer)
                info = i
                height = pickDefault(i, Ku.settingLong("videoHeight", 1080).toInt())
                items.clear()
                items.addAll(i.entries.map { it.index })
                if (i.video.isEmpty() && i.audio.isNotEmpty()) mode = "audio"
            } catch (e: Exception) {
                error = e.message
            } finally {
                loading = false
            }
        }
    }

    // A link handed over from the browser, a share or the Add sheet.
    LaunchedEffect(prefill) {
        if (prefill != null) {
            url = prefill.url
            if (prefill.auto) analyze()
        }
    }

    fun download(start: Boolean) {
        val i = info ?: return
        scope.launch {
            busy = true
            try {
                val media = buildJsonObject {
                    put("mode", mode)
                    if (mode == "video") height?.let { put("height", it) }
                    if (mode == "audio") put("audioBitrate", bitrate)
                    if (mode == "video") put("container", container) else if (i.ffmpegAvailable) put("container", audioFormat)
                    put("subtitles", subs && mode == "video")
                    put("subLangs", subLangs.joinToString(","))
                    put("embedSubtitles", subs)
                    put("writeThumbnail", false)
                    put("embedThumbnail", embedThumb)
                    put("playlist", i.isPlaylist)
                    if (i.isPlaylist && items.size < i.entries.size) put("playlistItems", items.sorted().joinToString(","))
                }
                val size = if (i.isPlaylist) null else if (mode == "video") i.video.firstOrNull { it.height == height }?.size else (i.audio.firstOrNull { it.bitrate == bitrate } ?: i.audio.firstOrNull())?.size
                val req = buildJsonObject {
                    put("url", i.webpageUrl.ifBlank { normalizeUrl(url) })
                    put("media", media)
                    put("cookies", Ku.json.encodeToJsonElement(prefill?.cookies.orEmpty()))
                    prefill?.referer?.let { put("referer", it) }
                    (queueId ?: if (!start) "main" else null)?.let { put("queueId", it) }
                    put("title", i.title)
                    i.thumbnail?.let { put("thumbnail", it) }
                    size?.let { put("sizeHint", it) }
                    put("source", if (prefill != null) "browser" else "android")
                }
                Ku.addMedia(req)
                KuService.ensure(ctx)
                UiState.toast(tf("Downloading {name}", "name" to i.title))
                UiState.media = null
            } catch (e: Exception) {
                UiState.toast(e.message ?: t("Could not start the download"))
            } finally {
                busy = false
            }
        }
    }

    KuScaffold(t("Video downloader")) { pad ->
        LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item {
                OutlinedTextField(
                    url,
                    { url = it },
                    label = { Text(t("Video or playlist address")) },
                    placeholder = { Text("https://…") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { analyze() }),
                    trailingIcon = {
                        Row {
                            IconButton({
                                val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                                cm?.primaryClip?.getItemAt(0)?.coerceToText(ctx)?.toString()?.trim()?.let { url = it; analyze() }
                            }) { Icon(Icons.Filled.ContentPaste, t("Paste")) }
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (url.contains("list=") || playlist) {
                        Switch(playlist, { playlist = it })
                        Spacer(Modifier.width(8.dp))
                        Text(t("Whole playlist"), Modifier.weight(1f))
                    } else {
                        Spacer(Modifier.weight(1f))
                    }
                    Button({ analyze() }, enabled = url.isNotBlank() && !loading) {
                        Icon(Icons.Filled.Search, null)
                        Spacer(Modifier.width(6.dp))
                        Text(t("Find formats"))
                    }
                }
            }
            if (loading) {
                item {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(8.dp)) {
                        CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.width(12.dp))
                        Text(t("Reading available formats…"))
                    }
                }
            }
            error?.let { e -> item { Notice(e) } }
            if (tools?.videoReady != true) {
                item {
                    digital.kuduy.kudownloader.ui.Card {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(t("Video tools are not installed yet"), style = MaterialTheme.typography.titleSmall)
                            Text(t("KuDownloader needs yt-dlp to read video sites. It is about 3 MB."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            repair?.let { (done, total) ->
                                if (total > 0) androidx.compose.material3.LinearProgressIndicator(progress = { (done.toFloat() / total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                                else androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth())
                            }
                            Button(
                                {
                                    scope.launch {
                                        try {
                                            val r = Ku.repairTools(ctx)
                                            if (r.videoReady) {
                                                error = null
                                                UiState.toast(t("Video tools are ready."))
                                            } else {
                                                UiState.toast(r.problems.joinToString("; "))
                                            }
                                        } catch (e: Exception) {
                                            UiState.toast(e.message ?: "")
                                        }
                                    }
                                },
                                enabled = repair == null,
                            ) { Text(t("Install video tools")) }
                        }
                    }
                }
            }
            val i = info
            if (i == null && !loading && error == null) {
                item {
                    EmptyState(
                        Icons.Filled.SmartDisplay,
                        t("Download videos and music"),
                        t("Paste a link from YouTube, Instagram, TikTok, X, Facebook, Vimeo and 1,000+ other sites, or tap the KuDownload button in the built-in browser."),
                    )
                }
            }
            if (i != null) {
                item {
                    Row {
                        i.thumbnail?.let {
                            AsyncImage(it, null, contentScale = ContentScale.Crop, modifier = Modifier.width(140.dp).aspectRatio(16f / 9f).clip(RoundedCornerShape(10.dp)).background(MaterialTheme.colorScheme.surfaceVariant))
                            Spacer(Modifier.width(12.dp))
                        }
                        Column(Modifier.weight(1f)) {
                            Text(i.title, style = MaterialTheme.typography.titleMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
                            val facts = listOfNotNull(i.uploader, i.duration?.let { Fmt.clock(it) }, i.extractor.takeIf { it.isNotBlank() }, if (i.isLive) t("Live") else null, if (i.isPlaylist) tf("{count} videos", "count" to (i.playlistCount ?: i.entries.size)) else null)
                            Text(facts.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                if (!i.ffmpegAvailable) item { Notice(t("FFmpeg is unavailable: only formats that need no merging can be downloaded."), MaterialTheme.colorScheme.tertiary) }
                item {
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                        SegmentedButton(mode == "video", { mode = "video" }, SegmentedButtonDefaults.itemShape(0, 2), icon = { Icon(Icons.Filled.Videocam, null) }, enabled = i.video.isNotEmpty()) { Text(t("Video")) }
                        SegmentedButton(mode == "audio", { mode = "audio" }, SegmentedButtonDefaults.itemShape(1, 2), icon = { Icon(Icons.Filled.MusicNote, null) }) { Text(t("Audio only")) }
                    }
                }
                if (mode == "video") {
                    items(i.video, key = { "v${it.height}" }) { v ->
                        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).clickable { height = v.height }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                            RadioButton(height == v.height, { height = v.height })
                            Column(Modifier.weight(1f)) {
                                Text(buildString {
                                    append(v.label.ifBlank { "${v.height}p" })
                                    v.fps?.takeIf { it > 30 }?.let { append(" ${it.toInt()}fps") }
                                    if (v.hdr) append(" HDR")
                                }, fontWeight = FontWeight.Medium)
                                Text(listOf(v.ext.uppercase(), v.vcodec).filter { it.isNotBlank() }.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            Text(v.size?.let { "≈ ${Fmt.size(it)}" } ?: "", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    item {
                        ChoiceLine(t("Container"), container, listOf("mp4" to t("MP4 (most compatible)"), "mkv" to "MKV", "webm" to "WebM")) { container = it }
                    }
                    val manual = i.subtitles
                    val auto = i.autoSubtitles.filter { it !in manual && it.length <= 3 }.take(20)
                    item {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(subs, { subs = it }, enabled = manual.isNotEmpty() || auto.isNotEmpty())
                            Text(t("Download and embed subtitles") + if (manual.isEmpty() && auto.isEmpty()) " " + t("(none available)") else "")
                        }
                        if (subs) {
                            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                (manual.map { it to false } + auto.map { it to true }).forEach { (l, isAuto) ->
                                    FilterChip(l in subLangs, { if (l in subLangs) subLangs.remove(l) else subLangs.add(l) }, { Text(if (isAuto) "$l (auto)" else l) })
                                }
                            }
                        }
                    }
                } else {
                    item {
                        ChoiceLine(t("Audio format"), audioFormat, audioFormats()) { audioFormat = it }
                    }
                    if (audioFormat == "mp3" || audioFormat == "flac" || !i.ffmpegAvailable) {
                        item {
                            ChoiceLine(t("Audio bitrate"), bitrate.toString(), listOf(320, 256, 192, 128).map { it.toString() to "$it kbps" }) { bitrate = it.toInt() }
                        }
                    }
                    if (i.audio.isEmpty()) item { Text(t("No audio track found."), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(embedThumb, { embedThumb = it })
                        Text(t("Embed the thumbnail as cover art"))
                    }
                }
                if (i.isPlaylist && i.entries.isNotEmpty()) {
                    item {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(items.size == i.entries.size, { on -> items.clear(); if (on) items.addAll(i.entries.map { it.index }) })
                            Text(tf("{count} of {total} selected", "count" to items.size, "total" to i.entries.size), style = MaterialTheme.typography.titleSmall)
                        }
                    }
                    items(i.entries, key = { "e${it.index}" }) { e ->
                        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable { if (e.index in items) items.remove(e.index) else items.add(e.index) }) {
                            Checkbox(e.index in items, { on -> if (on) items.add(e.index) else items.remove(e.index) })
                            Text("${e.index}", style = MaterialTheme.typography.labelSmall, modifier = Modifier.width(28.dp))
                            Text(e.title, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(e.duration?.let { Fmt.clock(it) } ?: "", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                if (queues.size > 1) {
                    item {
                        ChoiceLine(t("Queue"), queueId ?: "", listOf("" to t("Start now")) + queues.map { it.id to queueName(it.id, it.name) }) { queueId = it.ifBlank { null } }
                    }
                }
                item {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
                        OutlinedButton({ download(false) }, Modifier.weight(1f), enabled = !busy) { Text(t("Add to queue")) }
                        OutlinedButton(
                            { UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(listOf(i.webpageUrl.ifBlank { normalizeUrl(url) }), referer = prefill?.referer, cookies = prefill?.cookies.orEmpty()) },
                            enabled = !busy,
                        ) { Icon(Icons.Filled.Computer, t("Download on a computer…")) }
                        Button({ download(true) }, Modifier.weight(1f), enabled = !busy && (mode == "audio" || height != null || i.video.isEmpty())) { Text(t("Download"), fontWeight = FontWeight.SemiBold) }
                    }
                }
            }
        }
    }
}

@Composable
fun ChoiceLine(label: String, value: String, options: List<Pair<String, String>>, onPick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
        Box {
            TextButton({ open = true }) { Text(options.firstOrNull { it.first == value }?.second ?: value) }
            DropdownMenu(open, { open = false }) {
                options.forEach { (v, l) -> DropdownMenuItem({ Text(l) }, { open = false; onPick(v) }) }
            }
        }
    }
}

