// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu]
//   show   start, sound, action, drag, resize
//   still  one moment per take, for comparison with native frames
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, ...rest] = process.argv;
const gpu = rest.find((a) => a === "webgpu" || a === "webgl") ?? "webgpu";
const still = rest.filter((a) => a !== "webgpu" && a !== "webgl");
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
page.on("console", (m) => {
  if (m.type() === "error" || (m.type() === "warning" && /Invalid (ShaderModule|RenderPipeline|CommandBuffer)|Error while parsing WGSL/.test(m.text()))) {
    check(false, `console: ${m.text().slice(0, 300)}`);
  }
});
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
    await page.waitForTimeout(1500);
    const f = await frames(3000);
    check(f.fps > 20, `scene: ${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
    const music = await page.evaluate(() => window.demo.sound());
    check(music.playing === "soundtrack" && music.level > 0.0005, `music plays: ${JSON.stringify(music)}`);
    await shot("before");
    await page.keyboard.press("3");
    await page.waitForTimeout(400);
    const take = await page.evaluate(() => window.demo.player.take());
    check(take === 2, `take key: ${take}`);
    await page.keyboard.press("c");
    await page.waitForTimeout(400);
    check(await page.evaluate(() => window.demo.player.mode()) === "Free", "free camera");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(400);
    check(await page.evaluate(() => window.demo.player.expression()) === 1, "expression changes");
    await shot("action");
    await page.mouse.move(300, 300); await page.mouse.down(); await page.mouse.move(600, 260, { steps: 20 });
    await page.mouse.up();
    await page.mouse.wheel(0, 500);
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(700);
    await shot("wide");
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(700);
    await shot("phone");
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "fits a phone");
    await page.keyboard.press("c");
    check(await page.evaluate(() => window.demo.player.mode()) === "Helmet", "helmet camera");
    await page.keyboard.press("c");
    check(await page.evaluate(() => window.demo.player.mode()) === "Movie", "movie camera");
  },
  async still() {
    // Seconds, take and key of the still, matching scripts/dev.sh STILLS.
    const [seconds, take, key] = still;
    await page.goto(`${url}?time=${seconds}&take=${take}&key=${key}`);
    await page.waitForFunction(() => document.documentElement.dataset.done, null, { timeout: 90000 });
    await page.waitForTimeout(150);
    const used = await uses();
    if (used !== gpu) check(false, `canvas uses ${used}`);
    await shot(`still-${take}`);
    check(true, "still");
  },
};

if (!(command in commands)) throw new Error(`unknown command ${command}`);
await commands[command]();
await browser.close();
if (problems.length) {
  console.error(`FAILED: ${problems.join("; ")}`);
  process.exit(1);
}
console.log("passed");
