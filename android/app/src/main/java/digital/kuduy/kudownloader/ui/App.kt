package digital.kuduy.kudownloader.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.SmartDisplay
import androidx.compose.material.icons.filled.WifiTethering
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.AirMessage
import digital.kuduy.kudownloader.core.AirRequest
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.t
import digital.kuduy.kudownloader.i18n.te
import digital.kuduy.kudownloader.i18n.tf
import digital.kuduy.kudownloader.ui.screens.AboutScreen
import digital.kuduy.kudownloader.ui.screens.AddSheet
import digital.kuduy.kudownloader.ui.screens.AirSendPrompts
import digital.kuduy.kudownloader.ui.screens.AirSendScreen
import digital.kuduy.kudownloader.ui.screens.BatchScreen
import digital.kuduy.kudownloader.ui.screens.BrowserScreen
import digital.kuduy.kudownloader.ui.screens.DetailsSheet
import digital.kuduy.kudownloader.ui.screens.DownloadsScreen
import digital.kuduy.kudownloader.ui.screens.FetchScreen
import digital.kuduy.kudownloader.ui.screens.MoreScreen
import digital.kuduy.kudownloader.ui.screens.QueuesScreen
import digital.kuduy.kudownloader.ui.screens.RemoteSendSheet
import digital.kuduy.kudownloader.ui.screens.SettingsScreen
import digital.kuduy.kudownloader.ui.screens.VideoScreen
import digital.kuduy.kudownloader.ui.screens.WelcomeDialog
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonPrimitive

@Composable
fun KuRoot() {
    KuTheme {
        val phase by Ku.phase.collectAsStateWithLifecycle()
        when (phase) {
            Ku.Phase.Starting -> Splash()
            Ku.Phase.Failed -> StartFailed()
            Ku.Phase.Ready -> Main()
        }
    }
}

