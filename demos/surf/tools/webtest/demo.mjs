export const sound = null;
export async function actions({ page, check, report, shot }) {
  await page.keyboard.press("p");
  check((await report()).includes('subdiv'), `report: ${await report()}`);
  await page.click('#modes [data-key="p"]');
  await page.keyboard.press("d");
  await page.keyboard.press("p"); await page.keyboard.press("p");
  await page.keyboard.press("t");
  await page.keyboard.press('w');
  await page.keyboard.press('=');
  check((await report()).length > 0, `report: ${await report()}`);
  await page.keyboard.press('h');
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(500);
  await shot("background");
}
