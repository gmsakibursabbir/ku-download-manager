package digital.kuduy.kudownloader.ui

import androidx.compose.ui.graphics.Color

// Generated from app/src/app/airsend/PixelAnimal.tsx by android/scripts/sprites.cjs.
// Edit the desktop file and regenerate so both apps draw the same animals.

class Sprite(val half: List<String>, val pal: Map<Char, Color>, val lid: Char, val bgLight: Color, val bgDark: Color)

/** Pixel badge: `null` colour = the text colour. */
class OsSprite(val rows: List<String>, val pal: Map<Char, Color?>, val name: String)

val SPRITES: Map<String, Sprite> = linkedMapOf(
    "cat" to Sprite(
        listOf("o.....", "oo....", "obo...", "oaoooo", "oaaaaa", "oakaaa", "oakaaa", "obaaan", "oaaaaa", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF3B2314), 'a' to Color(0xFFF39C38), 'b' to Color(0xFFFFD9A8), 'k' to Color(0xFF1B1B1B), 'n' to Color(0xFFFF8FAB)),
        'a',
        Color(0xFFFFF0DC),
        Color(0xFF3A2A1A),
    ),
    "fox" to Sprite(
        listOf("o.....", "oo....", "obo...", "obbo..", "oaaooo", "oaaaaa", "oakaaa", "owaaaa", "owwaaa", ".owwwd", "..owww", "...ooo"),
        mapOf('o' to Color(0xFF3A1F10), 'a' to Color(0xFFF06A24), 'b' to Color(0xFFFFC9A0), 'w' to Color(0xFFFFF4E6), 'd' to Color(0xFF1B1B1B), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFFFE6D8),
        Color(0xFF3D2418),
    ),
    "frog" to Sprite(
        listOf(".ooo..", "owwwo.", "owkwoo", "owwwoa", "oaaaaa", "oaaaaa", "onaaaa", "oaoooo", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF1D3B17), 'a' to Color(0xFF5CC15A), 'w' to Color(0xFFFFFFFF), 'k' to Color(0xFF1B1B1B), 'n' to Color(0xFFFF9FB5)),
        'w',
        Color(0xFFE2F6DD),
        Color(0xFF1C3320),
    ),
    "panda" to Sprite(
        listOf(".oo...", "oddo..", "oddooo", "owwwww", "owddww", "owdkww", "owddww", "owwwwd", ".owwww", "..oooo"),
        mapOf('o' to Color(0xFF1B1B1B), 'w' to Color(0xFFFFFFFF), 'd' to Color(0xFF2E2E33), 'k' to Color(0xFFFFFFFF)),
        'd',
        Color(0xFFE8EBF2),
        Color(0xFF2A2D36),
    ),
    "bunny" to Sprite(
        listOf("..oo..", ".obo..", ".obo..", ".obo..", ".oaooo", "oaaaaa", "oakaaa", "obaaan", "oaaaaa", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF4A4052), 'a' to Color(0xFFF6F2F8), 'b' to Color(0xFFFFB3C6), 'k' to Color(0xFF1B1B1B), 'n' to Color(0xFFFF7FA0)),
        'a',
        Color(0xFFFBE9F2),
        Color(0xFF3A2733),
    ),
    "penguin" to Sprite(
        listOf("..oooo", ".oaaaa", "oaaaaa", "oaawww", "oawkww", "oawwwn", "oawwww", "oawwww", ".oawww", "..oooo"),
        mapOf('o' to Color(0xFF10131C), 'a' to Color(0xFF2C3A5A), 'w' to Color(0xFFF5F7FB), 'k' to Color(0xFF10131C), 'n' to Color(0xFFFFA726)),
        'w',
        Color(0xFFE1EAFB),
        Color(0xFF1F2A40),
    ),
    "pig" to Sprite(
        listOf("oo....", "obo...", "oaoooo", "oaaaaa", "oakaaa", "oaaaaa", "oabbbb", "oabdbb", "oabbbb", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF6A2F3D), 'a' to Color(0xFFFFB8C6), 'b' to Color(0xFFFF8FA6), 'd' to Color(0xFF6A2F3D), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFFFE8EE),
        Color(0xFF3D2229),
    ),
    "chick" to Sprite(
        listOf("....o.", "..oooo", ".oaaaa", "oaaaaa", "oakaaa", "oaaann", "obaaaa", "oaaaaa", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF5A3B00), 'a' to Color(0xFFFFD43B), 'b' to Color(0xFFFFB3A0), 'k' to Color(0xFF1B1B1B), 'n' to Color(0xFFFF922B)),
        'a',
        Color(0xFFFFF6CF),
        Color(0xFF3A3214),
    ),
    "dog" to Sprite(
        listOf("..oooo", ".oaaaa", "odoaaa", "oddaaa", "oddkaa", "oddaaa", "odoaww", ".oaawx", "..oaww", "...ooo"),
        mapOf('o' to Color(0xFF3B2A1E), 'a' to Color(0xFFE8C39A), 'd' to Color(0xFF8A5A3B), 'w' to Color(0xFFFFF7EE), 'x' to Color(0xFF1B1B1B), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFF7ECDF),
        Color(0xFF35291F),
    ),
    "bear" to Sprite(
        listOf(".oo...", "obbo..", "oaaooo", "oaaaaa", "oakaaa", "oaaaaa", "oaaabb", "oaabbx", ".oaabb", "..oooo"),
        mapOf('o' to Color(0xFF2A1A10), 'a' to Color(0xFF9A6A44), 'b' to Color(0xFFD8B08A), 'x' to Color(0xFF1B1B1B), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFF1E4D6),
        Color(0xFF33251B),
    ),
    "koala" to Sprite(
        listOf("ooo...", "obbo..", "obbooo", "oaaaaa", "oakaaa", "oaaaxx", "oaaaxx", "oaaaax", ".oaaaa", "..oooo"),
        mapOf('o' to Color(0xFF2E3440), 'a' to Color(0xFFA7B1BF), 'b' to Color(0xFFECEFF4), 'x' to Color(0xFF2E3440), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFE8EDF3),
        Color(0xFF252B35),
    ),
    "owl" to Sprite(
        listOf("o.....", "oo....", "oaoooo", "owwwwa", "owkkwa", "owwwwa", "oaaaan", "oabbba", ".oabbb", "..oooo"),
        mapOf('o' to Color(0xFF2B1D12), 'a' to Color(0xFF9C6B3E), 'w' to Color(0xFFFFF3D6), 'b' to Color(0xFFD9B48A), 'n' to Color(0xFFF0A020), 'k' to Color(0xFF1B1B1B)),
        'w',
        Color(0xFFF5EAD8),
        Color(0xFF33271A),
    ),
    "monkey" to Sprite(
        listOf("..oooo", ".oaaaa", "ooabbb", "obabkb", "ooabbb", ".oabbx", ".obbbb", ".obxxx", "..obbb", "...ooo"),
        mapOf('o' to Color(0xFF3B2414), 'a' to Color(0xFF7A4A2A), 'b' to Color(0xFFF0C9A0), 'x' to Color(0xFF3B2414), 'k' to Color(0xFF1B1B1B)),
        'b',
        Color(0xFFF6E6D4),
        Color(0xFF35251A),
    ),
    "tiger" to Sprite(
        listOf("o.....", "oo....", "owo...", "oaoooo", "osaasa", "oakaaa", "osaaaa", "owwwan", ".owwww", "..oooo"),
        mapOf('o' to Color(0xFF2A1A0A), 'a' to Color(0xFFF28C28), 's' to Color(0xFF2A1A0A), 'w' to Color(0xFFFFF4E0), 'n' to Color(0xFFFF8FAB), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFFFECCF),
        Color(0xFF3A2814),
    ),
    "mouse" to Sprite(
        listOf("ooo...", "obbo..", "obbo..", ".oaooo", ".oaaaa", "oakaaa", "oaaaaa", ".oaaan", "..oaaa", "...ooo"),
        mapOf('o' to Color(0xFF3A3A44), 'a' to Color(0xFFB8BCC8), 'b' to Color(0xFFFFB3C6), 'n' to Color(0xFFFF7FA0), 'k' to Color(0xFF1B1B1B)),
        'a',
        Color(0xFFEEEEF3),
        Color(0xFF2B2B33),
    ),
    "cow" to Sprite(
        listOf("h.....", "hooooo", "odwwww", "oddwww", "owkwww", "owwwww", "obbbbb", "obxbbb", ".obbbb", "..oooo"),
        mapOf('o' to Color(0xFF2B2B2B), 'w' to Color(0xFFF8F8F8), 'd' to Color(0xFF2B2B2B), 'b' to Color(0xFFFFB8C6), 'h' to Color(0xFFD9C9A0), 'x' to Color(0xFF8A4A5A), 'k' to Color(0xFF1B1B1B)),
        'w',
        Color(0xFFEEF2E6),
        Color(0xFF28301F),
    ),
)

