package digital.kuduy.kudownloader.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Download
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.LogLine
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.ConfirmDialog
import digital.kuduy.kudownloader.ui.Danger
import digital.kuduy.kudownloader.ui.FileGlyph
import digital.kuduy.kudownloader.ui.LocalKuColors
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.Pill
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DetailsSheet(id: String, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val map by Ku.downloads.collectAsStateWithLifecycle()
    val d = map[id]
    if (d == null) {
        LaunchedEffect(Unit) { onClose() }
        return
    }
    val ku = LocalKuColors.current
    var tab by remember { mutableStateOf(0) }
    var details by remember { mutableStateOf<JsonObject?>(null) }
    var confirmRemove by remember { mutableStateOf(false) }
    var edit by remember { mutableStateOf(false) }
    var verify by remember { mutableStateOf(false) }

    // Live engine details (connections, peers, log) while the sheet is open.
    LaunchedEffect(id) {
        while (true) {
            details = runCatching { Ku.details(id) }.getOrNull()
            delay(2000)
        }
    }

    ModalBottomSheet(onDismissRequest = onClose, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp).navigationBarsPadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                FileGlyph(d, 48)
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(d.name.ifBlank { d.url }, style = MaterialTheme.typography.titleMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 4.dp)) {
                        Pill(Fmt.status(d), if (d.status == "error") ku.danger else if (d.isFinished) ku.success else MaterialTheme.colorScheme.primary)
                        Pill(engineName(d.engine), MaterialTheme.colorScheme.onSurfaceVariant)
                        d.meta.verified?.let { Pill(t("Verified"), ku.success) }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))

            if (!d.isFinished) {
                LinearProgressIndicator(progress = { d.progress }, modifier = Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)))
                Spacer(Modifier.height(8.dp))
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Stat(t("Downloaded"), if (d.total > 0) "${Fmt.size(d.done)} / ${Fmt.size(d.total)}" else Fmt.size(d.done))
                Stat(t("Speed"), if (d.isRunning) Fmt.speed(d.speed) else "—")
                Stat(t("Time left"), if (d.isRunning) Fmt.duration(d.etaSeconds) else "—")
            }
            if (d.isTorrent) {
                Spacer(Modifier.height(8.dp))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Stat(t("Uploaded"), Fmt.size(d.uploaded))
                    Stat(t("Upload speed"), Fmt.speed(d.uploadSpeed))
                    Stat(t("Seeders"), d.meta.numSeeders?.toString() ?: "—")
                }
            }
            d.error?.let {
                Spacer(Modifier.height(10.dp))
                Notice(te(it))
            }
            d.meta.smartNote?.let {
                Spacer(Modifier.height(8.dp))
                Text(te(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }

            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.horizontalScroll(rememberScrollState())) {
                when {
                    d.isFinished -> {
                        Button({ if (!Files.open(ctx, File(d.path))) UiState.toast(t("No app on this phone can open this file.")) }) {
                            Icon(Icons.Filled.OpenInNew, null)
                            Spacer(Modifier.width(6.dp))
                            Text(t("Open"))
                        }
                        FilledTonalButton({ Files.share(ctx, File(d.path)) }) { Icon(Icons.Filled.Share, null); Spacer(Modifier.width(6.dp)); Text(t("Share")) }
                        FilledTonalButton({ Files.openFolder(ctx, File(d.path).parentFile ?: File(d.dir)) }) { Icon(Icons.Filled.FolderOpen, null); Spacer(Modifier.width(6.dp)); Text(t("Folder")) }
                        FilledTonalButton({ scope.act { Ku.redownload(listOf(d.id)); KuService.ensure(ctx) } }) { Icon(Icons.Filled.Refresh, null); Spacer(Modifier.width(6.dp)); Text(t("Download again")) }
                    }
                    d.isRunning || d.status == "queued" -> Button({ scope.act { Ku.pause(listOf(d.id)) } }) { Icon(Icons.Filled.Pause, null); Spacer(Modifier.width(6.dp)); Text(t("Pause")) }
                    else -> {
                        Button({ scope.act { Ku.resume(listOf(d.id)); KuService.ensure(ctx) } }) { Icon(Icons.Filled.PlayArrow, null); Spacer(Modifier.width(6.dp)); Text(if (d.status == "error") t("Retry") else t("Resume")) }
                        FilledTonalButton({ scope.act { Ku.redownload(listOf(d.id)); KuService.ensure(ctx) } }) { Icon(Icons.Filled.Refresh, null); Spacer(Modifier.width(6.dp)); Text(t("Start over")) }
                    }
                }
                FilledTonalButton({ edit = true }) { Icon(Icons.Filled.Edit, null); Spacer(Modifier.width(6.dp)); Text(t("Edit")) }
                if (d.isFinished) FilledTonalButton({ verify = true }) { Icon(Icons.Filled.VerifiedUser, null); Spacer(Modifier.width(6.dp)); Text(t("Checksum")) }
                Button(
                    { confirmRemove = true },
                    colors = ButtonDefaults.buttonColors(containerColor = Danger, contentColor = Color.White),
                ) { Icon(Icons.Filled.Delete, null); Spacer(Modifier.width(6.dp)); Text(t("Remove")) }
            }

            Spacer(Modifier.height(12.dp))
            val tabs = buildList {
                add(t("Info"))
                if (d.isTorrent || (details?.get("files") as? JsonArray)?.isNotEmpty() == true) add(t("Files"))
                if (d.isTorrent) add(t("Peers"))
                if (!d.isTorrent && !d.isMedia) add(t("Connections"))
                add(t("Log"))
            }
            if (tab >= tabs.size) tab = 0
            TabRow(tab) { tabs.forEachIndexed { i, s -> Tab(tab == i, { tab = i }, text = { Text(s, maxLines = 1) }) } }
            Column(Modifier.heightIn(min = 160.dp, max = 360.dp).verticalScroll(rememberScrollState()).padding(vertical = 10.dp)) {
                when (tabs[tab]) {
                    t("Info") -> InfoTab(d)
                    t("Files") -> FilesTab(d, details)
                    t("Peers") -> PeersTab(details)
                    t("Connections") -> ConnectionsTab(d, details)
                    t("Log") -> LogTab(details)
                }
            }
            Spacer(Modifier.height(12.dp))
        }
    }

    if (confirmRemove) {
        var deleteFiles by remember { mutableStateOf(false) }
        ConfirmDialog(
            t("Remove download?"),
            d.name,
            t("Remove"),
            danger = true,
            onDismiss = { confirmRemove = false },
            extra = {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 8.dp)) {
                    Checkbox(deleteFiles, { deleteFiles = it })
                    Text(t("Also delete the files"))
                }
            },
        ) {
            confirmRemove = false
            onClose()
            scope.act { Ku.remove(listOf(d.id), deleteFiles) }
        }
    }
    if (edit) EditDialog(d) { edit = false }
    if (verify) VerifyDialog(d) { verify = false }
}

