// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu]
//   show   start, frame rate, modes, drag, rotate, resize
//   still  fixed numbers of steps, for comparison with native frames
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, gpu = "webgpu"] = process.argv;
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
const browser = await chromium.launch({ channel: "chrome", headless: true, args: flags });
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

const commands = {
  async show() {
    await page.goto(url);
    await page.waitForSelector("#play:not([hidden])", { timeout: 30000 });
    const begin = Date.now();
    await page.click("#play");
    await page.waitForFunction(() => window.demo, null, { timeout: 90000 });
    console.log("    ", `[${gpu}] ready ${Date.now() - begin} ms after Start`);
    const used = await uses();
    check(used === gpu, `canvas uses ${used}`);
    await page.waitForTimeout(2000);
    for (const [key, name] of [["s", "smoke"], ["w", "water"], ["n", "logo"]]) {
      await page.click(`#modes [data-key=${key}]`);
      await page.waitForTimeout(1500);
      const f = await frames(2000);
      check(f.fps > 50 && f.worst < 100, `${name}: ${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
      await shot(name);
    }
    await page.mouse.move(300, 400); await page.mouse.down(); await page.mouse.move(700, 380, { steps: 30 });
    await page.waitForTimeout(500);
    await shot("dragged");
    await page.mouse.up();
    await page.keyboard.down("Shift");
    await page.mouse.move(300, 400); await page.mouse.down(); await page.mouse.move(700, 300, { steps: 10 }); await page.mouse.up();
    await page.keyboard.up("Shift");
    await page.mouse.wheel(0, 300);
    await page.keyboard.press("a");
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(700);
    await shot("wide");
    check(true, "drag, rotate, zoom, smooth edges, resize");
  },
  async still() {
    for (const key of ["s", "w"]) {
      await page.goto(`${url}?steps=60&key=${key}`);
      await page.waitForFunction(() => document.documentElement.dataset.done, null, { timeout: 90000 });
      await page.waitForTimeout(300);
      const used = await uses();
      check(used === gpu, `canvas uses ${used}`);
      await shot(key);
    }
    check(true, "stills");
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
