package digital.kuduy.kudownloader.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.SmartDisplay
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.ProbeInfo
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.AddPrefill
import digital.kuduy.kudownloader.ui.LocalKuColors
import digital.kuduy.kudownloader.ui.MediaPrefill
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.UiState
import digital.kuduy.kudownloader.ui.looksLikeLink
import digital.kuduy.kudownloader.ui.normalizeUrl
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddSheet(prefill: AddPrefill, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val queues by Ku.queues.collectAsStateWithLifecycle()
    val torrent = prefill.torrent

    var url by remember { mutableStateOf(prefill.url) }
    var probe by remember { mutableStateOf<ProbeInfo?>(null) }
    var probing by remember { mutableStateOf(false) }
    var filename by remember { mutableStateOf(prefill.filename ?: "") }
    var nameEdited by remember { mutableStateOf(prefill.filename != null) }
    var dir by remember { mutableStateOf<String?>(null) }
    var connections by remember { androidx.compose.runtime.mutableFloatStateOf(Ku.settingLong("defaultConnections", 0).toFloat()) }
    var queueId by remember { mutableStateOf("main") }
    var startNow by remember { mutableStateOf(true) }
    var advanced by remember { mutableStateOf(false) }
    var referer by remember { mutableStateOf(prefill.referer ?: "") }
    var userAgent by remember { mutableStateOf(prefill.userAgent ?: "") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var checksum by remember { mutableStateOf("") }
    var limitKb by remember { mutableStateOf("") }
    var headers by remember { mutableStateOf("") }
    var mirrors by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var duplicate by remember { mutableStateOf<JsonObject?>(null) }
    val selectedFiles = remember { mutableStateListOf<Int>().apply { torrent?.files?.forEach { add(it.index) } } }

    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let { Files.treeToPath(it) }?.let { dir = it }
    }

    // Look the address up as the user types (debounced).
    LaunchedEffect(url) {
        probe = null
        error = null
        val u = normalizeUrl(url)
        if (torrent != null || !looksLikeLink(u)) return@LaunchedEffect
        delay(500)
        probing = true
        try {
            val p = Ku.probe(u, options(prefill, referer, userAgent, username, password))
            probe = p
            if (!nameEdited) filename = p.filename ?: ""
            p.error?.let { error = te(it) }
        } catch (e: Exception) {
            error = e.message
        } finally {
            probing = false
        }
    }

    fun request(start: Boolean): JsonObject = buildJsonObject {
        put("url", normalizeUrl(url))
        put("mirrors", Ku.toJson(mirrors.lines().map { it.trim() }.filter { looksLikeLink(it) }))
        dir?.let { put("dir", it) }
        filename.trim().takeIf { it.isNotEmpty() }?.let { put("filename", it) }
        put("connections", connections.toInt())
        probe?.category?.let { put("category", it) }
        if (!start) put("queueId", queueId)
        put("start", start)
        put("source", prefill.source)
        (probe?.size ?: prefill.size)?.let { put("sizeHint", it) }
        val o = options(prefill, referer, userAgent, username, password).toMutableMap()
        if (checksum.isNotBlank()) o["checksum"] = Ku.toJson(checksum.trim())
        limitKb.toLongOrNull()?.takeIf { it > 0 }?.let { o["speedLimit"] = Ku.toJson(it * 1024) }
        val h = headers.lines().map { it.trim() }.filter { it.contains(':') }
        if (h.isNotEmpty()) o["headers"] = Ku.toJson(h)
        prefill.torrentData?.let { o["torrentData"] = Ku.toJson(it) }
        if (torrent != null && selectedFiles.size < torrent.files.size) o["selectFiles"] = Ku.toJson(selectedFiles.sorted().joinToString(","))
        put("options", JsonObject(o))
    }

    fun submit(start: Boolean, force: Boolean = false) {
        scope.launch {
            busy = true
            error = null
            try {
                if (!force && torrent == null) {
                    val dup = Ku.checkDuplicate(normalizeUrl(url), dir, filename.takeIf { it.isNotBlank() })
                    if (dup["existing"] != null && dup["existing"] !is kotlinx.serialization.json.JsonNull) {
                        duplicate = dup
                        return@launch
                    }
                }
                Ku.add(request(start))
                if (start) KuService.ensure(ctx)
                UiState.toast(if (start) t("Download started") else t("Added to the queue"))
                onClose()
            } catch (e: Exception) {
                error = e.message
            } finally {
                busy = false
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onClose, sheetState = sheet) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).imePadding().navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(if (torrent != null) t("Add torrent") else t("Add URL"), style = MaterialTheme.typography.titleLarge)
            if (torrent == null) {
                OutlinedTextField(
                    url,
                    { url = it },
                    label = { Text(t("Address")) },
                    placeholder = { Text("https://…") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    trailingIcon = {
                        IconButton({
                            val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                            cm?.primaryClip?.getItemAt(0)?.coerceToText(ctx)?.toString()?.trim()?.let { url = it }
                        }) { Icon(Icons.Filled.ContentPaste, t("Paste")) }
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            if (probing) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(t("Checking the address…"), style = MaterialTheme.typography.bodyMedium)
                }
            }

            val p = probe
            if (p?.kind == "media" || p?.engine == "ytdlp") {
                Notice(t("This is a video or music page. Choose the quality to download."), MaterialTheme.colorScheme.primary)
                Button(
                    {
                        UiState.media = MediaPrefill(normalizeUrl(url), prefill.cookies, prefill.referer)
                        UiState.go(Screen.Video)
                        onClose()
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(Icons.Filled.SmartDisplay, null)
                    Spacer(Modifier.width(8.dp))
                    Text(t("Choose quality"))
                }
            }

            if (torrent != null) {
                Text(torrent.name, style = MaterialTheme.typography.titleMedium)
                Text(tf("{count} files · {size}", "count" to torrent.files.size, "size" to Fmt.size(torrent.total)), color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (torrent.files.size > 1) {
                    Column(Modifier.heightIn(max = 260.dp).verticalScroll(rememberScrollState())) {
                        torrent.files.forEach { f ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Checkbox(f.index in selectedFiles, { on -> if (on) selectedFiles.add(f.index) else selectedFiles.remove(f.index) })
                                Text(f.path, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
                                Text(Fmt.size(f.length), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                }
            } else {
                OutlinedTextField(filename, { filename = it; nameEdited = true }, label = { Text(t("File name")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                p?.let { info ->
                    val facts = listOfNotNull(
                        info.size?.let { Fmt.size(it) } ?: t("Unknown size"),
                        info.resumable?.let { if (it) t("Resumable") else t("Can't resume") },
                        info.mime,
                    )
                    Text(facts.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = if (info.resumable == false) LocalKuColors.current.warning else MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Folder, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(t("Save to"), style = MaterialTheme.typography.labelMedium)
                    Text(dir ?: t("Automatic (by file type)"), style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
                TextButton({ folderPicker.launch(null) }) { Text(t("Change")) }
                if (dir != null) TextButton({ dir = null }) { Text(t("Reset")) }
            }

            TextButton({ advanced = !advanced }) {
                Text(t("Advanced options"))
                Icon(if (advanced) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, null)
            }
            AnimatedVisibility(advanced) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(if (connections < 1) t("Connections: Smart") else tf("Connections: {count}", "count" to connections.toInt()), style = MaterialTheme.typography.labelLarge)
                    Slider(connections, { connections = it }, valueRange = 0f..16f, steps = 15)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(t("Queue for “Add to queue”"), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
                        Box {
                            var open by remember { mutableStateOf(false) }
                            TextButton({ open = true }) { Text(queues.firstOrNull { it.id == queueId }?.let { queueName(it.id, it.name) } ?: t("Main queue")) }
                            DropdownMenu(open, { open = false }) {
                                queues.forEach { q -> DropdownMenuItem({ Text(queueName(q.id, q.name)) }, { queueId = q.id; open = false }) }
                            }
                        }
                    }
                    OutlinedTextField(referer, { referer = it }, label = { Text(t("Referer")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(userAgent, { userAgent = it }, label = { Text(t("User agent")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedTextField(username, { username = it }, label = { Text(t("User name")) }, singleLine = true, modifier = Modifier.weight(1f))
                        OutlinedTextField(password, { password = it }, label = { Text(t("Password")) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.weight(1f))
                    }
                    OutlinedTextField(checksum, { checksum = it }, label = { Text(t("Checksum")) }, placeholder = { Text("sha-256=…") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(limitKb, { limitKb = it.filter { c -> c.isDigit() } }, label = { Text(t("Speed limit (KB/s)")) }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(headers, { headers = it }, label = { Text(t("Extra headers (one per line)")) }, placeholder = { Text("Authorization: Bearer …") }, minLines = 2, modifier = Modifier.fillMaxWidth())
                    if (torrent == null) OutlinedTextField(mirrors, { mirrors = it }, label = { Text(t("Mirrors (one per line)")) }, minLines = 2, modifier = Modifier.fillMaxWidth())
                }
            }

            error?.let { Notice(it) }

            if (torrent == null) {
                TextButton(
                    {
                        UiState.remoteSend = digital.kuduy.kudownloader.ui.RemoteSend(listOf(normalizeUrl(url)), filename.takeIf { it.isNotBlank() }, referer.takeIf { it.isNotBlank() }, prefill.cookies)
                        onClose()
                    },
                    enabled = looksLikeLink(normalizeUrl(url)),
                ) {
                    Icon(Icons.Filled.Computer, null)
                    Spacer(Modifier.width(8.dp))
                    Text(t("Download on a computer instead…"))
                }
            }

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                val ok = torrent != null || looksLikeLink(normalizeUrl(url))
                OutlinedButton({ startNow = false; submit(false) }, Modifier.weight(1f), enabled = ok && !busy) { Text(t("Add to queue")) }
                Button({ startNow = true; submit(true) }, Modifier.weight(1.3f), enabled = ok && !busy) {
                    Icon(Icons.Filled.Download, null)
                    Spacer(Modifier.width(6.dp))
                    Text(t("Download"), fontWeight = FontWeight.SemiBold)
                }
            }
            Spacer(Modifier.height(12.dp))
        }
    }

    duplicate?.let { dup ->
        val existing = dup["existing"]?.jsonObject
        val fileThere = dup["existingFileExists"]?.jsonPrimitive?.booleanOrNull == true
        AlertDialog(
            onDismissRequest = { duplicate = null },
            title = { Text(t("Already downloaded")) },
            text = {
                Text(
                    tf("{name} was downloaded before.", "name" to (existing?.get("name")?.jsonPrimitive?.contentOrNull ?: "")) + " " +
                        if (fileThere) t("The file is still on this phone.") else t("The file is no longer on this phone."),
                )
            },
            confirmButton = { TextButton({ duplicate = null; submit(startNow, force = true) }) { Text(t("Download again")) } },
            dismissButton = {
                Row {
                    existing?.get("id")?.jsonPrimitive?.contentOrNull?.let { id ->
                        TextButton({ duplicate = null; onClose(); UiState.details = id }) { Text(t("Show")) }
                    }
                    TextButton({ duplicate = null }) { Text(t("Cancel")) }
                }
            },
        )
    }
}

private fun options(prefill: AddPrefill, referer: String, userAgent: String, username: String, password: String): JsonObject = buildJsonObject {
    referer.trim().takeIf { it.isNotEmpty() }?.let { put("referer", it) }
    userAgent.trim().takeIf { it.isNotEmpty() }?.let { put("userAgent", it) }
    username.takeIf { it.isNotEmpty() }?.let { put("username", it) }
    password.takeIf { it.isNotEmpty() }?.let { put("password", it) }
    if (prefill.cookies.isNotEmpty()) put("cookies", Ku.json.encodeToJsonElement(prefill.cookies))
}
