// Headless Chrome screenshot over the DevTools pipe (no npm deps, Node >= 22).
// `chrome --screenshot` does not terminate in this sandbox, so drive CDP directly.
// usage: node cdp-screenshot.mjs CHROME URL OUT.png WIDTH HEIGHT [SCRIPT]
// SCRIPT (optional) runs after load, e.g. fixed scrollTo offsets from `<fixture>.scroll`.
import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [chrome, url, out, w, h, script = ""] = process.argv.slice(2);
const width = Number(w), height = Number(h);
const profile = mkdtempSync(join(tmpdir(), "meari-visual-"));
const child = spawn(chrome, [
  "--headless", "--no-sandbox", "--disable-gpu", "--disable-extensions", "--no-first-run",
  "--hide-scrollbars", "--force-device-scale-factor=1", "--force-color-profile=srgb",
  "--font-render-hinting=none", `--user-data-dir=${profile}`, "--remote-debugging-pipe",
  `--window-size=${width},${height}`, ...(process.env.MEARI_CHROME_FLAGS ? process.env.MEARI_CHROME_FLAGS.split(" ") : []), "about:blank",
], { stdio: ["ignore", "ignore", "ignore", "pipe", "pipe"] });
const [, , , toChrome, fromChrome] = child.stdio;
let id = 0, buf = "";
const pending = new Map();
fromChrome.on("data", (chunk) => {
  buf += chunk.toString("utf8");
  let i;
  while ((i = buf.indexOf("\0")) >= 0) {
    const msg = JSON.parse(buf.slice(0, i)); buf = buf.slice(i + 1);
    if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
    else if (msg.method && pending.has(msg.method)) { pending.get(msg.method)(msg); pending.delete(msg.method); }
  }
});
const send = (method, params = {}, sessionId) => new Promise((ok, fail) => {
  const n = ++id;
  pending.set(n, (m) => (m.error ? fail(new Error(JSON.stringify(m.error))) : ok(m.result)));
  toChrome.write(JSON.stringify({ id: n, method, params, sessionId }) + "\0");
});
const once = (method) => new Promise((ok) => pending.set(method, ok));
const timer = setTimeout(() => { console.error("timeout"); finish(3); }, 30000);
function finish(code) { clearTimeout(timer); child.kill("SIGKILL"); try { rmSync(profile, { recursive: true, force: true }); } catch {} process.exit(code); }
try {
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false }, sessionId);
  await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 255, g: 255, b: 255, a: 1 } }, sessionId);
  const loaded = once("Page.loadEventFired");
  await send("Page.navigate", { url }, sessionId);
  await loaded;
  if (script) {
    const r = await send("Runtime.evaluate", { expression: script }, sessionId);
    if (r.exceptionDetails) throw new Error("script: " + JSON.stringify(r.exceptionDetails));
    await send("Runtime.evaluate", { expression: "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))", awaitPromise: true }, sessionId);
  }
  // Diagnostics (no effect on the image): per text input, the laid-out width, the font Chrome
  // resolved, and the intrinsic width measured on an unconstrained clone. Written next to
  // OUT.png so CI artifacts show cross-OS input sizing differences.
  const metrics = await send("Runtime.evaluate", { returnByValue: true, awaitPromise: true, expression: `(async () => {
    await document.fonts.ready;
    return [...document.querySelectorAll("input")].map((el) => {
      const cs = getComputedStyle(el);
      const host = document.createElement("div");
      host.style.cssText = "position:absolute;left:0;top:0;width:max-content;visibility:hidden";
      const clone = el.cloneNode(true);
      host.appendChild(clone); document.body.appendChild(host);
      const intrinsic = clone.getBoundingClientRect().width;
      host.remove();
      return { width: el.getBoundingClientRect().width, intrinsic, minWidth: cs.minWidth,
        flexShrink: cs.flexShrink, fontFamily: cs.fontFamily, fontSize: cs.fontSize,
        notoLoaded: document.fonts.check(cs.fontSize + " " + cs.fontFamily) };
    });
  })()` }, sessionId);
  if (metrics.result?.value?.length) writeFileSync(out.replace(/\.png$/, "-metrics.json"), JSON.stringify({ chrome: (await send("Browser.getVersion")).product, inputs: metrics.result.value }, null, 1));
  const { data } = await send("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width, height, scale: 1 } }, sessionId);
  writeFileSync(out, Buffer.from(data, "base64"));
  finish(0);
} catch (e) { console.error(String(e)); finish(2); }
