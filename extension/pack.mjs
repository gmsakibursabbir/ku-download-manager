// Packages the built extension:
//   dist/kudmx.crx  — Chromium CRX3, signed with .secrets/extension-key.pem
//                     (the same key as manifest "key", so the id is stable)
//   dist/kudmx.xpi  — Firefox add-on (unsigned; see README for Mozilla signing)
//
// Run after build.mjs:  node pack.mjs
// No dependencies: a minimal ZIP writer and the CRX3 protobuf are built here.

import { createHash, createPrivateKey, createPublicKey, sign } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateRawSync } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "dist");

function files(dir) {
  const out = [];
  for (const e of readdirSync(dir).sort()) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) out.push(...files(p));
    else out.push(p);
  }
  return out;
}

/** Deterministic ZIP (deflate, fixed timestamp) of a folder's contents. */
function zip(dir) {
  const locals = [];
  const central = [];
  let offset = 0;
  const dosTime = 0; // 00:00:00
  const dosDate = ((2025 - 1980) << 9) | (1 << 5) | 1; // 2025-01-01
  for (const path of files(dir)) {
    const name = Buffer.from(relative(dir, path).split(sep).join("/"), "utf8");
    const data = readFileSync(path);
    const deflated = deflateRawSync(data, { level: 9 });
    const useDeflate = deflated.length < data.length;
    const body = useDeflate ? deflated : data;
    const crc = crc32(data) >>> 0;
    const method = useDeflate ? 8 : 0;
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0);
    lh.writeUInt16LE(20, 4); // version needed
    lh.writeUInt16LE(0x0800, 6); // UTF-8 names
    lh.writeUInt16LE(method, 8);
    lh.writeUInt16LE(dosTime, 10);
    lh.writeUInt16LE(dosDate, 12);
    lh.writeUInt32LE(crc, 14);
    lh.writeUInt32LE(body.length, 18);
    lh.writeUInt32LE(data.length, 22);
    lh.writeUInt16LE(name.length, 26);
    lh.writeUInt16LE(0, 28);
    locals.push(lh, name, body);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0);
    ch.writeUInt16LE(20, 4); // made by
    ch.writeUInt16LE(20, 6);
    ch.writeUInt16LE(0x0800, 8);
    ch.writeUInt16LE(method, 10);
    ch.writeUInt16LE(dosTime, 12);
    ch.writeUInt16LE(dosDate, 14);
    ch.writeUInt32LE(crc, 16);
    ch.writeUInt32LE(body.length, 20);
    ch.writeUInt32LE(data.length, 24);
    ch.writeUInt16LE(name.length, 28);
    ch.writeUInt32LE(offset, 42);
    central.push(ch, name);
    offset += lh.length + name.length + body.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(central.length / 2, 8);
  end.writeUInt16LE(central.length / 2, 10);
  end.writeUInt32LE(cd.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, cd, end]);
}

// ── protobuf helpers (CRX3 header) ──
function varint(n) {
  const out = [];
  do {
    let b = n & 0x7f;
    n = Math.floor(n / 128);
    if (n) b |= 0x80;
    out.push(b);
  } while (n);
  return Buffer.from(out);
}
const field = (num, bytes) => Buffer.concat([varint((num << 3) | 2), varint(bytes.length), bytes]);

function crx3(zipBytes, pem) {
  const priv = createPrivateKey(pem);
  const pub = createPublicKey(priv).export({ type: "spki", format: "der" });
  const crxId = createHash("sha256").update(pub).digest().subarray(0, 16);
  const signedData = field(1, crxId); // SignedData { crx_id }
  const len = Buffer.alloc(4);
  len.writeUInt32LE(signedData.length);
  const toSign = Buffer.concat([Buffer.from("CRX3 SignedData\x00", "binary"), len, signedData, zipBytes]);
  const signature = sign("sha256", toSign, priv);
  const proof = Buffer.concat([field(1, pub), field(2, signature)]); // AsymmetricKeyProof
  const header = Buffer.concat([field(2, proof), field(10000, signedData)]); // CrxFileHeader
  const pre = Buffer.alloc(12);
  pre.write("Cr24", 0, "binary");
  pre.writeUInt32LE(3, 4);
  pre.writeUInt32LE(header.length, 8);
  const id = [...crxId.toString("hex")].map((c) => String.fromCharCode(97 + parseInt(c, 16))).join("");
  return { bytes: Buffer.concat([pre, header, zipBytes]), id };
}

for (const t of ["chrome", "firefox"]) {
  if (!existsSync(join(dist, t, "manifest.json"))) {
    console.error(`dist/${t} missing — run node build.mjs first`);
    process.exit(1);
  }
}

// dist/packages/ holds exactly what the installer bundles.
const pkgs = join(dist, "packages");
mkdirSync(pkgs, { recursive: true });
const xpi = zip(join(dist, "firefox"));
writeFileSync(join(dist, "kudmx.xpi"), xpi);
writeFileSync(join(pkgs, "kudmx.xpi"), xpi);
if (existsSync(join(dist, "kudmx.signed.xpi"))) writeFileSync(join(pkgs, "kudmx.signed.xpi"), readFileSync(join(dist, "kudmx.signed.xpi")));
console.log(`dist/kudmx.xpi  ${(xpi.length / 1024).toFixed(1)} KB`);

// Key: .secrets/extension-key.pem locally, or the KU_EXTENSION_KEY secret (PEM text) in CI.
const keyPath = join(here, "..", ".secrets", "extension-key.pem");
const pem = existsSync(keyPath) ? readFileSync(keyPath) : process.env.KU_EXTENSION_KEY?.trim() ? Buffer.from(process.env.KU_EXTENSION_KEY) : null;
if (!pem) {
  console.error("No signing key (.secrets/extension-key.pem or KU_EXTENSION_KEY): skipping kudmx.crx");
  process.exit(0);
}
const chromeZip = zip(join(dist, "chrome"));
const { bytes, id } = crx3(chromeZip, pem);
const expected = readFileSync(join(here, "chrome-extension-id.txt"), "utf8").trim();
if (id !== expected) {
  console.error(`key mismatch: crx id ${id}, manifest id ${expected}`);
  process.exit(1);
}
writeFileSync(join(dist, "kudmx.crx"), bytes);
writeFileSync(join(pkgs, "kudmx.crx"), bytes);
writeFileSync(join(dist, "kudmx-chrome.zip"), chromeZip); // for store upload
console.log(`dist/kudmx.crx  ${(bytes.length / 1024).toFixed(1)} KB  id ${id}`);
