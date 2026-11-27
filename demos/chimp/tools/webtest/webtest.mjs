// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu] [arg]
//   show   start, chapters, music, pause, free camera, resize
//   still  one frozen frame without the logo (args: chapter, seconds)
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, gpu = "webgpu", chapter = "1", time = "12"] = process.argv;
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
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
const started = () => page.waitForFunction(() => window.demo, null, { timeout: 90000 });
const state = () => page.evaluate(() => ({
  time: demo.player.time(), chapter: demo.player.chapter(), paused: demo.player.paused(),
  audio: demo.audio?.context.currentTime, sound: demo.audio?.context.state,
}));
const frames = (ms) => page.evaluate((ms) => new Promise((done) => {
  const start = performance.now();
  let n = 0, last = start, worst = 0;
  const tick = (now) => {
    n++; worst = Math.max(worst, now - last); last = now;
    now - start < ms ? requestAnimationFrame(tick) : done({ fps: n / ms * 1000, worst });
  };
  requestAnimationFrame(tick);
}), ms);

async function start() {
  await page.goto(url);
  await page.waitForSelector("#play:not([hidden])", { timeout: 30000 });
  const begin = Date.now();
  await page.click("#play");
  await started();
  const used = await uses();
  check(used === gpu, `canvas uses ${used}`);
  console.log("    ", `[${gpu}] ready ${Date.now() - begin} ms after Start`);
}

const commands = {
  async show() {
    await start();
    await page.waitForTimeout(2500);
    let s = await state();
    check(s.chapter === 0 && s.time > 1.5 && s.sound === "running", `plays (${s.time.toFixed(1)} s, sound ${s.sound})`);
    // One chapter, NORMAL.
    for (let c = 1; c <= 1; c++) {
      await page.keyboard.press(String(c));
      await page.keyboard.press("ArrowRight");
      await page.waitForTimeout(700);
      const f = await frames(1500);
      s = await state();
      check(s.chapter === c - 1 && f.fps > 50 && f.worst < 100, `chapter ${c}: ${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
      await shot(`${gpu}-chapter${c}`);
    }
    for (let i = 0; i < 20; i++) await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(700);
    s = await state();
    check(s.chapter === 0 && s.time < 1, "skipping past the end starts again");
    await page.keyboard.press("Space");
    await page.waitForTimeout(400);
    const a = await state();
    await page.waitForTimeout(700);
    const b = await state();
    check(a.paused && a.time === b.time && a.audio === b.audio, "pause holds picture and music");
    await page.keyboard.press("Space");
    await page.mouse.move(500, 400); await page.mouse.down(); await page.mouse.move(430, 370, { steps: 4 }); await page.mouse.up();
    await page.mouse.wheel(0, -200);
    await page.waitForTimeout(400);
    await shot(`${gpu}-orbit`);
    await page.keyboard.press("f");
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(600);
    await shot(`${gpu}-wide`);
  },
  async still() {
    await page.goto(`${url}?chapter=${chapter}&t=${time}`);
    await started();
    const used = await uses();
    check(used === gpu, `canvas uses ${used}`);
    await page.keyboard.press("l");
    await page.waitForTimeout(1200);
    await shot(`${gpu}-still`);
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
