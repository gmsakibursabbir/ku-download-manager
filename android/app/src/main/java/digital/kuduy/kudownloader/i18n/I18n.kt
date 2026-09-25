package digital.kuduy.kudownloader.i18n

import android.content.Context
import android.content.res.Resources
import androidx.core.os.ConfigurationCompat
import digital.kuduy.kudownloader.core.Native
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Interface translations, shared with the desktop app: keys are the English
 * text itself (an untranslated string stays English). `assets/i18n/<code>.json`
 * is generated from `app/src/lib/locales` plus the phone-only strings in
 * `android/i18n` (see `android/scripts/locales.mjs`).
 */
object I18n {
    val LANGUAGES = listOf(
        "en" to "English",
        "zh" to "简体中文",
        "hi" to "हिन्दी",
        "es" to "Español",
        "ar" to "العربية",
        "fr" to "Français",
        "bn" to "বাংলা",
        "pt" to "Português",
        "ru" to "Русский",
        "ja" to "日本語",
        "de" to "Deutsch",
        "ko" to "한국어",
    )
    private val RTL = setOf("ar")

    @Volatile var lang = "en"
        private set
    @Volatile private var dict: Map<String, String> = emptyMap()
    private val teCache = object : LinkedHashMap<String, String>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, String>?) = size > 300
    }

    val isRtl get() = lang in RTL

    fun preference(ctx: Context): String = ctx.getSharedPreferences("ku", Context.MODE_PRIVATE).getString("lang", "system") ?: "system"

    fun resolve(pref: String): String {
        if (pref.isNotBlank() && pref != "system" && LANGUAGES.any { it.first == pref }) return pref
        val loc = ConfigurationCompat.getLocales(Resources.getSystem().configuration)[0]
        val code = loc?.language ?: "en"
        return LANGUAGES.firstOrNull { it.first == code }?.first ?: "en"
    }

    /** Load the chosen language ("system" follows the phone). */
    fun load(ctx: Context, pref: String = preference(ctx)) {
        ctx.getSharedPreferences("ku", Context.MODE_PRIVATE).edit().putString("lang", pref).apply()
        val code = resolve(pref)
        lang = code
        dict = if (code == "en") emptyMap() else runCatching {
            val text = ctx.assets.open("i18n/$code.json").bufferedReader().use { it.readText() }
            Json.parseToJsonElement(text).jsonObject.mapValues { it.value.jsonPrimitive.content }
        }.getOrElse { emptyMap() }
        synchronized(teCache) { teCache.clear() }
    }

    /** Hand the table to the engine so its messages can be translated too. */
    fun pushToEngine() {
        val table = buildJsonObject { put("strings", JsonObject(dict.mapValues { JsonPrimitive(it.value) })) }
        runCatching { Native.call("setStrings", table.toString()) }
    }

    fun t(s: String): String = dict[s] ?: s

    /** Translate a sentence and fill its {placeholders}. */
    fun tf(s: String, vararg vars: Pair<String, Any?>): String {
        var out = t(s)
        for ((k, v) in vars) out = out.replace("{$k}", v?.toString() ?: "")
        return out
    }

    /** A message from the download engine, translated when it is a known one. */
    fun te(msg: String): String {
        if (lang == "en" || msg.isBlank()) return msg
        synchronized(teCache) { teCache[msg] }?.let { return it }
        val r = runCatching { Native.te(msg) }.getOrDefault(msg)
        synchronized(teCache) { teCache[msg] = r }
        return r
    }
}

/** Shorthands used throughout the interface. */
fun t(s: String) = I18n.t(s)
fun tf(s: String, vararg vars: Pair<String, Any?>) = I18n.tf(s, *vars)
fun te(s: String) = I18n.te(s)
