package digital.kuduy.kudownloader.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.InsertDriveFile
import androidx.compose.material.icons.filled.Album
import androidx.compose.material.icons.filled.Android
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.Audiotrack
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import digital.kuduy.kudownloader.core.Download
import digital.kuduy.kudownloader.core.Fmt
import digital.kuduy.kudownloader.i18n.t

private val LINK = Regex("""^(?:(?:https?|ftp|sftp)://\S+|magnet:\?\S+)$""", RegexOption.IGNORE_CASE)

fun looksLikeLink(s: String) = s.length < 8192 && LINK.matches(s.trim())

/** Accept "example.com/file.zip" too. */
fun normalizeUrl(s: String): String {
    val v = s.trim()
    if (v.isEmpty() || v.contains("://") || v.startsWith("magnet:", true)) return v
    return if (v.contains('.') && !v.contains(' ')) "https://$v" else v
}

data class Glyph(val icon: ImageVector, val tint: Color)

fun glyphFor(name: String, category: String = "", kind: String = ""): Glyph {
    val ext = Fmt.extension(name)
    return when {
        kind == "torrent" || kind == "magnet" -> Glyph(Icons.Filled.Hub, Color(0xFF16A34A))
        kind == "media" || category == "video" || ext in setOf("mp4", "mkv", "webm", "avi", "mov", "m4v", "ts", "3gp", "flv") -> Glyph(Icons.Filled.Movie, Color(0xFFDB2777))
        category == "music" || ext in setOf("mp3", "m4a", "flac", "wav", "ogg", "opus", "aac") -> Glyph(Icons.Filled.Audiotrack, Color(0xFF7C3AED))
        ext == "apk" -> Glyph(Icons.Filled.Android, Color(0xFF3DDC84))
        category == "archives" || ext in setOf("zip", "rar", "7z", "tar", "gz", "xz", "bz2", "zst") -> Glyph(Icons.Filled.Archive, Color(0xFFEA580C))
        category == "images-disk" || ext in setOf("iso", "img", "dmg") -> Glyph(Icons.Filled.Album, Color(0xFF0D9488))
        ext == "pdf" -> Glyph(Icons.Filled.PictureAsPdf, Color(0xFFDC2626))
        category == "documents" -> Glyph(Icons.Filled.Description, Color(0xFF2563EB))
        category == "images" -> Glyph(Icons.Filled.Image, Color(0xFF0891B2))
        category == "programs" -> Glyph(Icons.Filled.Apps, Color(0xFF52525B))
        else -> Glyph(Icons.AutoMirrored.Filled.InsertDriveFile, Color(0xFF64748B))
    }
}

@Composable
fun FileGlyph(d: Download, size: Int = 40) {
    val g = glyphFor(d.name, d.category, d.kind)
    Box(
        Modifier.size(size.dp).clip(RoundedCornerShape(10.dp)).background(g.tint.copy(alpha = 0.14f)),
        contentAlignment = Alignment.Center,
    ) { Icon(g.icon, null, tint = g.tint, modifier = Modifier.size((size * 0.55f).dp)) }
}

/** Download (accent) and upload (green) speed over the last minute. */
@Composable
fun SpeedGraph(samples: List<Pair<Long, Long>>, modifier: Modifier = Modifier) {
    val down = MaterialTheme.colorScheme.primary
    val up = LocalKuColors.current.upload
    val grid = MaterialTheme.colorScheme.outlineVariant
    Canvas(modifier) {
        val max = (samples.maxOfOrNull { maxOf(it.first, it.second) } ?: 0L).coerceAtLeast(64 * 1024).toFloat()
        val n = samples.size.coerceAtLeast(2)
        val stepX = size.width / (n - 1)
        for (i in 1..3) {
            val y = size.height * i / 4f
            drawLine(grid, Offset(0f, y), Offset(size.width, y), strokeWidth = 1f)
        }
        fun line(values: List<Long>, color: Color, fill: Boolean) {
            val path = Path()
            values.forEachIndexed { i, v ->
                val x = i * stepX
                val y = size.height - (v / max) * (size.height - 4f) - 2f
                if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            if (fill) {
                val area = Path().apply {
                    addPath(path)
                    lineTo((values.size - 1) * stepX, size.height)
                    lineTo(0f, size.height)
                    close()
                }
                drawPath(area, Brush.verticalGradient(listOf(color.copy(alpha = 0.28f), color.copy(alpha = 0.02f))))
            }
            drawPath(path, color, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
        line(samples.map { it.first }, down, true)
        if (samples.any { it.second > 0 }) line(samples.map { it.second }, up, false)
    }
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.primary,
        modifier = modifier.padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 6.dp),
    )
}

@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Surface(
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        content = content,
    )
}

@Composable
fun SwitchRow(title: String, checked: Boolean, subtitle: String? = null, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(enabled = enabled) { onChange(!checked) }.padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        Spacer(Modifier.width(12.dp))
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled)
    }
}

