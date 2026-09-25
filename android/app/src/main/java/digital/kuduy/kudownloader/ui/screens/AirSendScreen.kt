package digital.kuduy.kudownloader.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.TextFields
import androidx.compose.material.icons.filled.Verified
import androidx.compose.material.icons.filled.WifiTethering
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.AirPeer
import digital.kuduy.kudownloader.core.AirTransfer
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.AVATARS
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.Card
import digital.kuduy.kudownloader.ui.ClickRow
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.LocalKuColors
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.OsBadge
import digital.kuduy.kudownloader.ui.PixelAnimal
import digital.kuduy.kudownloader.ui.SectionTitle
import digital.kuduy.kudownloader.ui.SwitchRow
import digital.kuduy.kudownloader.ui.TextRow
import digital.kuduy.kudownloader.ui.UiState
import digital.kuduy.kudownloader.ui.animalName
import digital.kuduy.kudownloader.ui.avatarOf
import digital.kuduy.kudownloader.ui.looksLikeLink
import digital.kuduy.kudownloader.ui.osName
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.math.cos
import kotlin.math.sin

@Composable
fun AirSendScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val status by Ku.airStatus.collectAsStateWithLifecycle()
    val peers by Ku.airPeers.collectAsStateWithLifecycle()
    val transfers by Ku.airTransfers.collectAsStateWithLifecycle()
    var busy by remember { mutableStateOf(false) }
    var settingsOpen by remember { mutableStateOf(false) }
    var target by remember { mutableStateOf<AirPeer?>(null) }
    var textFor by remember { mutableStateOf<AirPeer?>(null) }
    var addOpen by remember { mutableStateOf(false) }
    var remoteFor by remember { mutableStateOf<AirPeer?>(null) }

    LaunchedEffect(Unit) { runCatching { Ku.airRefreshStatus() } }

    fun toggle(on: Boolean) {
        scope.launch {
            busy = true
            try {
                Ku.airSetEnabled(on)
                if (on) {
                    KuService.ensure(ctx)
                    Ku.airRefresh()
                }
            } catch (e: Exception) {
                UiState.toast(e.message ?: "")
            } finally {
                busy = false
            }
        }
    }

    fun send(peer: AirPeer, paths: List<String>, text: String?, pin: String? = null) {
        scope.launch {
            try {
                lastSend[peer.fingerprint] = paths to text
                Ku.airSend(peer.fingerprint, paths, text, pin)
                UiState.airShare = emptyList()
                UiState.airShareText = null
                UiState.toast(tf("Sending to {name}…", "name" to peer.alias))
            } catch (e: Exception) {
                UiState.toast(e.message ?: "")
            }
        }
    }

    val pickFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val peer = target ?: return@rememberLauncherForActivityResult
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            val paths = withContext(Dispatchers.IO) { uris.mapNotNull { Files.toLocalPath(ctx, it) } }
            if (paths.isEmpty()) UiState.toast(t("These files could not be read.")) else send(peer, paths, null)
        }
    }
    val pickFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        val peer = target ?: return@rememberLauncherForActivityResult
        val path = uri?.let { Files.treeToPath(it) }
        if (path != null && File(path).canRead()) send(peer, listOf(path), null)
        else if (uri != null) UiState.toast(t("Allow “Save to any folder” in Settings › Downloads to send whole folders."))
    }

    val pending = UiState.airShare
    val pendingText = UiState.airShareText

    KuScaffold(
        "KuAirSend",
        actions = {
            if (status.running) IconButton({ scope.act { Ku.airRefresh() } }) { Icon(Icons.Filled.Refresh, t("Look again")) }
            IconButton({ settingsOpen = true }) { Icon(Icons.Filled.Settings, t("Settings")) }
            Switch(status.running, { toggle(it) }, enabled = !busy, modifier = Modifier.padding(end = 12.dp))
        },
    ) { pad ->
        LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(bottom = 32.dp)) {
            if (pending.isNotEmpty() || pendingText != null) {
                item {
                    Card {
                        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(if (pendingText != null) Icons.Filled.TextFields else Icons.Filled.Description, null, tint = MaterialTheme.colorScheme.primary)
                            Spacer(Modifier.width(12.dp))
                            Text(
                                if (pendingText != null) t("Tap a device to send the links.") else tf("Tap a device to send {count} files.", "count" to pending.size),
                                Modifier.weight(1f),
                            )
                            IconButton({ UiState.airShare = emptyList(); UiState.airShareText = null }) { Icon(Icons.Filled.Close, t("Cancel")) }
                        }
                    }
                }
            }
            item {
                if (!status.running) {
                    Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        PixelAnimal(avatarOf(status.avatar, status.fingerprint), 96.dp, still = true)
                        Spacer(Modifier.height(16.dp))
                        Text(t("KuAirSend is off"), style = MaterialTheme.typography.titleMedium)
                        Spacer(Modifier.height(6.dp))
                        Text(
                            t("Send files, folders, text and links to nearby KuDownloader devices on the same Wi-Fi, at full local speed. Nothing goes through the internet."),
                            textAlign = TextAlign.Center,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(16.dp))
                        androidx.compose.material3.Button({ toggle(true) }, enabled = !busy) { Text(t("Turn on")) }
                    }
                } else {
                    Radar(status.alias, avatarOf(status.avatar, status.fingerprint), peers) { p ->
                        when {
                            pending.isNotEmpty() -> send(p, pending, null)
                            pendingText != null -> send(p, emptyList(), pendingText)
                            else -> target = p
                        }
                    }
                }
            }
            if (status.running) {
                item {
                    Text(
                        if (peers.isEmpty()) t("Looking for nearby devices… Open KuAirSend on the other device.") else t("Tap a device to send to it."),
                        Modifier.fillMaxWidth().padding(horizontal = 24.dp),
                        textAlign = TextAlign.Center,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                status.discoveryError?.let { e -> item { Notice(te(e), MaterialTheme.colorScheme.tertiary, Modifier.padding(12.dp)) } }
                item {
                    TextButton({ addOpen = true }, Modifier.padding(horizontal = 8.dp)) {
                        Icon(Icons.Filled.Add, null)
                        Spacer(Modifier.width(6.dp))
                        Text(t("Add a device by address"))
                    }
                }
            }
            if (transfers.isNotEmpty()) {
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        SectionTitle(t("Transfers"), Modifier.weight(1f))
                        TextButton({ scope.act { Ku.airClearHistory() } }, Modifier.padding(end = 8.dp)) { Text(t("Clear history")) }
                    }
                }
                items(transfers, key = { it.id }) { tr -> TransferRow(tr) }
            }
        }
    }

    target?.let { p ->
        PeerSheet(
            p,
            onDismiss = { target = null },
            onFiles = { pickFiles.launch(arrayOf("*/*")) },
            onFolder = { pickFolder.launch(null) },
            onText = { textFor = p; target = null },
            onRemote = { remoteFor = p; target = null },
        )
    }
    textFor?.let { p ->
        var text by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { textFor = null },
            title = { Text(tf("Send to {name}", "name" to p.alias)) },
            text = { OutlinedTextField(text, { text = it }, placeholder = { Text(t("Text or link")) }, minLines = 3, modifier = Modifier.fillMaxWidth()) },
            confirmButton = { TextButton({ textFor = null; send(p, emptyList(), text) }, enabled = text.isNotBlank()) { Text(t("Send")) } },
            dismissButton = { TextButton({ textFor = null }) { Text(t("Cancel")) } },
        )
    }
    if (addOpen) {
        var address by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { addOpen = false },
            title = { Text(t("Add a device by address")) },
            text = {
                Column {
                    Text(t("When a device doesn't appear (for example on a guest network), enter the address it shows in KuAirSend."), style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.height(8.dp))
                    OutlinedTextField(address, { address = it }, placeholder = { Text("192.168.1.20:53317") }, singleLine = true)
                }
            },
            confirmButton = {
                TextButton({
                    addOpen = false
                    scope.act {
                        val peer = Ku.airAdd(address.trim())
                        UiState.toast(tf("{name} added", "name" to peer.alias))
                    }
                }, enabled = address.isNotBlank()) { Text(t("Add")) }
            },
            dismissButton = { TextButton({ addOpen = false }) { Text(t("Cancel")) } },
        )
    }
    if (settingsOpen) AirSettingsSheet { settingsOpen = false }
    remoteFor?.let { p ->
        var url by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { remoteFor = null },
            title = { Text(tf("Download on {name}", "name" to p.alias)) },
            text = { OutlinedTextField(url, { url = it }, placeholder = { Text("https://…") }, singleLine = true, modifier = Modifier.fillMaxWidth()) },
            confirmButton = {
                TextButton({
                    remoteFor = null
                    UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(listOf(url.trim()))
                }, enabled = looksLikeLink(url.trim())) { Text(t("Next")) }
            },
            dismissButton = { TextButton({ remoteFor = null }) { Text(t("Cancel")) } },
        )
    }
}

