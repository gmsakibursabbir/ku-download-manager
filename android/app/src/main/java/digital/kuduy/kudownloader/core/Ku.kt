package digital.kuduy.kudownloader.core

import android.content.Context
import android.os.Build
import android.os.Environment
import android.provider.Settings.Global
import android.util.Log
import digital.kuduy.kudownloader.i18n.I18n
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import java.io.File

/**
 * The app's view of KuCore: live state as flows, and suspend functions for
 * every engine request (run off the main thread).
 */
object Ku {
    private const val TAG = "Ku"
    val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        coerceInputValues = true
        encodeDefaults = true
    }
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    enum class Phase { Starting, Ready, Failed }

    val phase = MutableStateFlow(Phase.Starting)
    val startError = MutableStateFlow<String?>(null)
    var tools: Tools.Found? = null
        private set
    /** The video tools as last checked (for Settings › Advanced and the Video screen). */
    val toolsState = MutableStateFlow<Tools.Found?>(null)
    /** yt-dlp download progress while repairing: (done, total). */
    val toolsRepair = MutableStateFlow<Pair<Long, Long>?>(null)

    private val _downloads = MutableStateFlow<Map<String, Download>>(emptyMap())
    val downloads: StateFlow<Map<String, Download>> = _downloads
    val stats = MutableStateFlow(Stats())
    val queues = MutableStateFlow<List<Queue>>(emptyList())
    val schedules = MutableStateFlow<List<Schedule>>(emptyList())
    val settings = MutableStateFlow(JsonObject(emptyMap()))

    /** Download and upload speed, one sample a second (the network graph). */
    const val HISTORY = 60
    val speedHistory = MutableStateFlow(List(HISTORY) { 0L to 0L })

    val airStatus = MutableStateFlow(AirStatus())
    val airPeers = MutableStateFlow<List<AirPeer>>(emptyList())
    val airTransfers = MutableStateFlow<List<AirTransfer>>(emptyList())

    /** Everything else the engine says (notices, prompts, completions). */
    private val _events = MutableSharedFlow<JsonObject>(extraBufferCapacity = 256)
    val events: SharedFlow<JsonObject> = _events

    /** Tool downloads and similar background progress, by tool name. */
    val toolProgress = MutableStateFlow<Map<String, Pair<Long, Long>>>(emptyMap())

    @Volatile private var started = false

    fun downloadDir(): File = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "KuDownloader")

    /** Starts KuCore once per process (tools first: yt-dlp needs their paths). */
    fun start(ctx: Context) {
        if (started) return
        started = true
        val app = ctx.applicationContext
        Thread({
            try {
                val t = Tools.prepare(app)
                tools = t
                toolsState.value = t
                val data = File(app.filesDir, "kucore").apply { mkdirs() }
                val env = t.env + mapOf("KU_DATA_DIR" to data.absolutePath, "KU_DOWNLOAD_DIR" to downloadDir().absolutePath)
                val config = buildJsonObject {
                    put("env", JsonObject(env.mapValues { JsonPrimitive(it.value) }))
                    put("tools", buildJsonObject {
                        t.ytdlp?.let { put("ytdlp", it) }
                        t.ffmpeg?.let { put("ffmpeg", it) }
                        t.aria2?.let { put("aria2", it) }
                    })
                    put("deviceName", deviceName(app))
                    put("threads", Runtime.getRuntime().availableProcessors().coerceIn(2, 6))
                }
                val err = Native.init(config.toString())
                if (err.isNotEmpty()) throw KuException(err)
                I18n.pushToEngine()
                reloadAllBlocking()
                phase.value = Phase.Ready
                // yt-dlp missing (first start without it, or a failed unpack): fetch it now.
                scope.launch {
                    if (!t.videoReady) runCatching { repairTools(app) }
                    // Sites change weekly; the bundled yt-dlp ages with the app.
                    runCatching { updateYtdlp(app, force = false) }
                    // New KuDownloader release: tell the user once per version.
                    AppUpdate.check()
                    AppUpdate.available.value?.takeIf { it.version != Prefs.appUpdateNotified.value }?.let {
                        Prefs.appUpdateNotified.value = it.version
                        digital.kuduy.kudownloader.service.Notifier.appUpdate(app, it.version)
                    }
                }
                pump()
            } catch (e: Throwable) {
                Log.e(TAG, "start", e)
                startError.value = e.message ?: e.toString()
                phase.value = Phase.Failed
            }
        }, "ku-start").start()
    }

    private fun deviceName(ctx: Context): String =
        (if (Build.VERSION.SDK_INT >= 25) runCatching { Global.getString(ctx.contentResolver, Global.DEVICE_NAME) }.getOrNull() else null)?.takeIf { it.isNotBlank() }
            ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"

    /** Unpack the tools again, download yt-dlp if needed, and point KuCore at them. */
    suspend fun repairTools(ctx: Context): Tools.Found = withContext(Dispatchers.IO) {
        toolsRepair.value = 0L to 0L
        try {
            val t = Tools.repair(ctx.applicationContext) { done, total -> toolsRepair.value = done to total }
            tools = t
            toolsState.value = t
            call("setEnv", args("env" to t.env))
            saveSettings(buildMap {
                t.ytdlp?.let { put("ytdlpPath", it) }
                t.ffmpeg?.let { put("ffmpegPath", it) }
                t.aria2?.let { put("aria2Path", it) }
            })
            t
        } finally {
            toolsRepair.value = null
        }
    }

    /**
     * Install the newest yt-dlp when it differs from ours. Automatic checks
     * run at most daily (and wait for Wi-Fi when "Wi-Fi only" is on).
     * Returns the installed version, or null when nothing changed.
     */
    suspend fun updateYtdlp(ctx: Context, force: Boolean): String? = withContext(Dispatchers.IO) {
        val now = System.currentTimeMillis()
        if (!force) {
            if (now - Prefs.ytdlpChecked.value < 24 * 3600_000L) return@withContext null
            val cm = ctx.getSystemService(android.net.ConnectivityManager::class.java)
            if (Prefs.wifiOnly.value && cm?.isActiveNetworkMetered == true) return@withContext null
        }
        val latest = Tools.latestYtdlp() ?: if (force) throw KuException(I18n.t("Could not reach GitHub. Check the connection and try again.")) else return@withContext null
        Prefs.ytdlpChecked.value = now
        if (latest == Prefs.ytdlpVersion.value && tools?.ytdlp != null) return@withContext null
        toolsRepair.value = 0L to 0L
        try {
            Tools.installLatestYtdlp(ctx.applicationContext) { done, total -> toolsRepair.value = done to total }
        } finally {
            toolsRepair.value = null
        }
        Prefs.ytdlpVersion.value = latest
        val t = Tools.prepare(ctx.applicationContext)
        tools = t
        toolsState.value = t
        t.ytdlp?.let { saveSettings(mapOf("ytdlpPath" to it)) }
        latest
    }

    /** Screenshot tests: show the screens with sample data (no engine). */
    fun sample(downloads: List<Download>, queues: List<Queue> = listOf(Queue(id = "main", name = "Main queue")), settings: JsonObject = JsonObject(emptyMap())) {
        _downloads.value = downloads.associateBy { it.id }
        this.queues.value = queues
        this.settings.value = settings
        refreshStats()
        phase.value = Phase.Ready
    }

    // ───────── requests ─────────

    fun callBlocking(method: String, args: JsonObject = JsonObject(emptyMap())): JsonElement {
        val out = json.parseToJsonElement(Native.call(method, args.toString())).jsonObject
        out["error"]?.let { throw KuException(I18n.te(it.jsonPrimitive.content)) }
        return out["ok"] ?: JsonNull
    }

    suspend fun call(method: String, args: JsonObject = JsonObject(emptyMap())): JsonElement =
        withContext(Dispatchers.IO) { callBlocking(method, args) }

    suspend inline fun <reified T> get(method: String, args: JsonObject = JsonObject(emptyMap())): T =
        json.decodeFromJsonElement(call(method, args))

    fun args(vararg pairs: Pair<String, Any?>): JsonObject = JsonObject(pairs.associate { (k, v) -> k to toJson(v) })

    fun toJson(v: Any?): JsonElement = when (v) {
        null -> JsonNull
        is JsonElement -> v
        is String -> JsonPrimitive(v)
        is Number -> JsonPrimitive(v)
        is Boolean -> JsonPrimitive(v)
        is Collection<*> -> JsonArray(v.map { toJson(it) })
        is Map<*, *> -> JsonObject(v.entries.associate { it.key.toString() to toJson(it.value) })
        else -> throw IllegalArgumentException("Unsupported value: ${v::class}")
    }

    private fun reloadAllBlocking() {
        val list: List<Download> = json.decodeFromJsonElement(callBlocking("listDownloads"))
        _downloads.value = list.associateBy { it.id }
        queues.value = json.decodeFromJsonElement(callBlocking("listQueues"))
        schedules.value = json.decodeFromJsonElement(callBlocking("listSchedules"))
        settings.value = callBlocking("getSettings").jsonObject
        stats.value = json.decodeFromJsonElement(callBlocking("stats"))
        runCatching {
            airStatus.value = json.decodeFromJsonElement(callBlocking("airStatus"))
            airPeers.value = json.decodeFromJsonElement(callBlocking("airPeers"))
            airTransfers.value = json.decodeFromJsonElement(callBlocking("airTransfers"))
        }
    }

    // ───────── events ─────────

    private fun pump() {
        Thread({
            var quiet = 0
            while (true) {
                val batch = runCatching { json.parseToJsonElement(Native.nextEvents(1000)).jsonArray }.getOrNull() ?: JsonArray(emptyList())
                var progressSeen = false
                for (e in batch) {
                    val o = e as? JsonObject ?: continue
                    runCatching { if (apply(o)) progressSeen = true }.onFailure { Log.w(TAG, "event ${o["type"]}", it) }
                }
                // Keep the graph moving (flat) while nothing downloads.
                if (progressSeen) quiet = 0 else if (++quiet >= 1) {
                    pushSpeed(0, 0)
                    quiet = 0
                }
            }
        }, "ku-events").apply { isDaemon = true }.start()
    }

    private fun str(o: JsonObject, k: String) = o[k]?.jsonPrimitive?.contentOrNull

    /** Applies one event; true when it was a progress tick. */
    private fun apply(o: JsonObject): Boolean {
        when (str(o, "type")) {
            "progress" -> {
                val items: List<ProgressItem> = json.decodeFromJsonElement(o["items"] ?: JsonArray(emptyList()))
                _downloads.update { cur ->
                    val next = cur.toMutableMap()
                    for (p in items) {
                        val d = next[p.id] ?: continue
                        next[p.id] = d.copy(status = p.status, done = p.done, total = p.total, speed = p.speed, uploadSpeed = p.uploadSpeed, activeConnections = p.activeConnections, eta = p.eta)
                    }
                    next
                }
                val down = o["downloadSpeed"]?.jsonPrimitive?.longOrNull ?: 0
                val up = o["uploadSpeed"]?.jsonPrimitive?.longOrNull ?: 0
                pushSpeed(down, up)
                refreshStats()
                return true
            }
            "upsert" -> {
                val d: Download = json.decodeFromJsonElement(o["download"]!!)
                _downloads.update { it + (d.id to d) }
                refreshStats()
            }
            "removed" -> {
                val ids = o["ids"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
                _downloads.update { it - ids }
                refreshStats()
            }
            "resync" -> reloadAllBlocking()
            "settingsChanged" -> settings.value = callBlocking("getSettings").jsonObject
            "queuesChanged" -> queues.value = json.decodeFromJsonElement(callBlocking("listQueues"))
            "schedulesChanged" -> schedules.value = json.decodeFromJsonElement(callBlocking("listSchedules"))
            "airSendPeers" -> airPeers.value = json.decodeFromJsonElement(o["peers"]!!)
            "airSendTransfer" -> {
                val t: AirTransfer = json.decodeFromJsonElement(o["transfer"]!!)
                airTransfers.update { list -> if (list.any { it.id == t.id }) list.map { if (it.id == t.id) t else it } else listOf(t) + list }
                _events.tryEmit(o)
            }
            "toolProgress" -> {
                val tool = str(o, "tool") ?: ""
                toolProgress.update { it + (tool to ((o["done"]?.jsonPrimitive?.longOrNull ?: 0) to (o["total"]?.jsonPrimitive?.longOrNull ?: 0))) }
            }
            "toolDone" -> {
                toolProgress.update { it - (str(o, "tool") ?: "") }
                _events.tryEmit(o)
            }
            else -> _events.tryEmit(o)
        }
        return false
    }

    private fun pushSpeed(down: Long, up: Long) {
        speedHistory.update { (it + (down to up)).takeLast(HISTORY) }
    }

    private fun refreshStats() {
        val all = _downloads.value.values
        stats.value = Stats(
            downloadSpeed = all.sumOf { if (it.isRunning) it.speed else 0 },
            uploadSpeed = all.sumOf { it.uploadSpeed },
            active = all.count { it.isRunning },
            queued = all.count { it.status == "queued" },
            paused = all.count { it.status == "paused" },
            completed = all.count { it.isFinished },
            errors = all.count { it.status == "error" },
            total = all.size,
        )
    }

    // ───────── typed helpers ─────────

    fun setting(key: String): JsonElement? = settings.value[key]
    fun settingString(key: String, def: String = "") = (settings.value[key] as? JsonPrimitive)?.contentOrNull ?: def
    fun settingBool(key: String, def: Boolean = false) = (settings.value[key] as? JsonPrimitive)?.booleanOrNull ?: def
    fun settingLong(key: String, def: Long = 0) = (settings.value[key] as? JsonPrimitive)?.longOrNull ?: def

    /** Merge [patch] into the settings and save (the engine validates). */
    suspend fun saveSettings(patch: Map<String, Any?>): JsonObject {
        val merged = JsonObject(settings.value + patch.mapValues { toJson(it.value) })
        val saved = call("saveSettings", args("settings" to merged)).jsonObject
        settings.value = saved
        return saved
    }

    /** Waiting for a queue that is not running (only starting the queue, or "Start now", runs it). */
    fun heldByQueue(d: Download): Boolean =
        d.status == "queued" && d.queueId != null && queues.value.none { it.id == d.queueId && it.running }

    /** Ready to run or running: work that keeps the background service alive. */
    fun active(d: Download): Boolean = d.isRunning || (d.status == "queued" && !heldByQueue(d))

    /** Start now, outside any queue. */
    suspend fun startNow(ids: Collection<String>) {
        call("moveToQueue", args("ids" to ids.toList(), "queueId" to null))
        resume(ids)
    }

    /** Start every paused download and every queue that has downloads waiting. */
    suspend fun startAll() {
        resumeAll()
        val waiting = downloads.value.values.filter { heldByQueue(it) }.mapNotNull { it.queueId }.toSet()
        waiting.forEach { runCatching { startQueue(it) } }
    }

    suspend fun pause(ids: Collection<String>) = call("pause", args("ids" to ids.toList()))
    suspend fun resume(ids: Collection<String>) = call("resume", args("ids" to ids.toList()))
    suspend fun redownload(ids: Collection<String>) = call("redownload", args("ids" to ids.toList()))
    suspend fun remove(ids: Collection<String>, deleteFiles: Boolean) = call("remove", args("ids" to ids.toList(), "deleteFiles" to deleteFiles))
    suspend fun pauseAll() = call("pauseAll")
    suspend fun resumeAll() = call("resumeAll")
    suspend fun clearFinished() = call("clearFinished")
    suspend fun edit(id: String, patch: Map<String, Any?>): Download = get("editDownload", args("id" to id, "patch" to patch))
    suspend fun moveToQueue(ids: Collection<String>, queueId: String?) = call("moveToQueue", args("ids" to ids.toList(), "queueId" to queueId))
    suspend fun reorder(id: String, direction: String) = call("reorder", args("id" to id, "direction" to direction))
    suspend fun probe(url: String, options: JsonObject? = null): ProbeInfo = get("probeUrl", args("url" to url, "options" to options))
    suspend fun add(req: JsonObject): Download = get("addDownload", args("req" to req))
    suspend fun addBatch(urls: List<String>, template: JsonObject): JsonObject = call("addBatch", args("urls" to urls, "template" to template)).jsonObject
    suspend fun analyze(url: String, playlist: Boolean, cookies: List<BrowserCookie> = emptyList(), referer: String? = null): MediaInfo =
        get("mediaAnalyze", args("url" to url, "playlist" to playlist, "cookies" to json.encodeToJsonElement(cookies), "referer" to referer))
    suspend fun addMedia(req: JsonObject): Download = get("mediaDownload", args("req" to req))
    suspend fun grab(url: String): List<GrabLink> = get("grabPage", args("url" to url))
    suspend fun extractLinks(html: String, base: String): List<GrabLink> = get("extractLinks", args("html" to html, "base" to base))
    suspend fun details(id: String): JsonObject = call("getDetails", args("id" to id)).jsonObject
    suspend fun verify(id: String, algo: String): String = call("verifyHash", args("id" to id, "algo" to algo)).jsonPrimitive.content
    suspend fun checkDuplicate(url: String, dir: String?, filename: String?): JsonObject =
        call("checkDuplicate", args("url" to url, "dir" to dir, "filename" to filename)).jsonObject
    suspend fun torrentInfo(base64: String): Pair<TorrentInfo, String> {
        val r = call("torrentInfo", args("data" to base64)).jsonObject
        return json.decodeFromJsonElement<TorrentInfo>(r["info"]!!) to r["data"]!!.jsonPrimitive.content
    }

    suspend fun saveQueue(q: Queue): Queue = get("saveQueue", args("queue" to json.encodeToJsonElement(q)))
    suspend fun deleteQueue(id: String) = call("deleteQueue", args("id" to id))
    suspend fun startQueue(id: String) = call("startQueue", args("id" to id))
    suspend fun stopQueue(id: String) = call("stopQueue", args("id" to id))
    suspend fun saveSchedule(s: Schedule): Schedule = get("saveSchedule", args("schedule" to json.encodeToJsonElement(s)))
    suspend fun deleteSchedule(id: String) = call("deleteSchedule", args("id" to id))
    suspend fun setProfile(id: String) = call("setProfile", args("id" to id))

    // KuAirSend
    suspend fun airSetEnabled(on: Boolean): AirStatus = get<AirStatus>("airSetEnabled", args("enabled" to on)).also { airStatus.value = it }
    suspend fun airRefreshStatus() { airStatus.value = get("airStatus") }
    suspend fun airSend(fingerprint: String, paths: List<String>, text: String?, pin: String?): String =
        call("airSend", args("fingerprint" to fingerprint, "paths" to paths, "text" to text, "pin" to pin)).jsonPrimitive.content
    /** Ask a nearby device to download a link itself, now or at [RemoteDownload.at]. */
    suspend fun airSendDownload(fingerprint: String, download: RemoteDownload, pin: String? = null): String =
        call("airSendDownload", args("fingerprint" to fingerprint, "download" to json.encodeToJsonElement(download), "pin" to pin)).jsonPrimitive.content
    suspend fun airCancel(id: String) = call("airCancel", args("id" to id))
    suspend fun airDecide(id: String, accept: Boolean, trust: Boolean) = call("airDecide", args("id" to id, "accept" to accept, "trust" to trust))
    suspend fun airRefresh() = call("airRefresh")
    suspend fun airAdd(address: String): AirPeer = get("airAdd", args("address" to address))
    suspend fun airTrust(fingerprint: String, trusted: Boolean) = call("airTrust", args("fingerprint" to fingerprint, "trusted" to trusted))
    suspend fun airClearHistory() {
        call("airClearHistory")
        airTransfers.value = get("airTransfers")
    }

    fun launch(block: suspend CoroutineScope.() -> Unit) = scope.launch(block = block)
}
