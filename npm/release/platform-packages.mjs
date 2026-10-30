#!/usr/bin/env node
// Builds one npm platform package from a release binary, or prints the main package's
// optionalDependencies. Usage:
//   platform-packages.mjs --target <rust-target> --bin <path> --version <v> --out <dir>
//   platform-packages.mjs --optional-dependencies --version <v>
import fs from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { PLATFORMS, packageName, binaryName } from "./platforms.mjs";

export function manifest(p, version) {
  const m = {
    name: packageName(p),
    version,
    description: `meari CLI binary for ${p.os}-${p.cpu}`,
    license: "MIT OR Apache-2.0",
    repository: { type: "git", url: "git+https://github.com/modootoday/meari.git" },
    os: [p.os],
    cpu: [p.cpu],
    files: ["bin/"],
    preferUnplugged: true,
  };
  if (p.libc) m.libc = [p.libc];
  return m;
}

export function optionalDependencies(version) {
  return Object.fromEntries(PLATFORMS.map((p) => [packageName(p), version]));
}

export function writePlatformPackage({ target, bin, version, out }) {
  const p = PLATFORMS.find((x) => x.target === target);
  if (!p) throw new Error(`unknown target ${target}`);
  const dir = path.join(out, `${p.os}-${p.cpu}`);
  fs.mkdirSync(path.join(dir, "bin"), { recursive: true });
  const dest = path.join(dir, "bin", binaryName(p));
  fs.copyFileSync(bin, dest);
  fs.chmodSync(dest, 0o755);
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify(manifest(p, version), null, 2) + "\n");
  return dir;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const { values } = parseArgs({
    options: {
      target: { type: "string" }, bin: { type: "string" }, version: { type: "string" },
      out: { type: "string", default: "platform" },
      "optional-dependencies": { type: "boolean", default: false },
    },
  });
  if (!values.version) throw new Error("--version is required");
  if (values["optional-dependencies"]) {
    process.stdout.write(JSON.stringify(optionalDependencies(values.version), null, 2) + "\n");
  } else {
    console.log(writePlatformPackage(values));
  }
}
