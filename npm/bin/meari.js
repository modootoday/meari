#!/usr/bin/env node
// Runs the meari binary from the platform package npm installed as an optional dependency.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";
import { PLATFORMS, packageName, binaryName } from "../release/platforms.mjs";

export function resolvePackage(platform, arch) {
  const p = PLATFORMS.find((x) => x.os === platform && x.cpu === arch);
  return p && { name: packageName(p), bin: binaryName(p) };
}

function main() {
  const pkg = resolvePackage(process.platform, process.arch);
  if (!pkg) {
    console.error(`meari: no prebuilt binary for ${process.platform}-${process.arch}; use \`cargo install meari-cli\``);
    process.exit(1);
  }
  let exe;
  try {
    const require = createRequire(import.meta.url);
    exe = path.join(path.dirname(require.resolve(`${pkg.name}/package.json`)), "bin", pkg.bin);
  } catch {
    console.error(`meari: ${pkg.name} is not installed (optional dependencies disabled?); reinstall without --no-optional`);
    process.exit(1);
  }
  const r = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
  if (r.error) throw r.error;
  process.exit(r.status ?? 1);
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1]?.endsWith(`${path.sep}meari`)) main();
