export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("trees 1,"), `begins: ${await report()}`);
  const before = await report();
  // The knob of the first slider, in a window of 1024 by 768.
  const y = 768 - (100 + 250) * 1.6;
  await page.mouse.move(62 * 1.6, y); await page.mouse.down(); await page.mouse.move(100 * 1.6, y, { steps: 5 }); await page.mouse.up();
  await page.waitForTimeout(200);
  check(parseFloat((await report()).split("fullness ")[1]) > 0.6, `slider: ${before} to ${await report()}`);
  await page.click('#modes [data-key="8"]');
  check((await report()).includes("fireflies 8"), `fireflies: ${await report()}`);
  await page.keyboard.press("l");
  check((await report()).includes("bills"), `bills: ${await report()}`);
  await shot("bills");
  await page.keyboard.press("l");
  await page.keyboard.press("g");
  await page.waitForTimeout(1500);
  check(parseInt((await report()).slice(6)) > 8, `grove: ${await report()}`);
  await page.mouse.move(600, 380); await page.mouse.down({ button: "right" }); await page.waitForTimeout(1500); await page.mouse.up({ button: "right" });
  check(parseFloat((await report()).split(", ").at(-1)) > 7.5, `back: ${await report()}`);
  await page.waitForTimeout(500);
  await shot("grove");
  await page.keyboard.press("w"); await page.waitForTimeout(300); await shot("wires"); await page.keyboard.press("w");
}
