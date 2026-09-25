package digital.kuduy.kudownloader.ui.screens

import android.util.Base64
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.SelectAll
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material.icons.filled.WifiOff
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.toMutableStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil.compose.AsyncImage
import digital.kuduy.kudownloader.core.Download
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.service.NetworkWatch
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.Card
import digital.kuduy.kudownloader.ui.ConfirmDialog
import digital.kuduy.kudownloader.ui.EmptyState
import digital.kuduy.kudownloader.ui.FileGlyph
import digital.kuduy.kudownloader.ui.LocalKuColors
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.SpeedGraph
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File

private enum class Filter(val label: String) { All("All"), Active("Active"), Unfinished("Unfinished"), Finished("Finished"), Failed("Failed") }

/** Run an engine request and show its error, if any. */
fun CoroutineScope.act(block: suspend () -> Unit) = launch {
    try {
        block()
    } catch (e: Exception) {
        UiState.toast(e.message ?: t("Something went wrong."))
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DownloadsScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val map by Ku.downloads.collectAsStateWithLifecycle()
    val stats by Ku.stats.collectAsStateWithLifecycle()
    val history by Ku.speedHistory.collectAsStateWithLifecycle()
    val queues by Ku.queues.collectAsStateWithLifecycle()
    val settings by Ku.settings.collectAsStateWithLifecycle()
    var filter by rememberSaveable { mutableStateOf(Filter.All) }
    var category by rememberSaveable { mutableStateOf<String?>(null) }
    var queue by rememberSaveable { mutableStateOf<String?>(null) }
    var search by rememberSaveable { mutableStateOf<String?>(null) }
    val selected: SnapshotStateList<String> = remember { emptyList<String>().toMutableStateList() }
    var menu by remember { mutableStateOf(false) }
    var confirmRemove by remember { mutableStateOf<List<String>?>(null) }
    var moveTo by remember { mutableStateOf<List<String>?>(null) }

    val torrentPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.act {
            val bytes = withContext(Dispatchers.IO) { Files.readBytes(ctx, uri) } ?: throw Exception(t("This file could not be read."))
            val (info, data) = Ku.torrentInfo(Base64.encodeToString(bytes, Base64.NO_WRAP))
            UiState.add = AddPrefill(url = "magnet:?xt=urn:btih:${info.infoHash}", filename = info.name, torrent = info, torrentData = data, size = info.total, source = "torrent")
        }
    }

    val categories = (settings["categories"] as? JsonArray)?.mapNotNull { c ->
        val o = c.jsonObject
        val id = o["id"]?.jsonPrimitive?.contentOrNull ?: return@mapNotNull null
        id to t(o["name"]?.jsonPrimitive?.contentOrNull ?: id)
    }.orEmpty()

    val list = map.values.asSequence()
        .filter {
            when (filter) {
                Filter.All -> true
                Filter.Active -> it.isRunning || it.status == "queued"
                Filter.Unfinished -> !it.isFinished
                Filter.Finished -> it.isFinished
                Filter.Failed -> it.status == "error"
            }
        }
        .filter { category == null || it.category == category }
        .filter { queue == null || (it.queueId ?: "main") == queue }
        .filter { s -> search.isNullOrBlank() || s.name.contains(search!!, true) || s.url.contains(search!!, true) }
        .sortedWith(compareByDescending<Download> { it.isRunning }.thenByDescending { it.createdAt })
        .toList()

    val selecting = selected.isNotEmpty()
    Scaffold(
        contentWindowInsets = androidx.compose.foundation.layout.WindowInsets(0, 0, 0, 0),
        topBar = {
            when {
                selecting -> TopAppBar(
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
                    navigationIcon = { IconButton({ selected.clear() }) { Icon(Icons.Filled.Close, t("Cancel")) } },
                    title = { Text(tf("{count} selected", "count" to selected.size)) },
                    actions = {
                        IconButton({ selected.clear(); selected.addAll(list.map { it.id }) }) { Icon(Icons.Filled.SelectAll, t("Select all")) }
                        IconButton({ val ids = selected.toList(); scope.act { Ku.resume(ids); KuService.ensure(ctx) } }) { Icon(Icons.Filled.PlayArrow, t("Resume")) }
                        IconButton({ val ids = selected.toList(); scope.act { Ku.pause(ids) } }) { Icon(Icons.Filled.Pause, t("Pause")) }
                        IconButton({ confirmRemove = selected.toList() }) { Icon(Icons.Filled.Delete, t("Remove")) }
                        Box {
                            var more by remember { mutableStateOf(false) }
                            IconButton({ more = true }) { Icon(Icons.Filled.MoreVert, t("More")) }
                            DropdownMenu(more, { more = false }) {
                                DropdownMenuItem({ Text(t("Download again")) }, { more = false; val ids = selected.toList(); scope.act { Ku.redownload(ids); KuService.ensure(ctx) } })
                                DropdownMenuItem({ Text(t("Move to queue…")) }, { more = false; moveTo = selected.toList() })
                                DropdownMenuItem({ Text(t("Share links")) }, {
                                    more = false
                                    Files.shareText(ctx, selected.mapNotNull { map[it]?.url }.joinToString("\n"))
                                })
                                DropdownMenuItem({ Text(t("Download on a computer…")) }, {
                                    more = false
                                    val sel = selected.mapNotNull { map[it] }
                                    UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(sel.map { it.url }, sel.singleOrNull()?.name)
                                    selected.clear()
                                })
                                DropdownMenuItem({ Text(t("Send links with KuAirSend")) }, {
                                    more = false
                                    UiState.airShareText = selected.mapNotNull { map[it]?.url }.joinToString("\n")
                                    selected.clear()
                                    UiState.go(Screen.AirSend)
                                })
                            }
                        }
                    },
                )
                search != null -> TopAppBar(
                    navigationIcon = { IconButton({ search = null }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, t("Back")) } },
                    title = {
                        TextField(
                            search!!,
                            { search = it },
                            placeholder = { Text(t("Search downloads")) },
                            singleLine = true,
                            colors = TextFieldDefaults.colors(focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent),
                            modifier = Modifier.fillMaxWidth(),
                        )
                    },
                )
                else -> TopAppBar(
                    title = { Text(t("Downloads"), fontWeight = FontWeight.SemiBold) },
                    actions = {
                        IconButton({ search = "" }) { Icon(Icons.Filled.Search, t("Search")) }
                        Box {
                            IconButton({ menu = true }) { Icon(Icons.Filled.MoreVert, t("More")) }
                            DropdownMenu(menu, { menu = false }) {
                                DropdownMenuItem({ Text(t("Resume all")) }, { menu = false; scope.act { Ku.resumeAll(); KuService.ensure(ctx) } })
                                DropdownMenuItem({ Text(t("Stop all")) }, { menu = false; scope.act { Ku.pauseAll() } })
                                DropdownMenuItem({ Text(t("Delete all completed")) }, { menu = false; scope.act { Ku.clearFinished() } })
                                HorizontalDivider()
                                DropdownMenuItem({ Text(t("Add batch download…")) }, { menu = false; UiState.go(Screen.Batch) })
                                DropdownMenuItem({ Text(t("Open .torrent file…")) }, { menu = false; torrentPicker.launch(arrayOf("application/x-bittorrent", "application/octet-stream", "*/*")) })
                                DropdownMenuItem({ Text(t("Open download folder")) }, {
                                    menu = false
                                    Files.openFolder(ctx, File(Ku.settingString("downloadDir", Ku.downloadDir().absolutePath)))
                                })
                            }
                        }
                    },
                )
            }
        },
        floatingActionButton = {
            if (!selecting) {
                ExtendedFloatingActionButton(
                    onClick = { UiState.add = AddPrefill() },
                    icon = { Icon(Icons.Filled.Add, null) },
                    text = { Text(t("Add URL")) },
                )
            }
        },
    ) { pad ->
        LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(bottom = 96.dp)) {
            item(key = "speed") { SpeedCard(stats.downloadSpeed, stats.uploadSpeed, stats.active, history) }
            NetworkWatch.reason()?.let { why ->
                item(key = "hold") {
                    Card {
                        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Filled.WifiOff, null, tint = LocalKuColors.current.warning)
                            Spacer(Modifier.width(12.dp))
                            Text(why, style = MaterialTheme.typography.bodyMedium)
                        }
                    }
                }
            }
            item(key = "filters") {
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Filter.entries.forEach { f -> FilterChip(filter == f, { filter = f }, { Text(t(f.label)) }) }
                    var catOpen by remember { mutableStateOf(false) }
                    Box {
                        FilterChip(category != null, { catOpen = true }, { Text(categories.firstOrNull { it.first == category }?.second ?: t("Category")) })
                        DropdownMenu(catOpen, { catOpen = false }) {
                            DropdownMenuItem({ Text(t("All categories")) }, { category = null; catOpen = false })
                            categories.forEach { (id, name) -> DropdownMenuItem({ Text(name) }, { category = id; catOpen = false }) }
                        }
                    }
                    if (queues.size > 1) {
                        var qOpen by remember { mutableStateOf(false) }
                        Box {
                            FilterChip(queue != null, { qOpen = true }, { Text(queues.firstOrNull { it.id == queue }?.let { queueName(it.id, it.name) } ?: t("Queue")) })
                            DropdownMenu(qOpen, { qOpen = false }) {
                                DropdownMenuItem({ Text(t("All queues")) }, { queue = null; qOpen = false })
                                queues.forEach { q -> DropdownMenuItem({ Text(queueName(q.id, q.name)) }, { queue = q.id; qOpen = false }) }
                            }
                        }
                    }
                }
            }
            if (list.isEmpty()) {
                item(key = "empty") {
                    EmptyState(
                        Icons.Filled.Download,
                        if (map.isEmpty()) t("No downloads yet") else t("Nothing here"),
                        if (map.isEmpty()) t("Tap Add URL, share a link to KuDownloader, or browse to a file.") else t("No download matches these filters."),
                    ) {
                        if (map.isEmpty()) {
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                androidx.compose.material3.FilledTonalButton({
                                    val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                                    val text = cm?.primaryClip?.getItemAt(0)?.coerceToText(ctx)?.toString()?.trim().orEmpty()
                                    UiState.add = AddPrefill(url = text.takeIf { digital.kuduy.kudownloader.ui.looksLikeLink(it) } ?: "")
                                }) { Text(t("Paste link")) }
                                androidx.compose.material3.OutlinedButton({ UiState.go(Screen.Browser) }) { Text(t("Browser")) }
                                androidx.compose.material3.OutlinedButton({ UiState.go(Screen.Video) }) { Text(t("Video")) }
                            }
                        }
                    }
                }
            }
            items(list, key = { it.id }) { d ->
                DownloadRow(
                    d,
                    selected = d.id in selected,
                    selecting = selecting,
                    onClick = {
                        if (selecting) {
                            if (d.id in selected) selected.remove(d.id) else selected.add(d.id)
                        } else {
                            UiState.details = d.id
                        }
                    },
                    onLongClick = { if (d.id !in selected) selected.add(d.id) },
                    onAction = {
                        scope.act {
                            when {
                                d.isFinished -> if (!Files.open(ctx, File(d.path))) UiState.toast(t("No app on this phone can open this file."))
                                Ku.heldByQueue(d) -> {
                                    Ku.startNow(listOf(d.id))
                                    KuService.ensure(ctx)
                                }
                                d.isRunning || d.status == "queued" -> Ku.pause(listOf(d.id))
                                else -> {
                                    Ku.resume(listOf(d.id))
                                    KuService.ensure(ctx)
                                }
                            }
                        }
                    },
                )
            }
        }
    }

    confirmRemove?.let { ids ->
        var deleteFiles by remember { mutableStateOf(false) }
        ConfirmDialog(
            title = if (ids.size == 1) t("Remove download?") else tf("Remove {count} downloads?", "count" to ids.size),
            text = t("They are removed from the list."),
            confirm = t("Remove"),
            danger = true,
            onDismiss = { confirmRemove = null },
            extra = {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 8.dp)) {
                    Checkbox(deleteFiles, { deleteFiles = it })
                    Text(t("Also delete the files"))
                }
            },
        ) {
            confirmRemove = null
            selected.clear()
            scope.act { Ku.remove(ids, deleteFiles) }
        }
    }
    moveTo?.let { ids ->
        QueuePicker(onDismiss = { moveTo = null }) { q ->
            moveTo = null
            selected.clear()
            scope.act { Ku.moveToQueue(ids, q) }
        }
    }
}

