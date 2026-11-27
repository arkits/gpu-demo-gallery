export const sound = null;
export async function actions({ page, check, report, shot }) {
  const first = await report();
  check(first.startsWith("script 1") && first.endsWith("running"), `begins: ${first}`);
  await page.waitForTimeout(1000);
  const later = await report();
  check(Number(later.match(/frame (\d+)/)[1]) > Number(first.match(/frame (\d+)/)[1]), `goes on: ${later}`);
  await shot("script1");
  await page.click('#modes [data-key="2"]');
  await page.waitForTimeout(1500);
  check((await report()).startsWith("script 2"), `second script: ${await report()}`);
  await shot("script2");
  await page.keyboard.press(" ");
  const at = await report();
  await page.waitForTimeout(400);
  check(at.endsWith("paused") && (await report()) === at, `pause holds: ${at}`);
  await page.keyboard.press("1");
  check((await report()).startsWith("script 1, frame 0"), "back to the first script");
}
