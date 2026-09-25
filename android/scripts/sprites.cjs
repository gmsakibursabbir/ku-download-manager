// Generates android/.../ui/PixelSprites.kt from app/src/app/airsend/PixelAnimal.tsx.
const fs = require("fs");
const src = fs.readFileSync("app/src/app/airsend/PixelAnimal.tsx", "utf8").replace(/\r\n/g, "\n");
const grab = (name) => {
  const m = src.match(new RegExp("const " + name + String.raw`[^=]*=\s*(\{[\s\S]*?\n\});`));
  if (!m) throw new Error(name);
  return Function('"use strict";return (' + m[1] + ")")();
};
const SPRITES = grab("SPRITES");
const OS = grab("OS");
const col = (c) => (c === "currentColor" ? "null" : "Color(0xFF" + c.slice(1).toUpperCase() + ")");
const q = (s) => JSON.stringify(s);
let out = `package digital.kuduy.kudownloader.ui

import androidx.compose.ui.graphics.Color

// Generated from app/src/app/airsend/PixelAnimal.tsx by android/scripts/sprites.cjs.
// Edit the desktop file and regenerate so both apps draw the same animals.

class Sprite(val half: List<String>, val pal: Map<Char, Color>, val lid: Char, val bgLight: Color, val bgDark: Color)

/** Pixel badge: \`null\` colour = the text colour. */
class OsSprite(val rows: List<String>, val pal: Map<Char, Color?>, val name: String)

val SPRITES: Map<String, Sprite> = linkedMapOf(
`;
for (const [k, s] of Object.entries(SPRITES)) {
  out += `    ${q(k)} to Sprite(\n        listOf(${s.half.map(q).join(", ")}),\n        mapOf(${Object.entries(s.pal).map(([c, v]) => `'${c}' to ${col(v)}`).join(", ")}),\n        '${s.lid}',\n        ${col(s.bg[0])},\n        ${col(s.bg[1])},\n    ),\n`;
}
out += `)\n\nval OS_SPRITES: Map<String, OsSprite> = mapOf(\n`;
for (const [k, o] of Object.entries(OS)) {
  out += `    ${q(k)} to OsSprite(listOf(${o.rows.map(q).join(", ")}), mapOf(${Object.entries(o.pal).map(([c, v]) => `'${c}' to ${col(v)}`).join(", ")}), ${q(o.name)}),\n`;
}
out += `)\n`;
fs.writeFileSync("android/app/src/main/java/digital/kuduy/kudownloader/ui/PixelSprites.kt", out);
console.log(Object.keys(SPRITES).length, "sprites,", Object.keys(OS).length, "os");
