export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).length > 0, `report: ${await report()}`);
  const keys = "w p".split(" ").filter(Boolean);
  for (const k of keys) {
    await page.keyboard.press(k);
    await page.click(`#modes [data-key="${k}"]`);
  }
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(400);
  await shot("background");
}
