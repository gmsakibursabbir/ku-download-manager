package digital.kuduy.kudownloader

import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import com.android.resources.NightMode
import digital.kuduy.kudownloader.core.AirPeer
import digital.kuduy.kudownloader.core.AirStatus
import digital.kuduy.kudownloader.core.AirTransfer
import digital.kuduy.kudownloader.core.Download
import digital.kuduy.kudownloader.core.DownloadMeta
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.ui.KuRoot
import digital.kuduy.kudownloader.ui.Screen
import digital.kuduy.kudownloader.ui.UiState
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * Renders every main screen with sample data, light and dark, as PNGs
 * (app/build/outputs/paparazzi) for design review. No device or engine needed.
 */
@RunWith(Parameterized::class)
class ScreensTest(private val dark: Boolean) {
    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "dark={0}")
        fun modes() = listOf(false, true)

        private const val GB = 1024L * 1024 * 1024
        private const val MB = 1024L * 1024
        private val now = System.currentTimeMillis()

        val samples = listOf(
            Download(id = "1", name = "ubuntu-24.04.1-desktop-amd64.iso", category = "images-disk", status = "downloading", total = 58 * GB / 10, done = 39 * GB / 10, speed = 24 * MB, activeConnections = 16, createdAt = now),
            Download(id = "2", name = "Switzerland 4K – Breathtaking Nature Scenery.mp4", kind = "media", engine = "ytdlp", category = "video", status = "downloading", total = 342 * MB, done = 120 * MB, speed = 8 * MB, createdAt = now - 1000),
            Download(id = "3", name = "The.Last.of.Us.S01E01.1080p.mkv", kind = "torrent", category = "torrents", status = "seeding", total = 34 * GB / 10, done = 34 * GB / 10, uploadSpeed = 400 * 1024, createdAt = now - 2000),
            Download(id = "4", name = "dataset-2026-q3.tar.gz", category = "archives", status = "queued", queueId = "main", total = 12 * GB, createdAt = now - 3000),
            Download(id = "5", name = "report-final-v3.pdf", category = "documents", status = "error", total = 2 * MB, error = "The server refused access (HTTP 403). The link may have expired.", createdAt = now - 4000),
            Download(id = "6", name = "node-v22.9.0-x64.msi", category = "programs", status = "paused", total = 30 * MB, done = 12 * MB, createdAt = now - 5000),
            Download(id = "7", name = "Album – Live at the Hall.flac", category = "music", status = "completed", total = 412 * MB, done = 412 * MB, completedAt = now - 3_600_000, createdAt = now - 6000, meta = DownloadMeta()),
            Download(id = "8", name = "KuDownloader_0.2.6_android-arm64-v8a.apk", category = "programs", status = "completed", total = 45 * MB, done = 45 * MB, completedAt = now - 86_400_000, createdAt = now - 7000),
        )
    }

    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_6.copy(nightMode = if (dark) NightMode.NIGHT else NightMode.NOTNIGHT, softButtons = false),
        maxPercentDifference = 100.0,
    )

    private fun show(screen: Screen, downloads: List<Download> = samples) {
        Prefs.welcomed.value = true
        Ku.sample(downloads, settings = JsonObject(mapOf("theme" to JsonPrimitive(if (dark) "dark" else "light"), "accent" to JsonPrimitive("blue"))))
        Ku.speedHistory.value = List(Ku.HISTORY) { i -> (20 + (i % 7) * 3) * MB to (i % 5) * 200 * 1024L }
        UiState.stack.clear()
        UiState.stack.add(screen)
        paparazzi.snapshot { KuRoot() }
    }

    @Test fun downloads() = show(Screen.Downloads)

    @Test fun downloadsEmpty() = show(Screen.Downloads, emptyList())

    @Test fun video() = show(Screen.Video)

    @Test fun browserStart() = show(Screen.Browser)

    @Test fun more() = show(Screen.More)

    @Test fun settings() = show(Screen.Settings)

    @Test fun queues() = show(Screen.Queues)

    @Test fun airsend() {
        Ku.airStatus.value = AirStatus(running = true, alias = "OnePlus 8T", avatar = "cat", fingerprint = "a1", port = 53318, addresses = listOf("192.168.1.7"))
        Ku.airPeers.value = listOf(
            AirPeer(fingerprint = "b2", alias = "PC", avatar = "penguin", os = "windows", ip = "192.168.1.20", port = 53318, trusted = true),
            AirPeer(fingerprint = "c3", alias = "Living-room laptop", avatar = "fox", os = "linux", ip = "192.168.1.21", port = 53318),
        )
        Ku.airTransfers.value = listOf(
            AirTransfer(id = "t1", direction = "send", peer = "PC", peerFingerprint = "b2", peerAvatar = "penguin", state = "transferring", fileCount = 3, total = 800 * MB, done = 310 * MB, speed = 42 * MB),
            AirTransfer(id = "t2", direction = "receive", peer = "PC", peerFingerprint = "b2", peerAvatar = "penguin", state = "done", fileCount = 1, total = 12 * MB, done = 12 * MB, finished = now),
        )
        show(Screen.AirSend)
    }
}
