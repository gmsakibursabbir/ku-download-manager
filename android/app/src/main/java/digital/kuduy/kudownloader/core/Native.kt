package digital.kuduy.kudownloader.core

/**
 * KuCore (Rust, `crates/ku-android`). Structured values travel as JSON:
 * [call] returns `{"ok": value}` or `{"error": "message"}`.
 */
object Native {
    init {
        System.loadLibrary("kudroid")
    }

    /** Starts the engine; returns "" or the reason it could not start. */
    @JvmStatic external fun init(config: String): String

    @JvmStatic external fun call(method: String, args: String): String

    /** Waits up to [timeoutMs] for engine events; a JSON array. */
    @JvmStatic external fun nextEvents(timeoutMs: Long): String

    /** An engine message in the interface language. */
    @JvmStatic external fun te(msg: String): String

    @JvmStatic external fun shouldBlock(url: String, source: String, kind: String): Boolean

    @JvmStatic external fun cosmetic(url: String): String

    @JvmStatic external fun hidden(request: String): String
}
