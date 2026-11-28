export const sound = "music";
const time = (r) => Number(r.match(/time ([\d.]+)/)[1]);
export async function actions({ page, check, report }) {
  const first = await report();
  check(first.startsWith("time "), `reports the frame: ${first}`);
  await page.waitForTimeout(1000);
  const later = await report();
  check(time(later) > time(first), `the flight goes on: ${later}`);
  await page.keyboard.press(" ");
  const a = await report();
  await page.waitForTimeout(400);
  check((await report()) === a, `space pauses: ${a}`);
  const held = await page.evaluate(() => window.demo.sound().context);
  check(held === "suspended", `space pauses the music: ${held}`);
  await page.keyboard.press(" ");
  await page.waitForTimeout(200);
  const going = await page.evaluate(() => window.demo.sound().context);
  check(going === "running", `space again plays it on: ${going}`);
  await page.keyboard.press("r");
  await page.waitForTimeout(300);
  check(time(await report()) < 1, "r restarts");
}
