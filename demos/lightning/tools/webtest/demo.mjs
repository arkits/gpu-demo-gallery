export const sound = "background";
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("lightning off"), `begins: ${await report()}`);
  await page.mouse.move(512, 540); await page.mouse.down();
  await page.waitForTimeout(3000);
  check((await report()).startsWith("lightning on"), `pressed: ${await report()}`);
  const s = await page.evaluate(() => window.demo.sound());
  check(s.playing.includes("lightning"), `sound: ${JSON.stringify(s)}`);
  await shot("lightning");
  await page.mouse.up();
  await page.waitForTimeout(200);
  check((await report()).startsWith("lightning off"), `let go: ${await report()}`);
  await page.click('#modes [data-key=" "]');
  await page.keyboard.press("b");
  await page.waitForTimeout(800);
  check((await report()).startsWith("lightning on, bumps off"), `keys: ${await report()}`);
  await page.keyboard.down("ArrowUp"); await page.waitForTimeout(900); await page.keyboard.up("ArrowUp");
  check(parseFloat((await report()).split("distance ")[1]) < 5.9, `nearer: ${await report()}`);
  await shot("near");
  await page.keyboard.press("w"); await page.waitForTimeout(200); await shot("wires"); await page.keyboard.press("w");
}
