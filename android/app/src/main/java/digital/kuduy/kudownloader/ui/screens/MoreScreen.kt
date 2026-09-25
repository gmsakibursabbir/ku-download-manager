package digital.kuduy.kudownloader.ui.screens

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Queue
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.TravelExplore
import androidx.compose.material.icons.filled.WifiTethering
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import digital.kuduy.kudownloader.BuildConfig
import digital.kuduy.kudownloader.R
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.ui.ClickRow
import digital.kuduy.kudownloader.ui.KuScaffold
import digital.kuduy.kudownloader.ui.Notice
import digital.kuduy.kudownloader.ui.PixelAnimal
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.SectionTitle
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.net.HttpURLConnection
import java.net.URL

const val REPO = "https://github.com/kuduyDigital/ku-download-manager"

@Composable
fun MoreScreen() {
    KuScaffold(t("More")) { pad ->
        Column(Modifier.fillMaxSize().padding(pad).verticalScroll(rememberScrollState())) {
            ClickRow(t("Batch downloads"), t("Many links at once, or a numbered pattern"), Icons.AutoMirrored.Filled.PlaylistAdd) { UiState.go(Screen.Batch) }
            ClickRow(t("Fetch Projects"), t("Download the files linked on a page"), Icons.Filled.TravelExplore) { UiState.go(Screen.Fetch) }
            ClickRow(t("Queues and schedules"), t("Start downloads at night or one after another"), Icons.Filled.Queue) { UiState.go(Screen.Queues) }
            ClickRow("KuAirSend", t("Send files to nearby KuDownloader devices"), Icons.Filled.WifiTethering) { UiState.go(Screen.AirSend) }
            ClickRow(t("Settings"), null, Icons.Filled.Settings) {
                UiState.settingsSection = null
                UiState.go(Screen.Settings)
            }
            ClickRow(t("About KuDownloader"), "${t("Version")} ${BuildConfig.VERSION_NAME}", Icons.Filled.Info) { UiState.go(Screen.About) }
        }
    }
}

@Composable
fun AboutScreen() {
    val ctx = LocalContext.current
    var checking by remember { mutableStateOf(true) }
    var latest by remember { mutableStateOf<Pair<String, String>?>(null) }
    // Releases are published on GitHub (the Play Store does not allow video downloaders).
    LaunchedEffect(Unit) {
        latest = withContext(Dispatchers.IO) {
            runCatching {
                val c = URL("https://api.github.com/repos/kuduyDigital/ku-download-manager/releases/latest").openConnection() as HttpURLConnection
                c.setRequestProperty("Accept", "application/vnd.github+json")
                c.connectTimeout = 10_000
                c.readTimeout = 10_000
                val o = Ku.json.parseToJsonElement(c.inputStream.bufferedReader().use { it.readText() }).jsonObject
                val tag = o["tag_name"]?.jsonPrimitive?.contentOrNull ?: return@runCatching null
                val apk = o["assets"]?.jsonArray?.map { it.jsonObject }?.firstOrNull { a ->
                    val n = a["name"]?.jsonPrimitive?.contentOrNull.orEmpty()
                    n.endsWith(".apk") && (n.contains(Build.SUPPORTED_ABIS.firstOrNull() ?: "arm64-v8a") || n.contains("universal"))
                }?.get("browser_download_url")?.jsonPrimitive?.contentOrNull ?: o["html_url"]?.jsonPrimitive?.contentOrNull ?: REPO
                tag to apk
            }.getOrNull()
        }
        checking = false
    }
    KuScaffold(t("About KuDownloader"), back = true) { pad ->
        Column(Modifier.fillMaxSize().padding(pad).verticalScroll(rememberScrollState()).padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            androidx.compose.foundation.layout.Box(Modifier.size(88.dp).clip(RoundedCornerShape(22.dp)).background(androidx.compose.ui.graphics.Color(0xFF2563EB))) {
                androidx.compose.foundation.Image(painterResource(R.drawable.ic_launcher_foreground), null, Modifier.size(88.dp))
            }
            Text("KuDownloader", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
            Text("${t("Version")} ${BuildConfig.VERSION_NAME}", color = MaterialTheme.colorScheme.onSurfaceVariant)
            when {
                checking -> CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                latest != null && isNewer(latest!!.first, BuildConfig.VERSION_NAME) -> {
                    Notice(tf("Version {version} is available.", "version" to latest!!.first.removePrefix("v")), MaterialTheme.colorScheme.primary)
                    Button({ ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(latest!!.second))) }) { Text(t("Download the update")) }
                }
                latest != null -> Text(t("You have the latest version."), style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(8.dp))
            Text(
                t("A fast download manager: resumable multi-connection downloads, videos and music from 1,000+ sites, torrents, queues and schedules, a private browser with an ad blocker, and KuAirSend for nearby devices."),
                style = MaterialTheme.typography.bodyMedium,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton({ ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(REPO))) }) { Text(t("Source code")) }
                TextButton({ ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://kuduydigital.github.io/ku-download-manager/privacy.html"))) }) { Text(t("Privacy")) }
            }
            SectionTitle(t("Built with"))
            Text("yt-dlp · FFmpeg · aria2 · Brave adblock-rust · youtubedl-android · Jetpack Compose · Rust", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

fun isNewer(candidate: String, current: String): Boolean {
    fun parts(v: String) = v.removePrefix("v").substringBefore('-').split('.').map { it.toIntOrNull() ?: 0 }
    if (candidate.contains('-')) return false
    val a = parts(candidate)
    val b = parts(current)
    for (i in 0 until maxOf(a.size, b.size)) {
        val x = a.getOrElse(i) { 0 }
        val y = b.getOrElse(i) { 0 }
        if (x != y) return x > y
    }
    return false
}

/** First start: what the app needs, asked for once. */
@Composable
fun WelcomeDialog(onDone: () -> Unit) {
    val notify = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { onDone() }
    val storage = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        if (Build.VERSION.SDK_INT >= 33) notify.launch(Manifest.permission.POST_NOTIFICATIONS) else onDone()
    }
    AlertDialog(
        onDismissRequest = {},
        icon = { PixelAnimal("cat", 64.dp) },
        title = { Text(t("Welcome to KuDownloader")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(t("Share a link to KuDownloader, paste it with +, or browse to a video and tap KuDownload."))
                Text(t("Downloads keep going with the screen off. Allow notifications to see their progress."), style = MaterialTheme.typography.bodySmall)
                Text(t("Files are saved in Download/KuDownloader."), style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            TextButton({
                when {
                    Build.VERSION.SDK_INT <= 28 -> storage.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
                    Build.VERSION.SDK_INT >= 33 -> notify.launch(Manifest.permission.POST_NOTIFICATIONS)
                    else -> onDone()
                }
            }) { Text(t("Continue")) }
        },
    )
}