private fun engineName(e: String) = when (e) {
    "kuhttp" -> "KuHTTP"
    "ytdlp" -> "yt-dlp"
    else -> "aria2"
}

@Composable
private fun Stat(label: String, value: String) {
    Column {
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun InfoRow(label: String, value: String, copy: Boolean = false) {
    val ctx = LocalContext.current
    Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.Top) {
        Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(110.dp))
        SelectionContainer(Modifier.weight(1f)) { Text(value, style = MaterialTheme.typography.bodySmall) }
        if (copy) {
            Icon(
                Icons.Filled.ContentCopy,
                t("Copy"),
                modifier = Modifier.padding(start = 6.dp).clip(RoundedCornerShape(4.dp)).combinedClickableCompat {
                    ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(label, value))
                    UiState.toast(t("Copied"))
                },
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun InfoTab(d: Download) {
    InfoRow(t("Address"), d.url, copy = true)
    InfoRow(t("Saved to"), d.path, copy = true)
    d.meta.mediaTitle?.let { InfoRow(t("Title"), it) }
    d.meta.uploader?.let { InfoRow(t("Uploader"), it) }
    d.meta.duration?.let { InfoRow(t("Duration"), Fmt.clock(it)) }
    d.meta.infoHash?.let { InfoRow(t("Info hash"), it, copy = true) }
    d.meta.mime?.let { InfoRow(t("Type"), it) }
    d.meta.resumable?.let { InfoRow(t("Resumable"), if (it) t("Yes") else t("No")) }
    InfoRow(t("Connections"), if (d.connections == 0) t("Smart") else d.connections.toString())
    InfoRow(t("Added"), Fmt.date(d.createdAt))
    d.completedAt?.let { InfoRow(t("Completed"), Fmt.date(it)) }
    d.meta.verified?.let { InfoRow(t("Checksum"), it) }
    if (d.meta.retries > 0) InfoRow(t("Retries"), d.meta.retries.toString())
}

@Composable
private fun FilesTab(d: Download, details: JsonObject?) {
    val live = (details?.get("files") as? JsonArray)?.mapNotNull { it as? JsonObject }
    if (live != null && live.isNotEmpty()) {
        live.forEach { f ->
            val path = f["path"]?.jsonPrimitive?.contentOrNull.orEmpty().substringAfterLast('/')
            val len = f["length"]?.jsonPrimitive?.contentOrNull?.toLongOrNull() ?: 0
            val done = f["completedLength"]?.jsonPrimitive?.contentOrNull?.toLongOrNull() ?: 0
            val sel = f["selected"]?.jsonPrimitive?.contentOrNull != "false"
            FileLine(path, len, done, sel)
        }
    } else {
        d.meta.files.forEach { FileLine(it.path.substringAfterLast('/'), it.length, it.completed, it.selected) }
    }
}

@Composable
private fun FileLine(name: String, len: Long, done: Long, selected: Boolean) {
    Column(Modifier.padding(vertical = 6.dp)) {
        Row {
            Text(name, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium, color = if (selected) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.outline)
            Text(Fmt.size(len), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (selected && len > 0) LinearProgressIndicator(progress = { (done.toFloat() / len).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth().padding(top = 4.dp).height(3.dp))
    }
}

@Composable
private fun PeersTab(details: JsonObject?) {
    val peers = (details?.get("peers") as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
    if (peers.isEmpty()) {
        Text(t("No peers connected."), color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    peers.forEach { p ->
        Row(Modifier.padding(vertical = 4.dp)) {
            Text("${p["ip"]?.jsonPrimitive?.contentOrNull}:${p["port"]?.jsonPrimitive?.contentOrNull}", Modifier.weight(1f), style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
            if (p["seeder"]?.jsonPrimitive?.contentOrNull == "true") Pill(t("Seeder"), LocalKuColors.current.success)
            Spacer(Modifier.width(8.dp))
            Text("↓ ${Fmt.speed(p["downloadSpeed"]?.jsonPrimitive?.longOrNull ?: 0)}  ↑ ${Fmt.speed(p["uploadSpeed"]?.jsonPrimitive?.longOrNull ?: 0)}", style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun ConnectionsTab(d: Download, details: JsonObject?) {
    val segments = (details?.get("segments") as? JsonArray)?.mapNotNull { it as? JsonObject }
    val conns = (details?.get("connections") as? JsonArray)?.mapNotNull { it as? JsonObject }
    Text(tf("{count} active connections", "count" to d.activeConnections), style = MaterialTheme.typography.bodyMedium)
    Spacer(Modifier.height(8.dp))
    if (!segments.isNullOrEmpty()) {
        // KuHTTP: one bar per segment.
        segments.forEachIndexed { i, s ->
            val start = s["start"]?.jsonPrimitive?.longOrNull ?: 0
            val end = s["end"]?.jsonPrimitive?.longOrNull ?: 0
            val done = s["downloaded"]?.jsonPrimitive?.longOrNull ?: 0
            val len = (end - start).coerceAtLeast(1)
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 3.dp)) {
                Text("#${i + 1}", style = MaterialTheme.typography.labelSmall, modifier = Modifier.width(32.dp))
                LinearProgressIndicator(progress = { (done.toFloat() / len).coerceIn(0f, 1f) }, modifier = Modifier.weight(1f).height(6.dp).clip(RoundedCornerShape(3.dp)))
                Spacer(Modifier.width(8.dp))
                Text(Fmt.size(done), style = MaterialTheme.typography.labelSmall)
            }
        }
    } else if (!conns.isNullOrEmpty()) {
        conns.forEach { c ->
            Row(Modifier.padding(vertical = 3.dp)) {
                Text(c["currentUri"]?.jsonPrimitive?.contentOrNull ?: c["uri"]?.jsonPrimitive?.contentOrNull ?: "", Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall)
                Text(Fmt.speed(c["speed"]?.jsonPrimitive?.longOrNull ?: 0), style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun LogTab(details: JsonObject?) {
    val lines: List<LogLine> = (details?.get("log") as? JsonArray)?.let { runCatching { Ku.json.decodeFromJsonElement<List<LogLine>>(it) }.getOrNull() }.orEmpty()
    if (lines.isEmpty()) {
        Text(t("Nothing logged yet."), color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    val ku = LocalKuColors.current
    SelectionContainer {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.surfaceContainerHighest).padding(10.dp)) {
            lines.takeLast(200).forEach { l ->
                val time = java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.ROOT).format(java.util.Date(l.ts))
                Text(
                    "$time  ${te(l.message)}",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = when (l.level) {
                        "error" -> ku.danger
                        "warn" -> ku.warning
                        else -> MaterialTheme.colorScheme.onSurface
                    },
                )
            }
        }
    }
}

@Composable
private fun EditDialog(d: Download, onClose: () -> Unit) {
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var url by remember { mutableStateOf(d.url) }
    var name by remember { mutableStateOf(d.name) }
    var connections by remember { mutableStateOf(d.connections.toFloat()) }
    val current = d.options?.get("speedLimit")?.jsonPrimitive?.longOrNull ?: 0
    var limit by remember { mutableStateOf(if (current > 0) (current / 1024).toString() else "") }
    var checksum by remember { mutableStateOf(d.options?.get("checksum")?.jsonPrimitive?.contentOrNull ?: "") }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(t("Edit download")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                if (!d.isRunning) {
                    OutlinedTextField(url, { url = it }, label = { Text(t("Address (refresh an expired link)")) }, singleLine = true)
                    if (d.done == 0L) OutlinedTextField(name, { name = it }, label = { Text(t("File name")) }, singleLine = true)
                }
                Text(if (connections < 1) t("Connections: Smart") else tf("Connections: {count}", "count" to connections.toInt()), style = MaterialTheme.typography.labelLarge)
                Slider(connections, { connections = it }, valueRange = 0f..16f, steps = 15)
                OutlinedTextField(limit, { limit = it.filter { c -> c.isDigit() } }, label = { Text(t("Speed limit (KB/s)")) }, placeholder = { Text(t("Unlimited")) }, singleLine = true)
                OutlinedTextField(checksum, { checksum = it }, label = { Text(t("Checksum")) }, placeholder = { Text("sha-256=…") }, singleLine = true)
            }
        },
        confirmButton = {
            TextButton({
                val patch = mutableMapOf<String, Any?>(
                    "connections" to connections.toInt(),
                    "speedLimit" to ((limit.toLongOrNull() ?: 0) * 1024),
                    "checksum" to checksum,
                )
                if (!d.isRunning) {
                    if (url != d.url) patch["url"] = url.trim()
                    if (name != d.name && d.done == 0L) patch["name"] = name.trim()
                }
                onClose()
                scope.act { Ku.edit(d.id, patch) }
            }) { Text(t("Save")) }
        },
        dismissButton = { TextButton(onClose) { Text(t("Cancel")) } },
    )
}

@Composable
private fun VerifyDialog(d: Download, onClose: () -> Unit) {
    val ctx = LocalContext.current
    var algo by remember { mutableStateOf("sha-256") }
    var result by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(algo) {
        busy = true
        result = null
        error = null
        try {
            result = Ku.verify(d.id, algo)
        } catch (e: Exception) {
            error = e.message
        } finally {
            busy = false
        }
    }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(t("Checksum")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.horizontalScroll(rememberScrollState())) {
                    listOf("md5", "sha-1", "sha-256", "sha-512").forEach { a ->
                        AssistChip({ algo = a }, { Text(a.uppercase(), fontWeight = if (a == algo) FontWeight.Bold else FontWeight.Normal) })
                    }
                }
                when {
                    busy -> LinearProgressIndicator(Modifier.fillMaxWidth())
                    error != null -> Notice(error!!)
                    result != null -> SelectionContainer { Text(result!!, fontFamily = FontFamily.Monospace, fontSize = 12.sp) }
                }
            }
        },
        confirmButton = {
            TextButton({
                result?.let {
                    ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("checksum", it))
                    UiState.toast(t("Copied"))
                }
            }, enabled = result != null) { Text(t("Copy")) }
        },
        dismissButton = { TextButton(onClose) { Text(t("Close")) } },
    )
}

