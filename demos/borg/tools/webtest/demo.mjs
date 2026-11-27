export const sound = null;
export async function actions({ page, check, report, shot }) {
  for (const k of "12345678") await page.keyboard.press(k);
  check((await report()).startsWith("textures  "), `keys take pictures away: ${await report()}`);
  await page.click('#modes [data-key="1"]');
  check((await report()).startsWith("textures 1 "), `buttons: ${await report()}`);
  for (const k of "2345678") await page.keyboard.press(k);
  await page.mouse.move(500, 400); await page.mouse.down(); await page.mouse.move(700, 300, { steps: 8 }); await page.mouse.up();
  await page.mouse.wheel(0, -200);
  await page.waitForTimeout(300);
  check(!(await report()).endsWith("5.00"), `zoom: ${await report()}`);
  await shot("near");
}
