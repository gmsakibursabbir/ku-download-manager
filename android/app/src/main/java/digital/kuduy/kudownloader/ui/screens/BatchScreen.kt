package digital.kuduy.kudownloader.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AutoFixHigh
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Files
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.Card
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

private val BATCH_LINK = Regex("""^(?:(?:https?|ftp|sftp)://\S+|magnet:\?\S+)$""", RegexOption.IGNORE_CASE)
private val RANGE = Regex("""\[(\d+)-(\d+)(?::(\d+))?]|\[([a-z])-([a-z])]""", RegexOption.IGNORE_CASE)

/** Expand `[001-120]`, `[a-z]` and `[1-99:2]` ranges (same rules as the desktop). */
fun expandPattern(p: String, limit: Int = 5000): List<String> {
    val m = RANGE.find(p) ?: return listOf(p)
    val before = p.substring(0, m.range.first)
    val after = p.substring(m.range.last + 1)
    val out = mutableListOf<String>()
    val rest = expandPattern(after, limit)
    if (m.groups[1] != null) {
        val width = m.groupValues[1].length
        val a = m.groupValues[1].toLong()
        val b = m.groupValues[2].toLong()
        val step = (m.groups[3]?.value?.toLongOrNull() ?: 1).coerceAtLeast(1)
        var i = a
        while (if (a <= b) i <= b else i >= b) {
            for (r in rest) {
                out += before + i.toString().padStart(width, '0') + r
                if (out.size >= limit) return out
            }
            i += if (a <= b) step else -step
        }
    } else {
        val a = m.groupValues[4][0]
        val b = m.groupValues[5][0]
        for (c in minOf(a, b)..maxOf(a, b)) {
            for (r in rest) {
                out += before + c + r
                if (out.size >= limit) return out
            }
        }
    }
    return out
}

@Composable
fun BatchScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val queues by Ku.queues.collectAsStateWithLifecycle()
    var text by rememberSaveable { mutableStateOf("") }
    var pattern by rememberSaveable { mutableStateOf("") }
    var dir by remember { mutableStateOf<String?>(null) }
    var queueId by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(UiState.batch) {
        UiState.batch?.let {
            text = (text.lines().filter { l -> l.isNotBlank() } + it).joinToString("\n")
            UiState.batch = null
        }
    }
    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let { Files.treeToPath(it) }?.let { dir = it }
    }

    val urls = text.lines().map { it.trim() }.filter { BATCH_LINK.matches(it) }.distinct()
    val invalid = text.lines().count { it.isNotBlank() && !it.trim().startsWith("#") } - urls.size
    val preview = if (pattern.isNotBlank()) expandPattern(pattern.trim()) else emptyList()

    fun add(start: Boolean) {
        scope.launch {
            busy = true
            try {
                val template = buildJsonObject {
                    put("url", "")
                    dir?.let { put("dir", it) }
                    (queueId ?: "main").let { put("queueId", it) }
                    put("start", start)
                    put("source", "batch")
                }
                val r = Ku.addBatch(urls, template)
                val added = r["added"]?.jsonArray?.size ?: 0
                val failed = r["failed"]?.jsonArray.orEmpty()
                if (failed.isEmpty()) {
                    UiState.toast(tf("{n} added", "n" to added))
                    text = ""
                } else {
                    val first = failed.first().jsonObject
                    UiState.toast(tf("{added} added, {failed} failed", "added" to added, "failed" to failed.size) + ": " + te(first["error"]?.jsonPrimitive?.contentOrNull ?: ""))
                }
                if (start) KuService.ensure(ctx)
            } catch (e: Exception) {
                UiState.toast(e.message ?: t("Could not add the downloads"))
            } finally {
                busy = false
            }
        }
    }

    KuScaffold(t("Batch downloads"), back = true) { pad ->
        Column(Modifier.fillMaxSize().padding(pad).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(t("Paste one link per line. Lines starting with # are ignored."), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedTextField(
                text,
                { text = it },
                label = { Text(t("Links")) },
                minLines = 6,
                maxLines = 14,
                textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                tf("{count} links", "count" to urls.size) + if (invalid > 0) " · " + tf("{count} lines are not links", "count" to invalid) else "",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Card {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.AutoFixHigh, null, tint = MaterialTheme.colorScheme.primary)
                        Spacer(Modifier.width(8.dp))
                        Text(t("Generate from a pattern"), style = MaterialTheme.typography.titleSmall)
                    }
                    OutlinedTextField(
                        pattern,
                        { pattern = it },
                        placeholder = { Text("https://example.com/photo[001-120].jpg") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(t("[1-50] numbers, [001-050] zero-padded, [1-99:2] every second, [a-z] letters."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (preview.isNotEmpty()) {
                        Text(preview.take(3).joinToString("\n") + if (preview.size > 3) "\n…" else "", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                        FilledTonalButton({
                            text = (text.lines().filter { it.isNotBlank() } + preview).joinToString("\n")
                            pattern = ""
                        }) { Text(tf("Add {count} links to the list", "count" to preview.size)) }
                    }
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(t("Save to"), style = MaterialTheme.typography.labelMedium)
                    Text(dir ?: t("Automatic (by file type)"), style = MaterialTheme.typography.bodyMedium)
                }
                TextButton({ folderPicker.launch(null) }) { Text(t("Change")) }
            }
            ChoiceLine(t("Queue"), queueId ?: "main", queues.map { it.id to queueName(it.id, it.name) }) { queueId = it }

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                OutlinedButton({ add(false) }, Modifier.weight(1f), enabled = urls.isNotEmpty() && !busy) { Text(t("Add to queue")) }
                Button({ add(true) }, Modifier.weight(1f), enabled = urls.isNotEmpty() && !busy) { Text(tf("Download {count}", "count" to urls.size)) }
            }
        }
    }
}