/** The last thing sent to each device: sent again when it asks for a PIN. */
private val lastSend = mutableMapOf<String, Pair<List<String>, String?>>()

/** Nearby devices around this one, on two rings. */
@Composable
private fun Radar(alias: String, avatar: String, peers: List<AirPeer>, onPick: (AirPeer) -> Unit) {
    val ring = MaterialTheme.colorScheme.primary
    val sweep = rememberInfiniteTransition(label = "radar")
    val pulse by sweep.animateFloat(0f, 1f, infiniteRepeatable(tween(2600, easing = LinearEasing)), label = "pulse")
    BoxWithConstraints(Modifier.fillMaxWidth().padding(horizontal = 12.dp).aspectRatio(1f).widthIn(max = 480.dp), contentAlignment = Alignment.Center) {
        val side = minOf(maxWidth, maxHeight)
        Canvas(Modifier.size(side)) {
            val c = Offset(size.width / 2, size.height / 2)
            val r = size.minDimension / 2
            for (f in listOf(0.3f, 0.62f, 0.94f)) drawCircle(ring.copy(alpha = 0.10f), r * f, c, style = Stroke(1.5f))
            drawCircle(ring.copy(alpha = (1f - pulse) * 0.35f), r * (0.18f + pulse * 0.8f), c, style = Stroke(3f))
        }
        // This device, in the exact centre.
        Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.align(Alignment.Center)) {
            PixelAnimal(avatar, 72.dp)
            Text(alias, style = MaterialTheme.typography.labelMedium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 110.dp))
        }
        val inner = peers.take(6)
        val outer = peers.drop(6).take(12)
        listOf(inner to 0.62f, outer to 0.9f).forEach { (list, radius) ->
            list.forEachIndexed { i, p ->
                val angle = (2 * Math.PI * i / list.size.coerceAtLeast(1)) - Math.PI / 2 + if (radius > 0.7f) Math.PI / list.size.coerceAtLeast(1) else 0.0
                val dx = side / 2 * radius * cos(angle).toFloat()
                val dy = side / 2 * radius * sin(angle).toFloat()
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    modifier = Modifier.align(Alignment.Center).offset(dx, dy).clip(RoundedCornerShape(12.dp)).clickable { onPick(p) }.padding(4.dp),
                ) {
                    Box {
                        PixelAnimal(avatarOf(p.avatar, p.fingerprint), 56.dp, seed = p.fingerprint.hashCode())
                        Box(Modifier.align(Alignment.BottomEnd)) { OsBadge(p.os, 20.dp) }
                    }
                    Text(p.alias, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 88.dp))
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PeerSheet(p: AirPeer, onDismiss: () -> Unit, onFiles: () -> Unit, onFolder: () -> Unit, onText: () -> Unit, onRemote: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(bottom = 24.dp)) {
            Row(Modifier.padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                PixelAnimal(avatarOf(p.avatar, p.fingerprint), 56.dp)
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(p.alias, style = MaterialTheme.typography.titleMedium)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        OsBadge(p.os, 16.dp)
                        Spacer(Modifier.width(6.dp))
                        Text("${osName(p.os)} · ${p.ip} · KuDownloader ${p.version}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                if (p.trusted) Icon(Icons.Filled.Verified, t("Trusted"), tint = LocalKuColors.current.success)
            }
            Spacer(Modifier.height(12.dp))
            ClickRow(t("Send files"), icon = Icons.Filled.Description) { onFiles() }
            ClickRow(t("Send a folder"), icon = Icons.Filled.FolderOpen) { onFolder() }
            ClickRow(t("Send text or a link"), icon = Icons.Filled.TextFields) { onText() }
            ClickRow(tf("Download on {name}", "name" to p.alias), t("It downloads the link itself, now or later"), Icons.Filled.CloudDownload) { onRemote() }
            HorizontalDivider()
            val scope = rememberCoroutineScope()
            // The live device (the sheet was opened with a snapshot), switched at once.
            val peers by Ku.airPeers.collectAsStateWithLifecycle()
            val live = peers.firstOrNull { it.fingerprint == p.fingerprint } ?: p
            var trusted by remember(live.trusted) { mutableStateOf(live.trusted) }
            SwitchRow(t("Trust this device"), trusted, t("Files, links and scheduled downloads go through without asking. It will be asked to trust this phone too.")) { on ->
                trusted = on
                scope.launch {
                    try {
                        Ku.airTrust(p.fingerprint, on)
                        Ku.airPeers.value = Ku.airPeers.value.map { if (it.fingerprint == p.fingerprint) it.copy(trusted = on) else it }
                    } catch (e: Exception) {
                        trusted = !on
                        UiState.toast(e.message ?: "")
                    }
                }
            }
        }
    }
}

