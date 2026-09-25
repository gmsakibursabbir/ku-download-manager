package digital.kuduy.kudownloader.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

// Mirrors of the engine's types (crates/ku-proto/src/model.rs,
// crates/kucore/src/types.rs, crates/kucore/src/airsend.rs). Unknown fields
// are ignored, so the engine can grow without breaking the app.

@Serializable
data class FileEntry(
    val path: String = "",
    val length: Long = 0,
    val completed: Long = 0,
    val selected: Boolean = true,
)

@Serializable
data class DownloadMeta(
    val mime: String? = null,
    val resumable: Boolean? = null,
    val thumbnail: String? = null,
    val mediaTitle: String? = null,
    val uploader: String? = null,
    val duration: Double? = null,
    val infoHash: String? = null,
    val numSeeders: Long? = null,
    val files: List<FileEntry> = emptyList(),
    val smartNote: String? = null,
    val verified: String? = null,
    val retries: Int = 0,
    val playlistIndex: String? = null,
)

@Serializable
data class Download(
    val id: String,
    val engine: String = "aria2",
    val kind: String = "http",
    val url: String = "",
    val mirrors: List<String> = emptyList(),
    val name: String = "",
    val dir: String = "",
    val filePath: String? = null,
    val status: String = "queued",
    val total: Long = 0,
    val done: Long = 0,
    val uploaded: Long = 0,
    val speed: Long = 0,
    val uploadSpeed: Long = 0,
    val connections: Int = 0,
    val activeConnections: Int = 0,
    val category: String = "",
    val queueId: String? = null,
    val position: Long = 0,
    val createdAt: Long = 0,
    val completedAt: Long? = null,
    val error: String? = null,
    val source: String = "",
    val options: JsonObject? = null,
    val meta: DownloadMeta = DownloadMeta(),
    /** Seconds left; only in live progress. */
    val eta: Long? = null,
) {
    val isRunning get() = status == "downloading" || status == "processing"
    val isFinished get() = status == "completed" || status == "seeding"
    val canResume get() = status == "paused" || status == "error" || status == "queued"
    val isTorrent get() = kind == "torrent" || kind == "magnet"
    val isMedia get() = kind == "media"
    val progress: Float get() = if (total > 0) (done.toFloat() / total).coerceIn(0f, 1f) else 0f
    val path: String get() = filePath ?: "${dir.trimEnd('/')}/$name"
    val etaSeconds: Long?
        get() = eta ?: if (speed > 0 && total > done) (total - done) / speed else null
}

@Serializable
data class ProgressItem(
    val id: String,
    val status: String,
    val done: Long,
    val total: Long,
    val speed: Long,
    val uploadSpeed: Long = 0,
    val activeConnections: Int = 0,
    val eta: Long? = null,
)

@Serializable
data class Stats(
    val downloadSpeed: Long = 0,
    val uploadSpeed: Long = 0,
    val active: Int = 0,
    val queued: Int = 0,
    val paused: Int = 0,
    val completed: Int = 0,
    val errors: Int = 0,
    val total: Int = 0,
)

@Serializable
data class Queue(
    val id: String = "",
    val name: String = "",
    val maxConcurrent: Int = 2,
    val position: Long = 0,
    val running: Boolean = false,
    val after: String = "none",
    val syncMinutes: Int = 0,
    val lastSync: Long = 0,
)

@Serializable
data class Schedule(
    val id: String = "",
    val name: String = "Night downloads",
    val enabled: Boolean = true,
    val queueId: String = "main",
    val start: String = "02:00",
    val stop: String? = null,
    val days: List<Int> = emptyList(),
    val date: String? = null,
    val profile: String? = null,
    val after: String = "none",
    val lastStart: String? = null,
    val lastStop: String? = null,
)

@Serializable
data class ProbeInfo(
    val url: String = "",
    val finalUrl: String = "",
    val filename: String? = null,
    val size: Long? = null,
    val mime: String? = null,
    val resumable: Boolean? = null,
    val status: Int? = null,
    val engine: String? = null,
    val kind: String? = null,
    val category: String? = null,
    val error: String? = null,
)

@Serializable
data class VideoQuality(
    val height: Int = 0,
    val label: String = "",
    val fps: Double? = null,
    val ext: String = "",
    val vcodec: String = "",
    val size: Long? = null,
    val hdr: Boolean = false,
)

@Serializable
data class AudioQuality(
    val bitrate: Int = 0,
    val ext: String = "",
    val acodec: String = "",
    val size: Long? = null,
)

