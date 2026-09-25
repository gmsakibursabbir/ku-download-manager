package digital.kuduy.kudownloader.core

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Phone-only preferences (the engine's settings are shared with the desktop
 * and live in KuCore). Each one is observable.
 */
object Prefs {
    private lateinit var sp: SharedPreferences

    class Pref<T>(private val key: String, private val def: T) {
        private val flow = MutableStateFlow(def)
        val state: StateFlow<T> get() = flow
        var value: T
            get() = flow.value
            set(v) {
                flow.value = v
                save(key, v)
            }

        @Suppress("UNCHECKED_CAST")
        internal fun load() {
            flow.value = when (def) {
                is Boolean -> sp.getBoolean(key, def)
                is Int -> sp.getInt(key, def)
                is Long -> sp.getLong(key, def)
                is String -> sp.getString(key, def) ?: def
                else -> def
            } as T
        }
    }

    private fun save(key: String, v: Any?) {
        if (!::sp.isInitialized) return
        sp.edit().apply {
            when (v) {
                is Boolean -> putBoolean(key, v)
                is Int -> putInt(key, v)
                is Long -> putLong(key, v)
                is String -> putString(key, v)
            }
        }.apply()
    }

    /** Pause downloads on mobile data and resume them on Wi-Fi. */
    val wifiOnly = Pref("wifiOnly", false)
    /** Pause while the battery saver is on. */
    val pauseOnBatterySaver = Pref("pauseOnBatterySaver", false)
    /** Keep the phone awake at full Wi-Fi speed while downloading. */
    val highPerfWifi = Pref("highPerfWifi", true)
    /** Offer a copied link when the app opens. */
    val clipboardOffer = Pref("clipboardOffer", true)
    /** Built-in browser */
    val adblock = Pref("adblock", true)
    val pill = Pref("pill", true)
    val detectMedia = Pref("detectMedia", true)
    val interceptDownloads = Pref("interceptDownloads", true)
    val searchEngine = Pref("searchEngine", "duckduckgo")
    val homePage = Pref("homePage", "")
    val desktopMode = Pref("desktopMode", false)
    val blockPopups = Pref("blockPopups", true)
    val bookmarks = Pref("bookmarks", "[]")
    val history = Pref("history", "[]")
    val adsBlocked = Pref("adsBlocked", 0L)
    /** Appearance: material-you colours from the wallpaper (Android 12+). */
    val dynamicColor = Pref("dynamicColor", false)
    val welcomed = Pref("welcomed", false)
    val lastClipboard = Pref("lastClipboard", "")

    private val all = listOf(
        wifiOnly, pauseOnBatterySaver, highPerfWifi, clipboardOffer, adblock, pill, detectMedia, interceptDownloads,
        searchEngine, homePage, desktopMode, blockPopups, bookmarks, history, adsBlocked, dynamicColor, welcomed, lastClipboard,
    )

    fun init(ctx: Context) {
        if (::sp.isInitialized) return
        sp = ctx.getSharedPreferences("ku-prefs", Context.MODE_PRIVATE)
        all.forEach { it.load() }
    }
}
