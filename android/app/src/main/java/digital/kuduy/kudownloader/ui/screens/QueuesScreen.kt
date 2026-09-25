package digital.kuduy.kudownloader.ui.screens

import android.app.AlarmManager
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Queue
import digital.kuduy.kudownloader.core.Schedule
import digital.kuduy.kudownloader.i18n.I18n
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.service.KuService
import digital.kuduy.kudownloader.ui.Card
import digital.kuduy.kudownloader.ui.ConfirmDialog
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.LocalKuColors
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.Pill
import digital.kuduy.kudownloader.ui.SectionTitle
import java.time.DayOfWeek
import java.time.format.TextStyle
import java.util.Locale
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Composable
fun QueuesScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val queues by Ku.queues.collectAsStateWithLifecycle()
    val schedules by Ku.schedules.collectAsStateWithLifecycle()
    val downloads by Ku.downloads.collectAsStateWithLifecycle()
    var editQueue by remember { mutableStateOf<Queue?>(null) }
    var editSchedule by remember { mutableStateOf<Schedule?>(null) }
    var deleteQueue by remember { mutableStateOf<Queue?>(null) }
    val exactAllowed = Build.VERSION.SDK_INT < 31 || ctx.getSystemService(AlarmManager::class.java).canScheduleExactAlarms()

    KuScaffold(t("Queues and schedules"), back = true) { pad ->
        LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(bottom = 32.dp)) {
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    SectionTitle(t("Queues"), Modifier.weight(1f))
                    TextButton({ editQueue = Queue(name = t("New queue")) }) { Icon(Icons.Filled.Add, null); Text(t("New queue")) }
                }
            }
            items(queues, key = { "q" + it.id }) { q ->
                val count = downloads.values.count { (it.queueId ?: "main") == q.id && !it.isFinished }
                Card {
                    Row(Modifier.padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(queueName(q.id, q.name), style = MaterialTheme.typography.titleSmall)
                                if (q.running) {
                                    Spacer(Modifier.width(8.dp))
                                    Pill(t("Running"), LocalKuColors.current.success)
                                }
                            }
                            val facts = mutableListOf(tf("{count} waiting", "count" to count), tf("{n} at a time", "n" to q.maxConcurrent))
                            if (q.syncMinutes > 0) facts += tf("sync every {n} min", "n" to q.syncMinutes)
                            Text(facts.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        if (q.running) {
                            IconButton({ scope.act { Ku.stopQueue(q.id) } }) { Icon(Icons.Filled.Stop, t("Stop")) }
                        } else {
                            IconButton({ scope.act { Ku.startQueue(q.id); KuService.ensure(ctx) } }) { Icon(Icons.Filled.PlayArrow, t("Start"), tint = MaterialTheme.colorScheme.primary) }
                        }
                        IconButton({ editQueue = q }) { Icon(Icons.Filled.Edit, t("Edit")) }
                        if (q.id != "main") IconButton({ deleteQueue = q }) { Icon(Icons.Filled.Delete, t("Delete")) }
                    }
                }
            }
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    SectionTitle(t("Schedules"), Modifier.weight(1f))
                    TextButton({ editSchedule = Schedule(name = t("Night downloads")) }) { Icon(Icons.Filled.Add, null); Text(t("New schedule")) }
                }
            }
            if (!exactAllowed) {
                item {
                    Column(Modifier.padding(horizontal = 12.dp)) {
                        Notice(t("Allow alarms so schedules start on time."), MaterialTheme.colorScheme.tertiary)
                        TextButton({
                            if (Build.VERSION.SDK_INT >= 31) runCatching { ctx.startActivity(Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM).setData(android.net.Uri.parse("package:" + ctx.packageName))) }
                        }) { Text(t("Allow")) }
                    }
                }
            }
            if (schedules.isEmpty()) {
                item { Text(t("No schedules yet. A schedule starts a queue at a set time, for example at night."), Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            items(schedules, key = { "s" + it.id }) { s ->
                Card {
                    Row(Modifier.padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Schedule, null, tint = MaterialTheme.colorScheme.primary)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(s.name, style = MaterialTheme.typography.titleSmall)
                            Text(describe(s, queues), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        Switch(s.enabled, { on -> scope.act { Ku.saveSchedule(s.copy(enabled = on)) } })
                        IconButton({ editSchedule = s }) { Icon(Icons.Filled.Edit, t("Edit")) }
                    }
                }
            }
        }
    }

    editQueue?.let { q -> QueueDialog(q) { editQueue = null } }
    editSchedule?.let { s -> ScheduleDialog(s) { editSchedule = null } }
    deleteQueue?.let { q ->
        ConfirmDialog(t("Delete queue?"), tf("Downloads in {name} move to the main queue.", "name" to q.name), t("Delete"), danger = true, onDismiss = { deleteQueue = null }) {
            deleteQueue = null
            scope.act { Ku.deleteQueue(q.id) }
        }
    }
}

private fun dayName(d: Int): String = DayOfWeek.of(d).getDisplayName(TextStyle.SHORT, Locale.forLanguageTag(I18n.lang))

private fun describe(s: Schedule, queues: List<Queue>): String {
    val q = queues.firstOrNull { it.id == s.queueId }?.let { queueName(it.id, it.name) } ?: s.queueId
    val days = when {
        !s.date.isNullOrBlank() -> s.date
        s.days.isEmpty() || s.days.size == 7 -> t("Every day")
        else -> s.days.sorted().joinToString(", ") { dayName(it) }
    }
    val time = s.stop?.let { "${s.start}–$it" } ?: s.start
    return "$days · $time · $q"
}

