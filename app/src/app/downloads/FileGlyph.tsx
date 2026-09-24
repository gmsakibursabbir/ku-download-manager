import { useState } from "react";
import { File, FileArchive, FileAudio, FileImage, FileText, FileVideo, Disc3, Package, Magnet, Clapperboard, type LucideIcon } from "lucide-react";
import { Icon } from "../../ui/primitives";
import type { Download } from "../../lib/types";

export const CATEGORY_ICON: Record<string, LucideIcon> = {
  archives: FileArchive,
  "images-disk": Disc3,
  programs: Package,
  video: FileVideo,
  music: FileAudio,
  documents: FileText,
  images: FileImage,
  torrents: Magnet,
};

export function glyphFor(d: Pick<Download, "kind" | "category">): LucideIcon {
  if (d.kind === "torrent" || d.kind === "magnet") return Magnet;
  if (d.kind === "media") return d.category === "music" ? FileAudio : Clapperboard;
  return CATEGORY_ICON[d.category] ?? File;
}

/** File-type glyph, or the media thumbnail when one is known. */
export function FileGlyph({ d, size = 16 }: { d: Download; size?: number }) {
  const [failed, setFailed] = useState(false);
  const thumb = d.meta.thumbnail;
  return (
    <span className="row-icon">
      {thumb && !failed ? <img src={thumb} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setFailed(true)} /> : <Icon icon={glyphFor(d)} size={size} />}
    </span>
  );
}