@Composable
private fun TransferRow(tr: AirTransfer) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val ku = LocalKuColors.current
    Card {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                PixelAnimal(avatarOf(tr.peerAvatar, tr.peerFingerprint), 36.dp, still = true)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    val what = when {
                        tr.download != null -> tr.download.filename ?: tr.download.url
                        tr.text != null -> t("Text")
                        tr.fileCount == 0 -> t("Preparing…")
                        tr.fileCount == 1 -> tr.files.firstOrNull()?.name ?: ""
                        else -> tf("{count} files", "count" to tr.fileCount)
                    }
                    Text(
                        if (tr.direction == "send") tf("To {name}", "name" to tr.peer) else tf("From {name}", "name" to tr.peer),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(what, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyLarge)
                }
                if (tr.isOpen) IconButton({ scope.act { Ku.airCancel(tr.id) } }) { Icon(Icons.Filled.Close, t("Cancel")) }
                if (tr.state == "pin" && tr.direction == "send" && lastSend.containsKey(tr.peerFingerprint)) {
                    var ask by remember { mutableStateOf(false) }
                    TextButton({ ask = true }) { Text(t("Enter PIN")) }
                    if (ask) {
                        var pin by remember { mutableStateOf("") }
                        AlertDialog(
                            onDismissRequest = { ask = false },
                            title = { Text(tf("PIN for {name}", "name" to tr.peer)) },
                            text = { OutlinedTextField(pin, { pin = it.filter { c -> c.isDigit() } }, singleLine = true, placeholder = { Text("1234") }) },
                            confirmButton = {
                                TextButton({
                                    ask = false
                                    val (paths, text) = lastSend.getValue(tr.peerFingerprint)
                                    scope.act { Ku.airSend(tr.peerFingerprint, paths, text, pin) }
                                }, enabled = pin.isNotBlank()) { Text(t("Send")) }
                            },
                            dismissButton = { TextButton({ ask = false }) { Text(t("Cancel")) } },
                        )
                    }
                }
                if (tr.state == "done" && tr.direction == "receive" && tr.folder != null) {
                    IconButton({ Files.openFolder(ctx, File(tr.folder)) }) { Icon(Icons.Filled.FolderOpen, t("Open folder"), tint = MaterialTheme.colorScheme.primary) }
                }
            }
            if (tr.state == "transferring" && tr.total > 0) {
                Spacer(Modifier.height(8.dp))
                LinearProgressIndicator(progress = { (tr.done.toFloat() / tr.total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)))
            }
            Spacer(Modifier.height(6.dp))
            val line = when (tr.state) {
                "waiting" -> t("Waiting for the other device to accept…")
                "transferring" -> listOf("${Fmt.size(tr.done)} / ${Fmt.size(tr.total)}", Fmt.speed(tr.speed)).joinToString(" · ")
                "done" -> tr.download?.let { d ->
                    val at = d.at?.takeIf { it > tr.started + 30_000 }
                    when {
                        at != null -> tf("Download scheduled for {time}", "time" to Fmt.date(at))
                        tr.direction == "send" -> tf("{peer} is downloading it", "peer" to tr.peer)
                        else -> t("Download added")
                    }
                } ?: listOf(t("Done"), Fmt.size(tr.total), Fmt.relative(tr.finished)).filter { it.isNotBlank() }.joinToString(" · ")
                "declined" -> t("Declined")
                "pin" -> t("Wrong PIN")
                "cancelled" -> t("Cancelled")
                else -> te(tr.error ?: t("Failed"))
            }
            Text(line, style = MaterialTheme.typography.bodySmall, color = when (tr.state) {
                "done" -> ku.success
                "failed", "declined", "pin" -> ku.danger
                else -> MaterialTheme.colorScheme.onSurfaceVariant
            })
            tr.text?.let { txt ->
                Spacer(Modifier.height(6.dp))
                Surface(color = MaterialTheme.colorScheme.surfaceContainerHighest, shape = RoundedCornerShape(8.dp)) {
                    Text(txt, Modifier.padding(10.dp), style = MaterialTheme.typography.bodyMedium, maxLines = 6, overflow = TextOverflow.Ellipsis)
                }
                Row {
                    TextButton({
                        ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("KuAirSend", txt))
                        UiState.toast(t("Copied"))
                    }) { Text(t("Copy")) }
                    if (looksLikeLink(txt.trim())) TextButton({ UiState.add = AddPrefill(url = txt.trim(), source = "airsend") }) { Text(t("Download")) }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun AirSettingsSheet(onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val status by Ku.airStatus.collectAsStateWithLifecycle()
    val settings by Ku.settings.collectAsStateWithLifecycle()
    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let { Files.treeToPath(it) }?.let { p -> scope.act { Ku.saveSettings(mapOf("airsendFolder" to p)); Ku.airRefreshStatus() } }
    }
    val current = avatarOf(status.avatar, status.fingerprint)
    ModalBottomSheet(onDismissRequest = onClose) {
        LazyColumn(contentPadding = PaddingValues(bottom = 32.dp)) {
            item {
                TextRow(t("Device name"), Ku.settingString("airsendName"), placeholder = status.alias) { v -> scope.act { Ku.saveSettings(mapOf("airsendName" to v)); Ku.airRefreshStatus() } }
                SectionTitle(t("Your animal"))
                // Four even columns, same spacing across and down.
                FlowRow(
                    Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    horizontalArrangement = Arrangement.SpaceEvenly,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    maxItemsInEachRow = 4,
                ) {
                    AVATARS.forEach { a ->
                        val chosen = a == current
                        Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                            Box(
                                Modifier.size(64.dp).clip(CircleShape)
                                    .border(3.dp, if (chosen) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                                    .clickable { scope.act { Ku.saveSettings(mapOf("airsendAvatar" to a)); Ku.airRefreshStatus() } },
                                contentAlignment = Alignment.Center,
                            ) { PixelAnimal(a, 54.dp, still = !chosen) }
                        }
                    }
                }
                Text(animalName(current), Modifier.padding(16.dp), style = MaterialTheme.typography.bodySmall)
                ClickRow(t("Save received files to"), status.folder.ifBlank { t("Download/KuDownloader/KuAirSend") }) { folderPicker.launch(null) }
                val auto = settings["airsendAutoAccept"]?.toString() == "true"
                SwitchRow(t("Accept files without asking"), auto, t("From any nearby KuDownloader. Trusted devices never ask.")) { on -> scope.act { Ku.saveSettings(mapOf("airsendAutoAccept" to on)) } }
                TextRow(t("PIN"), Ku.settingString("airsendPin"), subtitle = if (Ku.settingString("airsendPin").isBlank()) t("Off: anyone nearby can send") else t("Senders must enter it"), number = true) { v ->
                    scope.act { Ku.saveSettings(mapOf("airsendPin" to v)) }
                }
                if (status.running) {
                    SectionTitle(t("This device"))
                    Text(
                        (status.addresses.map { "$it:${status.port}" } + listOf(t("Fingerprint") + " " + status.fingerprint.take(16).chunked(4).joinToString(" "))).joinToString("\n"),
                        Modifier.padding(horizontal = 16.dp),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

/** Accept/decline dialogs for incoming files and messages. */
@Composable
fun AirSendPrompts() {
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    UiState.airRequests.firstOrNull()?.let { r ->
        var trust by remember(r.id) { mutableStateOf(false) }
        AlertDialog(
            onDismissRequest = {},
            icon = { PixelAnimal(avatarOf(r.peerAvatar, r.peerFingerprint), 56.dp) },
            title = { Text(tf("{name} wants to send you files", "name" to r.peer), textAlign = TextAlign.Center) },
            text = {
                Column {
                    r.files.take(6).forEach { f ->
                        Row {
                            Text(f.name, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(Fmt.size(f.size), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    if (r.fileCount > 6) Text(tf("and {count} more", "count" to (r.fileCount - 6)), style = MaterialTheme.typography.bodySmall)
                    Text(tf("{count} files · {size}", "count" to r.fileCount, "size" to Fmt.size(r.total)), style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(top = 8.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(trust, { trust = it })
                        Text(t("Always accept from this device"))
                    }
                }
            },
            confirmButton = {
                TextButton({
                    UiState.airRequests.remove(r)
                    digital.kuduy.kudownloader.service.Notifier.cancel(ctx, r.id)
                    scope.act { Ku.airDecide(r.id, accept = true, trust = trust); KuService.ensure(ctx) }
                }) { Text(t("Accept")) }
            },
            dismissButton = {
                TextButton({
                    UiState.airRequests.remove(r)
                    digital.kuduy.kudownloader.service.Notifier.cancel(ctx, r.id)
                    scope.act { Ku.airDecide(r.id, accept = false, trust = false) }
                }) { Text(t("Decline")) }
            },
        )
        return
    }
    UiState.airDownloads.firstOrNull()?.let { r ->
        DownloadRequestDialog(r)
        return
    }
    UiState.airTrusts.firstOrNull()?.let { r ->
        AlertDialog(
            onDismissRequest = { UiState.airTrusts.remove(r) },
            icon = { PixelAnimal(avatarOf(r.peerAvatar, r.peerFingerprint), 56.dp) },
            title = { Text(tf("{name} trusts this phone", "name" to r.peer), textAlign = TextAlign.Center) },
            text = { Text(tf("Trust {name} too? Files, links and scheduled downloads between you will then go through without asking.", "name" to r.peer)) },
            confirmButton = {
                TextButton({
                    UiState.airTrusts.remove(r)
                    digital.kuduy.kudownloader.service.Notifier.cancel(ctx, r.peerFingerprint)
                    scope.act { Ku.airTrust(r.peerFingerprint, true) }
                }) { Text(t("Trust")) }
            },
            dismissButton = { TextButton({ UiState.airTrusts.remove(r) }) { Text(t("Not now")) } },
        )
        return
    }
    UiState.airMessages.firstOrNull()?.let { m ->
        AlertDialog(
            onDismissRequest = { UiState.airMessages.remove(m) },
            icon = { PixelAnimal(avatarOf(m.peerAvatar, m.peerFingerprint), 56.dp) },
            title = { Text(tf("Message from {name}", "name" to m.peer)) },
            text = { Text(m.text) },
            confirmButton = {
                Row {
                    if (looksLikeLink(m.text.trim())) {
                        TextButton({
                            UiState.airMessages.remove(m)
                            UiState.add = AddPrefill(url = m.text.trim(), source = "airsend")
                        }) { Text(t("Download")) }
                    }
                    TextButton({
                        ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("KuAirSend", m.text))
                        UiState.airMessages.remove(m)
                        UiState.toast(t("Copied"))
                    }) { Text(t("Copy")) }
                }
            },
            dismissButton = { TextButton({ UiState.airMessages.remove(m) }) { Text(t("Close")) } },
        )
    }
}


/**
 * Hand links to a computer (or another phone) running KuDownloader: it
 * downloads them with its own connection, now or at a chosen time.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun RemoteSendSheet(send: digital.kuduy.kudownloader.ui.RemoteSend, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val status by Ku.airStatus.collectAsStateWithLifecycle()
    val peers by Ku.airPeers.collectAsStateWithLifecycle()
    var whenChoice by remember { mutableStateOf("now") }
    var custom by remember { mutableStateOf<java.time.LocalDateTime?>(null) }
    var picking by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(status.running) { if (status.running) runCatching { Ku.airRefresh() } }

    fun startAt(): Long? {
        val now = java.time.LocalDateTime.now()
        val at = when (whenChoice) {
            "hour" -> now.plusHours(1)
            "night" -> now.toLocalDate().atTime(2, 0).let { if (it.isAfter(now)) it else it.plusDays(1) }
            "custom" -> custom
            else -> null
        } ?: return null
        return at.atZone(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()
    }

    fun go(p: AirPeer) {
        scope.launch {
            busy = true
            try {
                val at = startAt()
                send.urls.forEachIndexed { i, u ->
                    // Untrusted devices are asked once per link, at most every 2 s.
                    if (i > 0 && !p.trusted) kotlinx.coroutines.delay(2100)
                    Ku.airSendDownload(
                        p.fingerprint,
                        digital.kuduy.kudownloader.core.RemoteDownload(url = u, filename = send.filename.takeIf { send.urls.size == 1 }, at = at, referer = send.referer, cookies = send.cookies),
                    )
                }
                UiState.toast(if (at != null) tf("{name} downloads it at {time}", "name" to p.alias, "time" to Fmt.date(at)) else tf("Sent to {name}", "name" to p.alias))
                onClose()
            } catch (e: Exception) {
                UiState.toast(e.message ?: "")
            } finally {
                busy = false
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onClose) {
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 28.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(t("Download on another device"), style = MaterialTheme.typography.titleLarge)
            Text(
                if (send.urls.size == 1) send.filename ?: send.urls[0] else tf("{count} links", "count" to send.urls.size),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(t("When"), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("now" to t("Now"), "hour" to t("In 1 hour"), "night" to t("Tonight at 02:00")).forEach { (k, l) ->
                    androidx.compose.material3.FilterChip(whenChoice == k, { whenChoice = k }, { Text(l) })
                }
                androidx.compose.material3.FilterChip(
                    whenChoice == "custom",
                    { picking = true },
                    { Text(custom?.let { Fmt.date(it.atZone(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()) } ?: t("Choose a time…")) },
                )
            }
            Text(t("The other device downloads the file itself, with its own connection."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            HorizontalDivider()
            when {
                !status.running -> {
                    Text(t("Turn on KuAirSend to see your computers."))
                    androidx.compose.material3.Button({ scope.act { Ku.airSetEnabled(true); KuService.ensure(ctx) } }) { Text(t("Turn on")) }
                }
                peers.isEmpty() -> Text(t("Looking for nearby devices… Open KuAirSend on the other device."), color = MaterialTheme.colorScheme.onSurfaceVariant)
                else -> peers.forEach { p ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(enabled = !busy) { go(p) }.padding(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box {
                            PixelAnimal(avatarOf(p.avatar, p.fingerprint), 44.dp, seed = p.fingerprint.hashCode())
                            Box(Modifier.align(Alignment.BottomEnd)) { OsBadge(p.os, 16.dp) }
                        }
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(p.alias, style = MaterialTheme.typography.bodyLarge)
                            Text(osName(p.os) + if (p.trusted) " · " + t("Trusted") else "", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        Icon(Icons.Filled.CloudDownload, null, tint = MaterialTheme.colorScheme.primary)
                    }
                }
            }
        }
    }
    if (picking) {
        val next = java.time.LocalDateTime.now().plusHours(1)
        val state = androidx.compose.material3.rememberTimePickerState(next.hour, 0, is24Hour = true)
        AlertDialog(
            onDismissRequest = { picking = false },
            title = { Text(t("Start at")) },
            text = { androidx.compose.material3.TimePicker(state) },
            confirmButton = {
                TextButton({
                    val today = java.time.LocalDate.now().atTime(state.hour, state.minute)
                    custom = if (today.isAfter(java.time.LocalDateTime.now())) today else today.plusDays(1)
                    whenChoice = "custom"
                    picking = false
                }) { Text(t("OK")) }
            },
            dismissButton = { TextButton({ picking = false }) { Text(t("Cancel")) } },
        )
    }
}

/** Another device asks this phone to download a link. */
@Composable
private fun DownloadRequestDialog(r: digital.kuduy.kudownloader.core.AirDownloadRequest) {
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var trust by remember(r.id) { mutableStateOf(false) }
    val at = r.download.at?.takeIf { it > System.currentTimeMillis() + 30_000 }
    fun answer(accept: Boolean) {
        UiState.airDownloads.remove(r)
        digital.kuduy.kudownloader.service.Notifier.cancel(ctx, r.id)
        scope.act {
            Ku.airDecide(r.id, accept, accept && trust)
            if (accept) KuService.ensure(ctx)
        }
    }
    AlertDialog(
        onDismissRequest = {},
        icon = { PixelAnimal(avatarOf(r.peerAvatar, r.peerFingerprint), 56.dp) },
        title = {
            Text(
                if (at != null) tf("{name} asks this phone to download at {time}", "name" to r.peer, "time" to Fmt.date(at)) else tf("{name} asks this phone to download", "name" to r.peer),
                textAlign = TextAlign.Center,
            )
        },
        text = {
            Column {
                r.download.filename?.let { Text(it, style = MaterialTheme.typography.titleSmall) }
                Text(r.download.url, style = MaterialTheme.typography.bodySmall, maxLines = 4, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(trust, { trust = it })
                    Text(t("Always accept from this device"))
                }
            }
        },
        confirmButton = { TextButton({ answer(true) }) { Text(if (at != null) t("Schedule") else t("Download")) } },
        dismissButton = { TextButton({ answer(false) }) { Text(t("Decline")) } },
    )
}