val OS_SPRITES: Map<String, OsSprite> = mapOf(
    "windows" to OsSprite(listOf("xxx.xxx", "xxx.xxx", "xxx.xxx", ".......", "xxx.xxx", "xxx.xxx", "xxx.xxx"), mapOf('x' to Color(0xFF1A8FFF)), "Windows"),
    "macos" to OsSprite(listOf("....x..", "...x...", ".xx.xx.", "xxxxxxx", "xxxxxx.", "xxxxxx.", "xxxxxxx", ".xx.xx."), mapOf('x' to null), "macOS"),
    "ios" to OsSprite(listOf("....x..", "...x...", ".xx.xx.", "xxxxxxx", "xxxxxx.", "xxxxxx.", "xxxxxxx", ".xx.xx."), mapOf('x' to null), "iOS"),
    "linux" to OsSprite(listOf("..xxx..", ".xwxwx.", ".xyyyx.", "xxwwwxx", "xwwwwwx", "xwwwwwx", ".xwwwx.", "yy...yy"), mapOf('x' to Color(0xFF1B1B1B), 'w' to Color(0xFFFFFFFF), 'y' to Color(0xFFF5B50A)), "Linux"),
    "android" to OsSprite(listOf(".x...x.", "..xxx..", ".xxxxx.", "xwxxxwx", "xxxxxxx", "xxxxxxx"), mapOf('x' to Color(0xFF3DDC84), 'w' to Color(0xFF10131C)), "Android"),
)