@Serializable
data class PlaylistEntry(
    val index: Int = 0,
    val id: String = "",
    val title: String = "",
    val url: String = "",
    val duration: Double? = null,
)

@Serializable
data class MediaInfo(
    val url: String = "",
    val webpageUrl: String = "",
    val extractor: String = "",
    val title: String = "",
    val uploader: String? = null,
    val duration: Double? = null,
    val viewCount: Long? = null,
    val uploadDate: String? = null,
    val thumbnail: String? = null,
    val isLive: Boolean = false,
    val isPlaylist: Boolean = false,
    val playlistCount: Int? = null,
    val entries: List<PlaylistEntry> = emptyList(),
    val video: List<VideoQuality> = emptyList(),
    val audio: List<AudioQuality> = emptyList(),
    val subtitles: List<String> = emptyList(),
    val autoSubtitles: List<String> = emptyList(),
    val ffmpegAvailable: Boolean = false,
)

@Serializable
data class GrabLink(
    val url: String = "",
    val text: String? = null,
    val kind: String? = null,
)

@Serializable
data class BrowserCookie(
    val name: String = "",
    val value: String = "",
    val domain: String = "",
    val path: String = "/",
    val secure: Boolean = false,
    val httpOnly: Boolean = false,
    val hostOnly: Boolean = true,
    val expirationDate: Double? = null,
)

@Serializable
data class LogLine(val ts: Long = 0, val level: String = "info", val message: String = "")

@Serializable
data class TorrentFile(val index: Int = 0, val path: String = "", val length: Long = 0)

@Serializable
data class TorrentInfo(
    val name: String = "",
    val infoHash: String = "",
    val total: Long = 0,
    val private: Boolean = false,
    val comment: String? = null,
    val trackers: List<String> = emptyList(),
    val files: List<TorrentFile> = emptyList(),
)

// ───────── KuAirSend ─────────

@Serializable
data class AirPeer(
    val fingerprint: String = "",
    val alias: String = "",
    val avatar: String = "",
    val os: String = "",
    val version: String = "",
    val ip: String = "",
    val port: Int = 0,
    val trusted: Boolean = false,
)

@Serializable
data class AirFile(val name: String = "", val size: Long = 0, val done: Boolean = false)

@Serializable
data class AirTransfer(
    val id: String = "",
    /** "send" or "receive". */
    val direction: String = "",
    val peer: String = "",
    val peerFingerprint: String = "",
    val peerAvatar: String = "",
    /** waiting / transferring / done / declined / pin / failed / cancelled. */
    val state: String = "",
    val files: List<AirFile> = emptyList(),
    val fileCount: Int = 0,
    val total: Long = 0,
    val done: Long = 0,
    val speed: Long = 0,
    val started: Long = 0,
    val finished: Long? = null,
    val error: String? = null,
    val folder: String? = null,
    val text: String? = null,
    /** A link handed to the other device to download itself. */
    val download: RemoteDownload? = null,
) {
    val isOpen get() = state == "waiting" || state == "transferring"
}

@Serializable
data class AirRequest(
    val id: String = "",
    val peer: String = "",
    val peerFingerprint: String = "",
    val peerAvatar: String = "",
    val peerOs: String = "",
    val files: List<AirFile> = emptyList(),
    val fileCount: Int = 0,
    val total: Long = 0,
)

/** "Download this": a link one device asks another to download, now or at [at]. */
@Serializable
data class RemoteDownload(
    val url: String = "",
    val filename: String? = null,
    /** Start time, ms since the epoch; null = now. */
    val at: Long? = null,
    val referer: String? = null,
    val cookies: List<BrowserCookie> = emptyList(),
)

/** A nearby device now trusts this one: trust it back? */
@Serializable
data class AirTrustRequest(
    val peer: String = "",
    val peerFingerprint: String = "",
    val peerAvatar: String = "",
    val peerOs: String = "",
)

@Serializable
data class AirDownloadRequest(
    val id: String = "",
    val peer: String = "",
    val peerFingerprint: String = "",
    val peerAvatar: String = "",
    val peerOs: String = "",
    val download: RemoteDownload = RemoteDownload(),
)

@Serializable
data class AirMessage(
    val id: String = "",
    val peer: String = "",
    val peerFingerprint: String = "",
    val peerAvatar: String = "",
    val text: String = "",
)

@Serializable
data class AirStatus(
    val running: Boolean = false,
    val alias: String = "",
    val avatar: String = "",
    val fingerprint: String = "",
    val port: Int = 0,
    val addresses: List<String> = emptyList(),
    val folder: String = "",
    val discoveryError: String? = null,
)

class KuException(message: String) : Exception(message)