@Composable
private fun Splash() {
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            Spacer(Modifier.height(20.dp))
            Text(t("Preparing KuDownloader…"), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(6.dp))
            Text(
                t("The first start unpacks the video tools; this takes a few seconds."),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
private fun StartFailed() {
    val err by Ku.startError.collectAsStateWithLifecycle()
    Column(Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.Center) {
        Text(t("KuDownloader could not start"), style = MaterialTheme.typography.headlineSmall)
        Spacer(Modifier.height(12.dp))
        Notice(err ?: "")
    }
}

private data class Tab(val screen: Screen, val label: String, val icon: ImageVector)

@Composable
private fun Main() {
    val snack = remember { SnackbarHostState() }
    val screen = UiState.stack.last()
    val active by Ku.stats.collectAsStateWithLifecycle()
    val requests = UiState.airRequests.size

    BackHandler(enabled = UiState.stack.size > 1 || screen != Screen.Downloads) { UiState.back() }

    // Engine events the interface shows (notifications cover the background).
    LaunchedEffect(Unit) {
        Ku.events.collect { e ->
            val str = { k: String -> e[k]?.jsonPrimitive?.contentOrNull }
            when (str("type")) {
                "notice" -> UiState.toast(listOfNotNull(str("title")?.let { te(it) }, str("message")?.let { te(it) }).filter { it.isNotBlank() }.joinToString(": "))
                "airSendRequest" -> UiState.airRequests.add(Ku.json.decodeFromJsonElement<AirRequest>(e["request"]!!))
                "airSendTrust" -> {
                    val r = Ku.json.decodeFromJsonElement<digital.kuduy.kudownloader.core.AirTrustRequest>(e["request"]!!)
                    UiState.airTrusts.removeAll { it.peerFingerprint == r.peerFingerprint }
                    UiState.airTrusts.add(r)
                }
                "airSendDownload" -> UiState.airDownloads.add(Ku.json.decodeFromJsonElement<digital.kuduy.kudownloader.core.AirDownloadRequest>(e["request"]!!))
                "airSendMessage" -> UiState.airMessages.add(Ku.json.decodeFromJsonElement<AirMessage>(e["message"]!!))
                "toolDone" -> if (e["ok"]?.jsonPrimitive?.contentOrNull == "false") UiState.toast(te(str("message") ?: ""))
                "queueDone" -> UiState.toast(tf("All downloads in {name} are done.", "name" to (str("name") ?: "")))
            }
        }
    }
    LaunchedEffect(Unit) {
        while (true) {
            if (UiState.toasts.isNotEmpty()) {
                val msg = UiState.toasts.removeAt(0)
                if (msg.isNotBlank()) snack.showSnackbar(msg, withDismissAction = true, duration = SnackbarDuration.Short)
            } else {
                kotlinx.coroutines.delay(150)
            }
        }
    }
    val offer = UiState.clipboardOffer
    LaunchedEffect(offer) {
        if (offer != null) {
            val r = snack.showSnackbar(tf("Download the copied link? {url}", "url" to offer.take(80)), actionLabel = t("Download"), withDismissAction = true, duration = SnackbarDuration.Long)
            if (r == SnackbarResult.ActionPerformed) UiState.add = AddPrefill(url = offer, source = "clipboard")
            UiState.clipboardOffer = null
        }
    }

    val tabs = listOf(
        Tab(Screen.Downloads, t("Downloads"), Icons.Filled.Download),
        Tab(Screen.Browser, t("Browser"), Icons.Filled.Public),
        Tab(Screen.Video, t("Video"), Icons.Filled.SmartDisplay),
        Tab(Screen.AirSend, "AirSend", Icons.Filled.WifiTethering),
        Tab(Screen.More, t("More"), Icons.Filled.MoreHoriz),
    )
    val topScreen = UiState.stack.first()
    Scaffold(
        snackbarHost = { SnackbarHost(snack) },
        bottomBar = {
            if (screen.top) {
                NavigationBar {
                    tabs.forEach { tab ->
                        NavigationBarItem(
                            selected = topScreen == tab.screen,
                            onClick = { UiState.go(tab.screen) },
                            icon = {
                                when {
                                    tab.screen == Screen.Downloads && active.active > 0 -> BadgedBox(badge = { Badge { Text("${active.active}") } }) { Icon(tab.icon, null) }
                                    tab.screen == Screen.AirSend && requests > 0 -> BadgedBox(badge = { Badge { Text("$requests") } }) { Icon(tab.icon, null) }
                                    else -> Icon(tab.icon, null)
                                }
                            },
                            label = { Text(tab.label, maxLines = 1, softWrap = false, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis, style = MaterialTheme.typography.labelSmall) },
                        )
                    }
                }
            }
        },
    ) { pad ->
        Box(Modifier.fillMaxSize().padding(bottom = pad.calculateBottomPadding())) {
            AnimatedContent(screen, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "screen") { s ->
                when (s) {
                    Screen.Downloads -> DownloadsScreen()
                    Screen.Browser -> BrowserScreen()
                    Screen.Video -> VideoScreen()
                    Screen.AirSend -> AirSendScreen()
                    Screen.More -> MoreScreen()
                    Screen.Batch -> BatchScreen()
                    Screen.Fetch -> FetchScreen()
                    Screen.Queues -> QueuesScreen()
                    Screen.Settings -> SettingsScreen()
                    Screen.About -> AboutScreen()
                }
            }
        }
    }

    UiState.add?.let { AddSheet(it) { UiState.add = null } }
    UiState.details?.let { id -> DetailsSheet(id) { UiState.details = null } }
    AirSendPrompts()
    UiState.remoteSend?.let { RemoteSendSheet(it) { UiState.remoteSend = null } }
    val welcomed by Prefs.welcomed.state.collectAsStateWithLifecycle()
    if (!welcomed) WelcomeDialog { Prefs.welcomed.value = true }
}
