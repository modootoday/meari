// Rust target -> npm platform package. The launcher (npm/bin/meari.js) and the release
// scripts share this table so a published platform package is always found.
export const PLATFORMS = [
  { target: "x86_64-unknown-linux-gnu", os: "linux", cpu: "x64", libc: "glibc" },
  { target: "aarch64-unknown-linux-gnu", os: "linux", cpu: "arm64", libc: "glibc" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64" },
];

export const SCOPE = "@modootoday";
export const MAIN = `${SCOPE}/meari-cli`;

export function packageName(p) {
  return `${MAIN}-${p.os}-${p.cpu}`;
}

export function binaryName(p) {
  return p.os === "win32" ? "meari.exe" : "meari";
}
