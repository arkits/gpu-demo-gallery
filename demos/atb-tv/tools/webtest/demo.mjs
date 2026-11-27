export const sound = null;
export async function actions({ page, check, report, shot }) {
  // The movie advances a frame a refresh.
  const a = await report(); await page.waitForTimeout(300);
  check(a !== (await report()), `movie plays: ${await report()}`);
  await page.keyboard.press("l");
  check((await report()).includes("lighting 1"), `lighting key: ${await report()}`);
  await page.click('#modes [data-key="c"]');
  check((await report()).includes("target 1"), `movie on button: ${await report()}`);
  await page.waitForTimeout(400);
  await shot("background");
}
