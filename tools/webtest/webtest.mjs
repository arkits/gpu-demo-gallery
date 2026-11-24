// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu] [arg]
//   show   start, sync, seek, pause, free camera, resize
//   still  one frozen frame without logos (arg: scene seconds)
//   full   the whole show in real time
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, gpu = "webgpu", arg] = process.argv;
mkdirSync(out, { recursive: true });
const flags = {
  webgpu: ["--enable-unsafe-webgpu"],
  webgl: ["--disable-blink-features=WebGPU", "--disable-features=WebGPU"],
}[gpu];
if (!flags) throw new Error(`unknown gpu ${gpu}`);
const problems = [];
const check = (ok, what) => {
  console.log(ok ? "ok  " : "FAIL", `[${gpu}]`, what);
  if (!ok) problems.push(what);
};
const browser = await chromium.launch({ channel: "chrome", headless: true, args: [...flags, "--mute-audio"] });
const page = await browser.newPage({ viewport: { width: 1024, height: 768 } });
page.on("console", (m) => m.type() === "error" && check(false, `console: ${m.text().slice(0, 300)}`));
page.on("pageerror", (e) => check(false, `page: ${String(e).slice(0, 300)}`));
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
const started = () => page.waitForFunction(() => window.pipedream, null, { timeout: 90000 });
const state = () => page.evaluate(() => ({
  time: pipedream.player.time(), audio: pipedream.audio?.currentTime, paused: pipedream.player.paused(),
}));
const offset = (s) => Math.abs(s.time - 10 - s.audio);
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
  console.log("    ", `[${gpu}] ready ${Date.now() - begin} ms after Start`);
}

const commands = {
  async show() {
    await start();
    await page.waitForTimeout(3500);
    let s = await state();
    check(offset(s) < 0.06, `in sync at start (${(offset(s) * 1000).toFixed(0)} ms)`);
    for (let i = 0; i < 12; i++) await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(2500);
    s = await state();
    check(s.time > 70 && offset(s) < 0.06, `in sync after seeking (${(offset(s) * 1000).toFixed(0)} ms)`);
    const f = await frames(3000);
    check(f.fps > 50 && f.worst < 100, `${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
    await shot(`${gpu}-play`);
    await page.keyboard.press("Space");
    await page.waitForTimeout(400);
    const a = await state();
    await page.waitForTimeout(700);
    const b = await state();
    check(a.paused && a.time === b.time && a.audio === b.audio, "pause holds picture and music");
    await page.keyboard.press("Space");
    await page.keyboard.press("4");
    await page.mouse.move(500, 400); await page.mouse.down(); await page.mouse.move(430, 370, { steps: 4 }); await page.mouse.up();
    await page.waitForTimeout(400);
    await shot(`${gpu}-orbit`);
    await page.keyboard.press("f");
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(600);
    await shot(`${gpu}-wide`);
  },
  async still() {
    await page.goto(`${url}?t=${arg ?? 100}`);
    await started();
    await page.keyboard.press("l");
    await page.waitForTimeout(1500);
    await shot(`${gpu}-still`);
    check(true, `still at ${arg ?? 100}`);
  },
  async full() {
    await start();
    await page.evaluate(() => {
      const s = (window.stats = { n: 0, late: 0, offset: 0, looped: false, last: performance.now(), time: 0 });
      const tick = (now) => {
        s.n++; if (now - s.last > 25) s.late++; s.last = now;
        const t = pipedream.player.time(), a = pipedream.audio;
        if (t < s.time - 100) s.looped = true;
        s.time = t;
        if (!a.paused && !a.ended && a.currentTime > 0.5) s.offset = Math.max(s.offset, Math.abs(t - 10 - a.currentTime));
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    for (let i = 0; i < 11; i++) {
      await page.waitForTimeout(20000);
      console.log("    ", JSON.stringify(await page.evaluate(() => window.stats)));
    }
    const s = await page.evaluate(() => window.stats);
    check(s.looped, "show restarts");
    check(s.offset < 0.1, `stays in sync (${(s.offset * 1000).toFixed(0)} ms at worst)`);
    check(s.late < s.n / 100, `${s.late} late of ${s.n} frames`);
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