@Composable
fun ClickRow(title: String, subtitle: String? = null, icon: ImageVector? = null, trailing: (@Composable RowScope.() -> Unit)? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon?.let {
            // Icons sit in a soft tinted tile.
            Box(
                Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.55f)),
                contentAlignment = Alignment.Center,
            ) { Icon(it, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(22.dp)) }
            Spacer(Modifier.width(16.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
        trailing?.invoke(this)
    }
}

/** A row that opens a menu of choices. */
@Composable
fun <T> ChoiceRow(title: String, value: T, choices: List<Pair<T, String>>, subtitle: String? = null, onChange: (T) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        ClickRow(title, subtitle ?: choices.firstOrNull { it.first == value }?.second ?: value.toString()) { open = true }
        DropdownMenu(open, { open = false }) {
            choices.forEach { (v, label) ->
                DropdownMenuItem(
                    text = { Text(label, fontWeight = if (v == value) FontWeight.SemiBold else FontWeight.Normal) },
                    onClick = {
                        open = false
                        onChange(v)
                    },
                )
            }
        }
    }
}

/** A row that edits a text or number value in a dialog. */
@Composable
fun TextRow(title: String, value: String, subtitle: String? = null, placeholder: String = "", number: Boolean = false, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    ClickRow(title, subtitle ?: value.ifBlank { placeholder.ifBlank { t("Not set") } }) { open = true }
    if (open) {
        var text by remember { mutableStateOf(value) }
        AlertDialog(
            onDismissRequest = { open = false },
            title = { Text(title) },
            text = {
                OutlinedTextField(
                    text,
                    { text = if (number) it.filter { c -> c.isDigit() } else it },
                    singleLine = true,
                    placeholder = { Text(placeholder) },
                    keyboardOptions = if (number) androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number) else androidx.compose.foundation.text.KeyboardOptions.Default,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                TextButton({
                    open = false
                    onChange(text.trim())
                }) { Text(t("Save")) }
            },
            dismissButton = { TextButton({ open = false }) { Text(t("Cancel")) } },
        )
    }
}

@Composable
fun ConfirmDialog(
    title: String,
    text: String,
    confirm: String,
    danger: Boolean = false,
    onDismiss: () -> Unit,
    extra: (@Composable () -> Unit)? = null,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                Text(text)
                extra?.invoke()
            }
        },
        confirmButton = {
            if (danger) {
                androidx.compose.material3.Button(
                    onClick = onConfirm,
                    colors = androidx.compose.material3.ButtonDefaults.buttonColors(containerColor = Danger, contentColor = Color.White),
                ) { Text(confirm) }
            } else {
                TextButton(onConfirm) { Text(confirm) }
            }
        },
        dismissButton = { TextButton(onDismiss) { Text(t("Cancel")) } },
    )
}

@Composable
fun Pill(text: String, color: Color, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = color,
        modifier = modifier.clip(CircleShape).background(color.copy(alpha = 0.14f)).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

@Composable
fun EmptyState(icon: ImageVector, title: String, body: String, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
    Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Box(Modifier.size(72.dp).clip(CircleShape).background(MaterialTheme.colorScheme.primaryContainer), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = MaterialTheme.colorScheme.onPrimaryContainer, modifier = Modifier.size(36.dp))
        }
        Spacer(Modifier.height(16.dp))
        Text(title, style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(6.dp))
        Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        action?.let {
            Spacer(Modifier.height(16.dp))
            it()
        }
    }
}

@Composable
fun Notice(text: String, color: Color = MaterialTheme.colorScheme.error, modifier: Modifier = Modifier) {
    Surface(color = color.copy(alpha = 0.12f), shape = RoundedCornerShape(12.dp), modifier = modifier.fillMaxWidth()) {
        Text(text, color = color, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
    }
}

/**
 * A screen with a top bar. The app's outer scaffold already keeps clear of
 * the navigation bar, so only the top bar handles the status bar here.
 */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun KuScaffold(
    title: String,
    back: Boolean = false,
    actions: @Composable RowScope.() -> Unit = {},
    fab: @Composable () -> Unit = {},
    bottom: @Composable () -> Unit = {},
    content: @Composable (androidx.compose.foundation.layout.PaddingValues) -> Unit,
) {
    androidx.compose.material3.Scaffold(
        topBar = {
            androidx.compose.material3.TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold) },
                navigationIcon = {
                    if (back) {
                        androidx.compose.material3.IconButton({ UiState.back() }) {
                            Icon(androidx.compose.material.icons.Icons.AutoMirrored.Filled.ArrowBack, t("Back"))
                        }
                    }
                },
                actions = actions,
            )
        },
        floatingActionButton = fab,
        bottomBar = bottom,
        contentWindowInsets = androidx.compose.foundation.layout.WindowInsets(0, 0, 0, 0),
        content = content,
    )
}

/** A titled group of rows in one rounded card (settings-style lists). */
@Composable
fun Group(title: String? = null, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        title?.let {
            Text(it, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(start = 8.dp, top = 10.dp, bottom = 8.dp))
        }
        Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
            Column { content() }
        }
    }
}