@Composable
private fun QueueDialog(q: Queue, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf(queueName(q.id, q.name)) }
    var max by remember { mutableStateOf(q.maxConcurrent.toFloat()) }
    var sync by remember { mutableStateOf(if (q.syncMinutes > 0) q.syncMinutes.toString() else "") }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(if (q.id.isEmpty()) t("New queue") else t("Edit queue")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(name, { name = it }, label = { Text(t("Name")) }, singleLine = true)
                Text(tf("Downloads at the same time: {n}", "n" to max.toInt()), style = MaterialTheme.typography.labelLarge)
                Slider(max, { max = it }, valueRange = 1f..8f, steps = 6)
                OutlinedTextField(sync, { sync = it.filter { c -> c.isDigit() } }, label = { Text(t("Synchronize every (minutes)")) }, placeholder = { Text(t("Off")) }, singleLine = true)
                Text(t("Synchronization re-checks finished files on the server and downloads the ones that changed."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        },
        confirmButton = {
            TextButton({
                onClose()
                val keepName = if (q.id == "main" && name == t("Main queue")) q.name else name.trim()
                scope.act { Ku.saveQueue(q.copy(name = keepName, maxConcurrent = max.toInt(), syncMinutes = sync.toIntOrNull() ?: 0)) }
            }, enabled = name.isNotBlank()) { Text(t("Save")) }
        },
        dismissButton = { TextButton(onClose) { Text(t("Cancel")) } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ScheduleDialog(s: Schedule, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val queues by Ku.queues.collectAsStateWithLifecycle()
    val settings by Ku.settings.collectAsStateWithLifecycle()
    val profiles = (settings["profiles"] as? JsonArray)?.map { it.jsonObject }.orEmpty()
    var name by remember { mutableStateOf(s.name) }
    var queueId by remember { mutableStateOf(s.queueId) }
    val days = remember { mutableStateListOf<Int>().apply { addAll(s.days) } }
    var start by remember { mutableStateOf(s.start) }
    var stop by remember { mutableStateOf(s.stop) }
    var profile by remember { mutableStateOf(s.profile) }
    var picking by remember { mutableStateOf<String?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(if (s.id.isEmpty()) t("New schedule") else t("Edit schedule")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                OutlinedTextField(name, { name = it }, label = { Text(t("Name")) }, singleLine = true)
                ChoiceLine(t("Queue"), queueId, queues.map { it.id to queueName(it.id, it.name) }) { queueId = it }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(t("Start at"), Modifier.weight(1f))
                    TextButton({ picking = "start" }) { Text(start, fontWeight = FontWeight.SemiBold) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(t("Stop at"), Modifier.weight(1f))
                    TextButton({ picking = "stop" }) { Text(stop ?: t("Never")) }
                    if (stop != null) TextButton({ stop = null }) { Text(t("Clear")) }
                }
                Text(t("Days"), style = MaterialTheme.typography.labelLarge)
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    (1..7).forEach { d -> FilterChip(d in days, { if (d in days) days.remove(d) else days.add(d) }, { Text(dayName(d)) }) }
                }
                Text(if (days.isEmpty()) t("Every day") else "", style = MaterialTheme.typography.bodySmall)
                ChoiceLine(
                    t("Speed while running"),
                    profile ?: "",
                    listOf("" to t("Don't change")) + profiles.mapNotNull { p -> p["id"]?.jsonPrimitive?.contentOrNull?.let { it to t(p["name"]?.jsonPrimitive?.contentOrNull ?: it) } },
                ) { profile = it.ifBlank { null } }
            }
        },
        confirmButton = {
            TextButton({
                onClose()
                scope.act { Ku.saveSchedule(s.copy(name = name.trim(), queueId = queueId, days = days.sorted(), start = start, stop = stop, profile = profile, after = "none")) }
            }, enabled = name.isNotBlank()) { Text(t("Save")) }
        },
        dismissButton = {
            Row {
                if (s.id.isNotEmpty()) TextButton({ confirmDelete = true }) { Text(t("Delete"), color = MaterialTheme.colorScheme.error) }
                TextButton(onClose) { Text(t("Cancel")) }
            }
        },
    )
    picking?.let { which ->
        val init = (if (which == "start") start else stop ?: "06:00").split(":")
        val state = rememberTimePickerState(init.getOrNull(0)?.toIntOrNull() ?: 2, init.getOrNull(1)?.toIntOrNull() ?: 0, is24Hour = true)
        AlertDialog(
            onDismissRequest = { picking = null },
            text = { TimePicker(state) },
            confirmButton = {
                TextButton({
                    val v = "%02d:%02d".format(state.hour, state.minute)
                    if (which == "start") start = v else stop = v
                    picking = null
                }) { Text(t("OK")) }
            },
            dismissButton = { TextButton({ picking = null }) { Text(t("Cancel")) } },
        )
    }
    if (confirmDelete) {
        ConfirmDialog(t("Delete schedule?"), s.name, t("Delete"), danger = true, onDismiss = { confirmDelete = false }) {
            confirmDelete = false
            onClose()
            scope.act { Ku.deleteSchedule(s.id) }
        }
    }
}

