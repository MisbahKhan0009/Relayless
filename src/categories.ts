import { Archive, File, FileText, Film, Image, Music, Package, type LucideIcon } from "lucide-react";

export type Category = "video" | "photos" | "audio" | "documents" | "archives" | "apps" | "other";

export const CATEGORIES: { id: Category; label: string; icon: LucideIcon; noun: string }[] = [
  { id: "video", label: "Video", icon: Film, noun: "Video file" },
  { id: "photos", label: "Photos", icon: Image, noun: "Image" },
  { id: "audio", label: "Audio", icon: Music, noun: "Audio file" },
  { id: "documents", label: "Documents", icon: FileText, noun: "Document" },
  { id: "archives", label: "Archives", icon: Archive, noun: "Archive" },
  { id: "apps", label: "Apps", icon: Package, noun: "App or disk image" },
  { id: "other", label: "Other", icon: File, noun: "File" },
];

const EXT: Record<string, Category> = {};
const add = (c: Category, list: string) => list.split(" ").forEach((e) => (EXT[e] = c));
add("video", "mov mp4 m4v mkv avi webm mxf r3d braw prores mts m2ts wmv flv mpg mpeg 3gp");
add("photos", "jpg jpeg png gif webp heic heif tif tiff bmp raw cr2 cr3 nef arw dng orf rw2 psd svg exr dpx");
add("audio", "wav mp3 flac aac m4a ogg opus aif aiff wma mid midi");
add("documents", "pdf doc docx xls xlsx csv ppt pptx odt ods odp txt md rtf tex json xml html epub key pages numbers");
add("archives", "zip 7z rar tar gz tgz bz2 xz zst lz4 cab");
add("apps", "exe msi apk aab dmg iso img appimage deb rpm pkg vhd vhdx vmdk ova");

export function categoryOf(ext: string): Category {
  return EXT[ext.toLowerCase()] ?? "other";
}

export function categoryInfo(c: Category) {
  return CATEGORIES.find((x) => x.id === c)!;
}
