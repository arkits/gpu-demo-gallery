// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu]
//   show   start, sound, frame rate, stages, models, drag, resize
//   still  every stage of every model, for comparison with native frames
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
    const at = () => page.evaluate(() => [window.demo.player.model(), window.demo.player.stage()].join());
    check(await at() === "0,4", "starts with reflection");
    await page.waitForTimeout(5000);
    check(await at() === "0,0", "the show begins by itself");
    for (let s = 0; s < 6; s++) {
      await page.click(`#modes [data-stage="${s}"]`);
      check(await at() === `0,${s}`, `stage ${s} on request`);
      const f = await frames(1000);
      check(f.fps > 50 && f.worst < 100, `stage ${s}: ${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
    }
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(4500);
    check(await at() === "1,3", "right arrow goes on to the next model");
    const music = await page.evaluate(() => window.demo.sound());
    check(music.looping === "techy" && music.level > 0.001, `music plays: ${JSON.stringify(music)}`);
    await page.keyboard.press("3");
    await page.waitForTimeout(4500);
    check(await at() === "2,0", "number keys choose a model");
    await page.keyboard.press("End");
    await shot("before");
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
  },
  async still() {
    for (let m = 0; m < 3; m++) for (let s = 0; s < 6; s++) {
      await page.goto(`${url}?model=${m}&stage=${s}&yaw=0.5&pitch=-0.3`);
      await page.waitForFunction(() => document.documentElement.dataset.done, null, { timeout: 90000 });
      await page.waitForTimeout(150);
      const used = await uses();
      if (used !== gpu) check(false, `canvas uses ${used}`);
      await shot(`${m}${s}`);
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
