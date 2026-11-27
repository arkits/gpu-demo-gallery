export const sound = null;
export async function actions({ page, check, report, shot }) {
  const first = await report();
  check(first.startsWith("frame "), `reports the frame: ${first}`);
  await page.waitForTimeout(1000);
  const later = await report();
  check(Number(later.match(/frame (\d+)/)[1]) > Number(first.match(/frame (\d+)/)[1]), `the animation goes on: ${later}`);
  await page.keyboard.press(" ");
  const a = await report();
  await page.waitForTimeout(400);
  check((await report()) === a, `space pauses: ${a}`);
  await page.keyboard.press(" ");
  await shot("bottle");
  await page.keyboard.press("t");
  check((await report()).includes("dry 110"), `t dries the glass: ${await report()}`);
}
