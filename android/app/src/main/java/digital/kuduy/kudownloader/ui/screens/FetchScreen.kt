package digital.kuduy.kudownloader.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.TravelExplore
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import digital.kuduy.kudownloader.core.GrabLink
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.EmptyState
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.UiState
import digital.kuduy.kudownloader.ui.glyphFor
import digital.kuduy.kudownloader.ui.normalizeUrl
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

private val PAGE_EXT = setOf("", "html", "htm", "php", "asp", "aspx", "jsp", "cgi", "shtml")

private fun nameOf(l: GrabLink): String = runCatching {
    val u = java.net.URI(l.url)
    java.net.URLDecoder.decode(u.path.trimEnd('/').substringAfterLast('/'), "UTF-8").ifBlank { u.host ?: l.url }
}.getOrDefault(l.url)

/** Fetch Projects: collect the links on a page and download the ones you pick. */
@Composable
fun FetchScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val prefill = UiState.grab
    var pageUrl by rememberSaveable { mutableStateOf(prefill?.pageUrl ?: "") }
    var links by remember { mutableStateOf(prefill?.links ?: emptyList()) }
    var loading by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var filter by remember { mutableStateOf("") }
    var filesOnly by remember { mutableStateOf(true) }
    val types = remember { mutableStateListOf<String>() }
    val selected = remember { mutableStateListOf<String>() }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(prefill) {
        prefill?.let {
            pageUrl = it.pageUrl ?: ""
            links = it.links
            selected.clear()
            types.clear()
        }
    }

    fun fetch() {
        scope.launch {
            loading = true
            error = null
            try {
                links = Ku.grab(normalizeUrl(pageUrl))
                UiState.grab = null
                selected.clear()
                types.clear()
            } catch (e: Exception) {
                error = e.message
            } finally {
                loading = false
            }
        }
    }

    val ext = { l: GrabLink -> (l.kind ?: "").lowercase() }
    val allTypes = links.map(ext).filter { it !in PAGE_EXT }.groupingBy { it }.eachCount().entries.sortedByDescending { it.value }.take(16)
    val q = filter.trim().lowercase()
    val visible = links.filter { l ->
        val e = ext(l)
        if (filesOnly && e in PAGE_EXT && !l.url.startsWith("magnet:")) return@filter false
        if (types.isNotEmpty() && e !in types) return@filter false
        q.isEmpty() || l.url.lowercase().contains(q) || (l.text ?: "").lowercase().contains(q)
    }
    val chosen = visible.filter { it.url in selected }.map { it.url }

    fun add(start: Boolean) {
        scope.launch {
            busy = true
            try {
                val template = buildJsonObject {
                    put("url", "")
                    put("queueId", "main")
                    put("start", start)
                    put("source", if (prefill != null) "browser" else "grabber")
                    put("options", buildJsonObject {
                        (pageUrl.takeIf { it.isNotBlank() })?.let { put("referer", it) }
                        put("cookies", Ku.json.encodeToJsonElement(prefill?.cookies.orEmpty()))
                        prefill?.userAgent?.let { put("userAgent", it) }
                    })
                }
                val r = Ku.addBatch(chosen, template)
                val added = r["added"]?.jsonArray?.size ?: 0
                val failed = r["failed"]?.jsonArray.orEmpty()
                UiState.toast(
                    if (failed.isEmpty()) tf("{n} added", "n" to added)
                    else tf("{added} added, {skipped} skipped", "added" to added, "skipped" to failed.size) + ": " + te(failed.first().jsonObject["error"]?.jsonPrimitive?.contentOrNull ?: ""),
                )
                selected.clear()
                if (start) KuService.ensure(ctx)
            } catch (e: Exception) {
                UiState.toast(e.message ?: t("Could not add the downloads"))
            } finally {
                busy = false
            }
        }
    }

    KuScaffold(
        t("Fetch Projects"),
        back = true,
        bottom = {
            if (chosen.isNotEmpty()) {
                Surface(tonalElevation = 3.dp) {
                    Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton({ add(false) }, Modifier.weight(1f), enabled = !busy) { Text(t("Add to queue")) }
                        Button({ add(true) }, Modifier.weight(1f), enabled = !busy) { Text(tf("Download {count}", "count" to chosen.size)) }
                    }
                }
            }
        },
    ) { pad ->
        LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(bottom = 24.dp)) {
            item {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedTextField(
                        pageUrl,
                        { pageUrl = it },
                        label = { Text(t("Page address")) },
                        leadingIcon = { Icon(Icons.Filled.Link, null) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Search),
                        keyboardActions = KeyboardActions(onSearch = { fetch() }),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (loading) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.weight(1f))
                        Button({ fetch() }, enabled = pageUrl.isNotBlank() && !loading) { Text(t("Find links")) }
                    }
                    error?.let { Notice(it) }
                }
            }
            if (links.isEmpty() && !loading) {
                item {
                    EmptyState(Icons.Filled.TravelExplore, t("Download everything on a page"), t("Enter a page address, or use “Download all links” from the browser's menu."))
                }
            }
            if (links.isNotEmpty()) {
                item {
                    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedTextField(filter, { filter = it }, placeholder = { Text(t("Filter")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            FilterChip(filesOnly, { filesOnly = !filesOnly }, { Text(t("Files only")) })
                            allTypes.forEach { (e, n) -> FilterChip(e in types, { if (e in types) types.remove(e) else types.add(e) }, { Text(".$e ($n)") }) }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            val all = visible.isNotEmpty() && visible.all { it.url in selected }
                            Checkbox(all, { on -> if (on) selected.addAll(visible.map { it.url }.filter { it !in selected }) else selected.removeAll(visible.map { it.url }.toSet()) })
                            Text(tf("{shown} of {total} links · {count} selected", "shown" to visible.size, "total" to links.size, "count" to chosen.size), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
            items(visible, key = { it.url }) { l ->
                val g = glyphFor(nameOf(l))
                Row(
                    Modifier.fillMaxWidth().clickable { if (l.url in selected) selected.remove(l.url) else selected.add(l.url) }.padding(horizontal = 16.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Checkbox(l.url in selected, { on -> if (on) selected.add(l.url) else selected.remove(l.url) })
                    Icon(g.icon, null, tint = g.tint, modifier = Modifier.size(22.dp))
                    Spacer(Modifier.width(10.dp))
                    Column(Modifier.weight(1f)) {
                        Text(nameOf(l), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(l.text?.takeIf { it.isNotBlank() } ?: l.url, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}
