package digital.kuduy.kudownloader.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import digital.kuduy.kudownloader.core.Ku
import digital.kuduy.kudownloader.core.Prefs
import digital.kuduy.kudownloader.i18n.I18n
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/** The desktop's accent colours (Settings › Appearance), same names. */
val ACCENTS = listOf(
    "blue" to Color(0xFF2563EB),
    "violet" to Color(0xFF7C3AED),
    "teal" to Color(0xFF0D9488),
    "green" to Color(0xFF16A34A),
    "orange" to Color(0xFFEA580C),
    "pink" to Color(0xFFDB2777),
    "red" to Color(0xFFDC2626),
    "graphite" to Color(0xFF52525B),
)

val Danger = Color(0xFFD13438)
val UploadColor = Color(0xFF16A34A)

data class KuColors(val success: Color, val warning: Color, val danger: Color, val upload: Color, val dark: Boolean)

val LocalKuColors = staticCompositionLocalOf { KuColors(Color(0xFF16A34A), Color(0xFFD97706), Danger, UploadColor, false) }

private fun tone(c: Color, towards: Color, amount: Float) = lerp(c, towards, amount)

private fun lightScheme(accent: Color): ColorScheme = lightColorScheme(
    primary = accent,
    onPrimary = Color.White,
    primaryContainer = tone(accent, Color.White, 0.84f),
    onPrimaryContainer = tone(accent, Color.Black, 0.55f),
    secondary = tone(accent, Color(0xFF64748B), 0.55f),
    onSecondary = Color.White,
    secondaryContainer = tone(accent, Color.White, 0.9f),
    onSecondaryContainer = tone(accent, Color.Black, 0.6f),
    tertiary = Color(0xFF16A34A),
    background = Color(0xFFF7F8FC),
    onBackground = Color(0xFF15171C),
    surface = Color(0xFFF7F8FC),
    onSurface = Color(0xFF15171C),
    surfaceVariant = Color(0xFFE8EBF2),
    onSurfaceVariant = Color(0xFF5B6170),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF2F4F9),
    surfaceContainer = Color(0xFFEEF0F6),
    surfaceContainerHigh = Color(0xFFE8EBF2),
    surfaceContainerHighest = Color(0xFFE2E5ED),
    outline = Color(0xFFC3C8D4),
    outlineVariant = Color(0xFFDDE1EA),
    error = Danger,
)

private fun darkScheme(accent: Color, palette: String): ColorScheme {
    val (bg, s1, s2, s3, s4) = when (palette) {
        "black" -> listOf(Color.Black, Color(0xFF0A0A0B), Color(0xFF121214), Color(0xFF1A1A1D), Color(0xFF232327))
        "midnight" -> listOf(Color(0xFF0B1020), Color(0xFF10172A), Color(0xFF151D33), Color(0xFF1B243D), Color(0xFF222C47))
        else -> listOf(Color(0xFF111318), Color(0xFF16181E), Color(0xFF1B1E25), Color(0xFF22252D), Color(0xFF2A2D36))
    }
    val light = tone(accent, Color.White, 0.25f)
    return darkColorScheme(
        primary = light,
        onPrimary = Color(0xFF0B1020),
        primaryContainer = tone(accent, bg, 0.62f),
        onPrimaryContainer = tone(accent, Color.White, 0.8f),
        secondary = tone(accent, Color(0xFF94A3B8), 0.6f),
        onSecondary = Color.Black,
        secondaryContainer = tone(accent, bg, 0.75f),
        onSecondaryContainer = tone(accent, Color.White, 0.82f),
        tertiary = Color(0xFF4ADE80),
        background = bg,
        onBackground = Color(0xFFE7E9EE),
        surface = bg,
        onSurface = Color(0xFFE7E9EE),
        surfaceVariant = s3,
        onSurfaceVariant = Color(0xFFA7ADBA),
        surfaceContainerLowest = bg,
        surfaceContainerLow = s1,
        surfaceContainer = s2,
        surfaceContainerHigh = s3,
        surfaceContainerHighest = s4,
        outline = Color(0xFF454A56),
        outlineVariant = Color(0xFF30343D),
        error = Color(0xFFF1707B),
    )
}

@Composable
fun KuTheme(content: @Composable () -> Unit) {
    val settings by Ku.settings.collectAsStateWithLifecycle()
    val dynamic by Prefs.dynamicColor.state.collectAsStateWithLifecycle()
    val theme = (settings["theme"] as? JsonPrimitive)?.contentOrNull ?: "system"
    val accentName = (settings["accent"] as? JsonPrimitive)?.contentOrNull ?: "blue"
    val palette = (settings["darkPalette"] as? JsonPrimitive)?.contentOrNull ?: "default"
    val dark = when (theme) {
        "dark" -> true
        "light" -> false
        else -> isSystemInDarkTheme()
    }
    val accent = ACCENTS.firstOrNull { it.first == accentName }?.second ?: ACCENTS[0].second
    val ctx = LocalContext.current
    val scheme = when {
        dynamic && Build.VERSION.SDK_INT >= 31 -> if (dark) dynamicDarkColorScheme(ctx).let { if (palette == "black") it.copy(background = Color.Black, surface = Color.Black) else it } else dynamicLightColorScheme(ctx)
        dark -> darkScheme(accent, palette)
        else -> lightScheme(accent)
    }
    val ku = KuColors(
        success = if (dark) Color(0xFF4ADE80) else Color(0xFF16A34A),
        warning = if (dark) Color(0xFFFBBF24) else Color(0xFFD97706),
        danger = Danger,
        upload = if (dark) Color(0xFF4ADE80) else UploadColor,
        dark = dark,
    )
    CompositionLocalProvider(
        LocalKuColors provides ku,
        LocalLayoutDirection provides if (I18n.isRtl) LayoutDirection.Rtl else LayoutDirection.Ltr,
    ) {
        MaterialTheme(colorScheme = scheme, typography = Typography(), content = content)
    }
}