fun queueName(id: String, name: String) = if (id == "main" && name == "Main queue") t("Main queue") else name

@Composable
fun QueuePicker(onDismiss: () -> Unit, onPick: (String?) -> Unit) {
    val queues by Ku.queues.collectAsStateWithLifecycle()
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(t("Move to queue")) },
        text = {
            Column {
                queues.forEach { q ->
                    Text(
                        queueName(q.id, q.name),
                        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).combinedClickableCompat { onPick(q.id) }.padding(12.dp),
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = { androidx.compose.material3.TextButton(onDismiss) { Text(t("Cancel")) } },
    )
}

@OptIn(ExperimentalFoundationApi::class)
fun Modifier.combinedClickableCompat(onLong: (() -> Unit)? = null, onClick: () -> Unit) = this.combinedClickable(onLongClick = onLong, onClick = onClick)

@Composable
private fun SpeedCard(down: Long, up: Long, active: Int, history: List<Pair<Long, Long>>) {
    val scope = rememberCoroutineScope()
    val settings by Ku.settings.collectAsStateWithLifecycle()
    val profile = settings["activeProfile"]?.jsonPrimitive?.contentOrNull ?: "unlimited"
    val profiles = (settings["profiles"] as? JsonArray)?.map { it.jsonObject }.orEmpty()
    Card {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(Fmt.speed(down), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
                    val parts = mutableListOf(if (active > 0) tf("{count} downloading", "count" to active) else t("Idle"))
                    if (up > 0) parts += "↑ ${Fmt.speed(up)}"
                    Text(parts.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                var open by remember { mutableStateOf(false) }
                Box {
                    androidx.compose.material3.AssistChip(
                        onClick = { open = true },
                        label = { Text(t(profiles.firstOrNull { it["id"]?.jsonPrimitive?.contentOrNull == profile }?.get("name")?.jsonPrimitive?.contentOrNull ?: "Unlimited")) },
                        leadingIcon = { Icon(Icons.Filled.Speed, null, Modifier.size(18.dp)) },
                    )
                    DropdownMenu(open, { open = false }) {
                        profiles.forEach { p ->
                            val id = p["id"]?.jsonPrimitive?.contentOrNull ?: return@forEach
                            val name = t(p["name"]?.jsonPrimitive?.contentOrNull ?: id)
                            val limit = p["download"]?.jsonPrimitive?.contentOrNull?.toLongOrNull() ?: 0
                            DropdownMenuItem(
                                text = { Text(if (limit > 0) "$name · ${Fmt.speed(limit)}" else name, fontWeight = if (id == profile) FontWeight.SemiBold else FontWeight.Normal) },
                                onClick = { open = false; scope.act { Ku.setProfile(id) } },
                            )
                        }
                    }
                }
            }
            // The graph only while something moves (or just did): more room for the list.
            if (active > 0 || history.any { it.first > 0 || it.second > 0 }) {
                Spacer(Modifier.height(10.dp))
                SpeedGraph(history, Modifier.fillMaxWidth().height(64.dp))
            }
        }
    }
}

@Composable
fun DownloadRow(d: Download, selected: Boolean, selecting: Boolean, onClick: () -> Unit, onLongClick: () -> Unit, onAction: () -> Unit) {
    val ku = LocalKuColors.current
    Surface(
        color = if (selected) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent,
        modifier = Modifier.fillMaxWidth().combinedClickableCompat(onLong = onLongClick, onClick = onClick),
    ) {
        Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (selecting) {
                Checkbox(selected, { onClick() })
            } else if (d.meta.thumbnail != null && d.isMedia) {
                AsyncImage(d.meta.thumbnail, null, contentScale = ContentScale.Crop, modifier = Modifier.size(width = 56.dp, height = 40.dp).clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.surfaceVariant))
            } else {
                FileGlyph(d)
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(d.meta.mediaTitle?.takeIf { d.isMedia && d.name.isBlank() } ?: d.name.ifBlank { d.url }, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyLarge)
                if (!d.isFinished && d.status != "error") {
                    Spacer(Modifier.height(6.dp))
                    if (d.total > 0 || d.status != "downloading") {
                        LinearProgressIndicator(progress = { d.progress }, modifier = Modifier.fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp)), color = if (d.status == "paused") MaterialTheme.colorScheme.outline else MaterialTheme.colorScheme.primary)
                    } else {
                        LinearProgressIndicator(Modifier.fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp)))
                    }
                }
                Spacer(Modifier.height(4.dp))
                Text(statusLine(d), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = if (d.status == "error") ku.danger else MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (!selecting) {
                IconButton(onAction) {
                    when {
                        d.isFinished -> Icon(Icons.Filled.FolderOpen, t("Open"), tint = MaterialTheme.colorScheme.primary)
                        Ku.heldByQueue(d) -> Icon(Icons.Filled.PlayArrow, t("Start now"), tint = MaterialTheme.colorScheme.primary)
                        d.isRunning || d.status == "queued" -> Icon(Icons.Filled.Pause, t("Pause"))
                        d.status == "error" -> Icon(Icons.Filled.ErrorOutline, t("Retry"), tint = ku.danger)
                        else -> Icon(Icons.Filled.PlayArrow, t("Resume"), tint = MaterialTheme.colorScheme.primary)
                    }
                }
            } else if (d.isFinished) {
                Icon(Icons.Filled.CheckCircle, null, tint = ku.success)
            }
        }
    }
}

