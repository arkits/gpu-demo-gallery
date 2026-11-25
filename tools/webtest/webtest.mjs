// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu]
//   show   start, sound, frame rate, what demo.mjs does, drag, resize
//   still  a moment (seconds:keys), for comparison with the native frame
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
import { actions, sound } from "./demo.mjs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, gpu = "webgpu", moment = "0:"] = process.argv;
mkdirSync(out, { recursive: true });
const flags = {
  webgpu: ["--enable-unsafe-webgpu"],
  webgl: [],
}[gpu];
if (!flags) throw new Error(`unknown gpu ${gpu}`);
const problems = [];
const check = (ok, what) => {
  console.log(ok ? "ok  " : "FAIL", `[${gpu}]`, what);
  if (!ok) problems.push(what);
};
const browser = await chromium.launch({ channel: "chrome", headless: true, args: [...flags, "--mute-audio"] });
const page = await browser.newPage({ viewport: { width: 1024, height: 768 } });
// Chrome's flags for turning WebGPU off do not work; hide it from the page instead.
if (gpu === "webgl") await page.addInitScript(() => Object.defineProperty(Navigator.prototype, "gpu", { get: () => undefined }));
// What the canvas really uses: asking for another kind of context gives null.
const uses = () => page.evaluate(() => {
  const c = document.getElementById("view");
  return c.getContext("webgpu") ? "webgpu" : c.getContext("webgl2") ? "webgl" : "nothing";
});
page.on("console", (m) => m.type() === "error" && check(false, `console: ${m.text().slice(0, 300)}`));
page.on("pageerror", (e) => check(false, `page: ${String(e).slice(0, 300)}`));
const shot = (name) => page.screenshot({ path: `${out}/${gpu}-${name}.png` });
const frames = (ms) => page.evaluate((ms) => new Promise((done) => {
  const start = performance.now();
  let n = 0, last = start, worst = 0;
  const tick = (now) => {
    n++; worst = Math.max(worst, now - last); last = now;
    now - start < ms ? requestAnimationFrame(tick) : done({ fps: n / ms * 1000, worst });
  };
  requestAnimationFrame(tick);
}), ms);

const report = () => page.evaluate(() => window.demo.player.report());

const commands = {
  async show() {
    await page.goto(url);
    await page.waitForSelector("#play:not([hidden])", { timeout: 30000 });
    await shot("start");
    const begin = Date.now();
    await page.click("#play");
    await page.waitForFunction(() => window.demo, null, { timeout: 90000 });
    console.log("    ", `[${gpu}] ready ${Date.now() - begin} ms after Start`);
    const used = await uses();
    check(used === gpu, `canvas uses ${used}`);
    await page.waitForTimeout(2000);
    const f = await frames(3000);
    check(f.fps > 50 && f.worst < 100, `${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
    if (sound) {
      const s = await page.evaluate(() => window.demo.sound());
      check(s.playing.includes(sound) && s.level > 0.0002, `sound plays: ${JSON.stringify(s)}`);
    }
    await shot("show");
    await actions({ page, check, report, shot, frames });
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(700);
    await shot("wide");
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(700);
    await shot("phone");
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "fits a phone");
  },
  async still() {
    const [time, keys] = moment.split(":");
    await page.goto(`${url}?time=${time}&keys=${encodeURIComponent(keys)}`);
    await page.waitForFunction(() => document.documentElement.dataset.done, null, { timeout: 90000 });
    await page.waitForTimeout(150);
    const used = await uses();
    check(used === gpu, `still ${moment}, canvas uses ${used}`);
    await shot("still");
  },
};
try {
  if (!commands[command]) throw new Error(`unknown command ${command}`);
  await commands[command]();
} catch (e) {
  check(false, e.message);
}
await browser.close();
process.exit(problems.length ? 1 : 0);
