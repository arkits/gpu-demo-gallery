// Drives the web build in headless Chrome: webtest.mjs <url> <out dir> <command> [gpu]
//   show   start, menus, keys, walking, resize
//   still  moments of the clock, for comparison with native frames
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
const { chromium } = createRequire(process.env.PLAYWRIGHT_FROM + "/")("playwright-core");

const [, , url, out, command, gpu = "webgpu"] = process.argv;
mkdirSync(out, { recursive: true });
const flags = { webgpu: ["--enable-unsafe-webgpu"], webgl: [] }[gpu];
if (!flags) throw new Error(`unknown gpu ${gpu}`);
const problems = [];
const check = (ok, what) => {
  console.log(ok ? "ok  " : "FAIL", `[${gpu}]`, what);
  if (!ok) problems.push(what);
};
const browser = await chromium.launch({ channel: "chrome", headless: true, args: [...flags, "--mute-audio"] });
const page = await browser.newPage({ viewport: { width: 1024, height: 768 } });
if (gpu === "webgl") await page.addInitScript(() => Object.defineProperty(Navigator.prototype, "gpu", { get: () => undefined }));
const uses = () => page.evaluate(() => {
  const c = document.getElementById("view");
  return c.getContext("webgpu") ? "webgpu" : c.getContext("webgl2") ? "webgl" : "nothing";
});
page.on("console", (m) => {
  if (m.type() === "error" || (m.type() === "warning" && /Invalid (ShaderModule|RenderPipeline|CommandBuffer|BindGroup)|Error while parsing WGSL/.test(m.text()))) {
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
    await page.waitForSelector("#play:not([hidden])", { timeout: 120000 });
    const begin = Date.now();
    await page.click("#play");
    await page.waitForFunction(() => window.demo, null, { timeout: 120000 });
    console.log("    ", `[${gpu}] ready ${Date.now() - begin} ms after Start`);
    const used = await uses();
    check(used === gpu, `canvas uses ${used}`);
    await page.waitForTimeout(1500);
    const f = await frames(3000);
    check(f.fps > 20, `scene: ${f.fps.toFixed(0)} fps, worst frame ${f.worst.toFixed(0)} ms`);
    await shot("start");
    // The Navigation menu, then Pause.
    await page.mouse.click(80, 22);
    await page.waitForTimeout(300);
    await shot("menu");
    await page.mouse.click(100, 100);
    await page.waitForTimeout(300);
    check(await page.evaluate(() => !window.demo.player.playing()), "the Pause button pauses");
    await page.keyboard.press(" ");
    check(await page.evaluate(() => window.demo.player.playing()), "space plays again");
    // Walk forward and turn.
    await page.keyboard.down("ArrowUp");
    await page.waitForTimeout(800);
    await page.keyboard.up("ArrowUp");
    await page.mouse.move(600, 500); await page.mouse.down({ button: "right" }); await page.mouse.move(700, 480, { steps: 10 }); await page.mouse.up({ button: "right" });
    await page.waitForTimeout(300);
    await shot("walked");
    await page.keyboard.press("w");
    await page.waitForTimeout(300);
    await shot("wire");
    await page.keyboard.press("w");
    await page.setViewportSize({ width: 1600, height: 700 });
    await page.waitForTimeout(700);
    await shot("wide");
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(700);
    await shot("phone");
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "fits a phone");
  },
  async still() {
    for (const [name, query] of [["a", "time=1"], ["b", "time=20"], ["c", "time=40"]]) {
      await page.goto(`${url}?${query}`);
      await page.waitForFunction(() => document.documentElement.dataset.done, null, { timeout: 180000 });
      await page.waitForTimeout(150);
      const used = await uses();
      if (used !== gpu) check(false, `canvas uses ${used}`);
      await shot(name);
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
