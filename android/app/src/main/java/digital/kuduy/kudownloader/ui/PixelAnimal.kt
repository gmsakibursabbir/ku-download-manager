package digital.kuduy.kudownloader.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import digital.kuduy.kudownloader.i18n.t

val AVATARS = SPRITES.keys.toList()

val ANIMAL_NAMES = mapOf(
    "cat" to "Cat", "fox" to "Fox", "frog" to "Frog", "panda" to "Panda", "bunny" to "Bunny", "penguin" to "Penguin",
    "pig" to "Pig", "chick" to "Chick", "dog" to "Dog", "bear" to "Bear", "koala" to "Koala", "owl" to "Owl",
    "monkey" to "Monkey", "tiger" to "Tiger", "mouse" to "Mouse", "cow" to "Cow",
)

fun animalName(a: String) = t(ANIMAL_NAMES[a] ?: a)

/** Same pick as the desktop (FNV-1a), so a device keeps its animal everywhere. */
fun hashPick(fingerprint: String, n: Int): Int {
    var h = 2166136261L.toInt()
    for (c in fingerprint) h = (h xor c.code) * 16777619
    return ((h.toLong() and 0xFFFFFFFFL) % n).toInt()
}

fun avatarOf(avatar: String, fingerprint: String) = avatar.takeIf { it in SPRITES } ?: AVATARS[hashPick(fingerprint, AVATARS.size)]

/** A round avatar with an 8-bit animal that bobs and blinks. */
@Composable
fun PixelAnimal(animal: String, size: Dp = 64.dp, seed: Int = 0, still: Boolean = false) {
    val s = SPRITES[animal] ?: SPRITES.getValue("cat")
    val dark = LocalKuColors.current.dark
    val phase = (seed % 17) * 230
    val anim = rememberInfiniteTransition(label = "px")
    val bob by anim.animateFloat(
        0f, 1f,
        infiniteRepeatable(tween(1400, delayMillis = 0, easing = LinearEasing), RepeatMode.Reverse, initialStartOffset = androidx.compose.animation.core.StartOffset(phase)),
        label = "bob",
    )
    val blink by anim.animateFloat(
        0f, 0f,
        infiniteRepeatable(
            keyframes {
                durationMillis = 4200
                0f at 0
                0f at 3900
                1f at 3950
                1f at 4100
                0f at 4150
            },
            initialStartOffset = androidx.compose.animation.core.StartOffset(phase * 3),
        ),
        label = "blink",
    )
    Box(Modifier.size(size).clip(CircleShape).background(if (dark) s.bgDark else s.bgLight)) {
        Canvas(Modifier.size(size)) {
            val rows = s.half.size
            // 12 wide, `rows` tall, inside a 15.5 × 15 box like the desktop's viewBox.
            val unit = this.size.width / 15.5f
            val left = 1.75f * unit
            val top = ((15 - rows) / 2f) * unit + if (still) 0f else (bob - 0.5f) * unit * 0.5f
            val eyesShut = !still && blink > 0.5f
            s.half.forEachIndexed { y, row ->
                val full = row + row.reversed()
                full.forEachIndexed { x, ch ->
                    if (ch == '.') return@forEachIndexed
                    val c = when {
                        ch == 'k' && eyesShut -> s.pal[s.lid]
                        else -> s.pal[ch]
                    } ?: Color.Magenta
                    drawRect(c, Offset(left + x * unit, top + y * unit), Size(unit * 1.02f, unit * 1.02f))
                }
            }
        }
    }
}

/** The device's system as a small pixel badge. */
@Composable
fun OsBadge(os: String, size: Dp = 20.dp) {
    val o = OS_SPRITES[os] ?: return
    val fg = MaterialTheme.colorScheme.onSurface
    Box(Modifier.size(size).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceContainerHighest)) {
        Canvas(Modifier.size(size)) {
            val w = o.rows.maxOf { it.length }
            val h = o.rows.size
            val unit = this.size.width / 11f
            val left = (11 - w) / 2f * unit
            val top = (11 - h) / 2f * unit
            o.rows.forEachIndexed { y, row ->
                row.forEachIndexed { x, ch ->
                    if (ch == '.') return@forEachIndexed
                    val c = if (o.pal.containsKey(ch)) o.pal[ch] ?: fg else return@forEachIndexed
                    drawRect(c, Offset(left + x * unit, top + y * unit), Size(unit * 1.02f, unit * 1.02f))
                }
            }
        }
    }
}

fun osName(os: String) = OS_SPRITES[os]?.name ?: os.replaceFirstChar { it.uppercase() }
