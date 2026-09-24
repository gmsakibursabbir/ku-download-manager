import { memo } from "react";

/**
 * 8-bit animal faces for KuAirSend devices. Each sprite is the left half of a
 * 12-pixel-wide face (mirrored for the right half):
 *   o outline · a fur · b/w/d/n extra colours · k eye (blinks to the `lid` colour)
 */
interface Sprite {
  half: string[];
  pal: Record<string, string>;
  lid: string;
  /** Circle background (light / dark). */
  bg: [string, string];
}

const SPRITES: Record<string, Sprite> = {
  cat: {
    half: ["o.....", "oo....", "obo...", "oaoooo", "oaaaaa", "oakaaa", "oakaaa", "obaaan", "oaaaaa", ".oaaaa", "..oooo"],
    pal: { o: "#3b2314", a: "#f39c38", b: "#ffd9a8", k: "#1b1b1b", n: "#ff8fab" },
    lid: "a",
    bg: ["#fff0dc", "#3a2a1a"],
  },
  fox: {
    half: ["o.....", "oo....", "obo...", "obbo..", "oaaooo", "oaaaaa", "oakaaa", "owaaaa", "owwaaa", ".owwwd", "..owww", "...ooo"],
    pal: { o: "#3a1f10", a: "#f06a24", b: "#ffc9a0", w: "#fff4e6", d: "#1b1b1b", k: "#1b1b1b" },
    lid: "a",
    bg: ["#ffe6d8", "#3d2418"],
  },
  frog: {
    half: [".ooo..", "owwwo.", "owkwoo", "owwwoa", "oaaaaa", "oaaaaa", "onaaaa", "oaoooo", ".oaaaa", "..oooo"],
    pal: { o: "#1d3b17", a: "#5cc15a", w: "#ffffff", k: "#1b1b1b", n: "#ff9fb5" },
    lid: "w",
    bg: ["#e2f6dd", "#1c3320"],
  },
  panda: {
    half: [".oo...", "oddo..", "oddooo", "owwwww", "owddww", "owdkww", "owddww", "owwwwd", ".owwww", "..oooo"],
    pal: { o: "#1b1b1b", w: "#ffffff", d: "#2e2e33", k: "#ffffff" },
    lid: "d",
    bg: ["#e8ebf2", "#2a2d36"],
  },
  bunny: {
    half: ["..oo..", ".obo..", ".obo..", ".obo..", ".oaooo", "oaaaaa", "oakaaa", "obaaan", "oaaaaa", ".oaaaa", "..oooo"],
    pal: { o: "#4a4052", a: "#f6f2f8", b: "#ffb3c6", k: "#1b1b1b", n: "#ff7fa0" },
    lid: "a",
    bg: ["#fbe9f2", "#3a2733"],
  },
  penguin: {
    half: ["..oooo", ".oaaaa", "oaaaaa", "oaawww", "oawkww", "oawwwn", "oawwww", "oawwww", ".oawww", "..oooo"],
    pal: { o: "#10131c", a: "#2c3a5a", w: "#f5f7fb", k: "#10131c", n: "#ffa726" },
    lid: "w",
    bg: ["#e1eafb", "#1f2a40"],
  },
  pig: {
    half: ["oo....", "obo...", "oaoooo", "oaaaaa", "oakaaa", "oaaaaa", "oabbbb", "oabdbb", "oabbbb", ".oaaaa", "..oooo"],
    pal: { o: "#6a2f3d", a: "#ffb8c6", b: "#ff8fa6", d: "#6a2f3d", k: "#1b1b1b" },
    lid: "a",
    bg: ["#ffe8ee", "#3d2229"],
  },
  chick: {
    half: ["....o.", "..oooo", ".oaaaa", "oaaaaa", "oakaaa", "oaaann", "obaaaa", "oaaaaa", ".oaaaa", "..oooo"],
    pal: { o: "#5a3b00", a: "#ffd43b", b: "#ffb3a0", k: "#1b1b1b", n: "#ff922b" },
    lid: "a",
    bg: ["#fff6cf", "#3a3214"],
  },
};

export const ANIMAL_NAMES: Record<string, string> = {
  cat: "Cat",
  fox: "Fox",
  frog: "Frog",
  panda: "Panda",
  bunny: "Bunny",
  penguin: "Penguin",
  pig: "Pig",
  chick: "Chick",
};

function spriteRects(s: Sprite) {
  const px: { x: number; y: number; c: string; eye: boolean }[] = [];
  s.half.forEach((row, y) => {
    const full = row + [...row].reverse().join("");
    [...full].forEach((ch, x) => {
      if (ch === ".") return;
      if (ch === "k") {
        px.push({ x, y, c: s.pal[s.lid], eye: false });
        px.push({ x, y, c: s.pal.k, eye: true });
      } else {
        px.push({ x, y, c: s.pal[ch] ?? "#ff00ff", eye: false });
      }
    });
  });
  return px;
}

const CACHE = new Map<string, ReturnType<typeof spriteRects>>();

/** A round avatar with an animated 8-bit animal. */
export const PixelAnimal = memo(function PixelAnimal({ animal, size = 64, seed = 0, still }: { animal: string; size?: number; seed?: number; still?: boolean }) {
  const s = SPRITES[animal] ?? SPRITES.cat;
  let rects = CACHE.get(animal);
  if (!rects) CACHE.set(animal, (rects = spriteRects(s)));
  const rows = s.half.length;
  // Different devices bob and blink out of step.
  const delay = { animationDelay: `${-(seed % 17) * 0.23}s` };
  return (
    <span className="px-avatar" style={{ width: size, height: size, ["--px-bg-light" as string]: s.bg[0], ["--px-bg-dark" as string]: s.bg[1] }} data-still={still || undefined}>
      <svg viewBox={`-1.75 ${-(15 - rows) / 2} 15.5 15`} shapeRendering="crispEdges" aria-hidden="true">
        <g className="px-sprite" style={delay}>
          {rects.map((r, i) => (
            <rect key={i} x={r.x} y={r.y} width={1.02} height={1.02} fill={r.c} className={r.eye ? "px-eye" : undefined} style={r.eye ? delay : undefined} />
          ))}
        </g>
      </svg>
    </span>
  );
});