fun statusLine(d: Download): String {
    val parts = mutableListOf<String>()
    when (d.status) {
        "downloading", "processing" -> {
            parts += if (d.total > 0) "${Fmt.size(d.done)} / ${Fmt.size(d.total)}" else Fmt.size(d.done)
            if (d.status == "processing") parts += t("Processing") else parts += Fmt.speed(d.speed)
            d.etaSeconds?.let { parts += tf("{time} left", "time" to Fmt.duration(it)) }
        }
        "seeding" -> {
            parts += t("Seeding")
            parts += "↑ ${Fmt.speed(d.uploadSpeed)}"
            parts += Fmt.size(d.total)
        }
        "completed" -> {
            parts += Fmt.size(d.total.takeIf { it > 0 } ?: d.done)
            parts += Fmt.relative(d.completedAt)
        }
        "error" -> parts += te(d.error ?: t("Error"))
        else -> {
            parts += if (Ku.heldByQueue(d)) tf("Waiting for {queue}", "queue" to (Ku.queues.value.firstOrNull { it.id == d.queueId }?.let { queueName(it.id, it.name) } ?: "")) else Fmt.status(d)
            if (d.total > 0) parts += "${Fmt.size(d.done)} / ${Fmt.size(d.total)}" else if (d.done > 0) parts += Fmt.size(d.done)
        }
    }
    return parts.filter { it.isNotBlank() }.joinToString(" · ")
}

@Suppress("unused")
private fun JsonObject.str(k: String) = this[k]?.jsonPrimitive?.contentOrNull
