export const sound = null;
export async function actions({ page, check, report, shot }) {
  await page.keyboard.press("p");
  check((await report()).startsWith("plain"), `mode: ${await report()}`);
  await page.click('#modes [data-key="p"]');
  check((await report()).startsWith("bumps,"), `button: ${await report()}`);
  await page.keyboard.press("p"); await page.keyboard.press("p");
  await page.keyboard.press("t");
  check((await report()).includes("bump 1"), `bump: ${await report()}`);
  for (let i = 0; i < 10; i++) await page.keyboard.press("+");
  check((await report()).includes("7200"), `detail: ${await report()}`);
  await page.keyboard.press("g");
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(500);
  await shot("background");
}
