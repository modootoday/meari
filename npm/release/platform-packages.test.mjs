import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { PLATFORMS, packageName } from "./platforms.mjs";
import { manifest, optionalDependencies, writePlatformPackage } from "./platform-packages.mjs";
import { resolvePackage } from "../bin/meari.js";

test("every platform has a unique scoped package name", () => {
  const names = PLATFORMS.map(packageName);
  assert.equal(new Set(names).size, names.length);
  for (const n of names) assert.match(n, /^@modootoday\/meari-cli-(linux|darwin|win32)-(x64|arm64)$/);
});

test("optionalDependencies pin every platform to the release version", () => {
  const deps = optionalDependencies("0.1.0");
  assert.equal(Object.keys(deps).length, PLATFORMS.length);
  assert.ok(Object.values(deps).every((v) => v === "0.1.0"));
});

test("linux manifests restrict libc to glibc", () => {
  const linux = PLATFORMS.find((p) => p.target === "x86_64-unknown-linux-gnu");
  assert.deepEqual(manifest(linux, "0.1.0").libc, ["glibc"]);
  const mac = PLATFORMS.find((p) => p.os === "darwin");
  assert.equal(manifest(mac, "0.1.0").libc, undefined);
});

test("platform package carries the binary and its manifest", () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "meari-npm-"));
  const bin = path.join(tmp, "meari");
  fs.writeFileSync(bin, "#!/bin/sh\n");
  const dir = writePlatformPackage({ target: "aarch64-apple-darwin", bin, version: "0.1.0", out: tmp });
  const m = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  assert.equal(m.name, "@modootoday/meari-cli-darwin-arm64");
  assert.ok(fs.existsSync(path.join(dir, "bin", "meari")));
});

test("launcher resolves the package for the running platform", () => {
  assert.equal(resolvePackage("linux", "x64").name, "@modootoday/meari-cli-linux-x64");
  assert.equal(resolvePackage("win32", "x64").bin, "meari.exe");
  assert.equal(resolvePackage("freebsd", "x64"), undefined);
});
