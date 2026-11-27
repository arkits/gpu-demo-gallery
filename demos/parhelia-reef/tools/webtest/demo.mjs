export const sound = "music";
const time = (r) => Number(r.match(/time ([\d.]+)/)[1]);
export async function actions({ page, check, report, shot }) {
  const first = await report();
  check(first.startsWith("time ") && first.includes("Camera01"), `reports the frame: ${first}`);
  await page.waitForTimeout(1000);
  const later = await report();
  check(time(later) > time(first), `the tour goes on: ${later}`);
  await page.keyboard.press(" ");
  const a = await report();
  await page.waitForTimeout(400);
  check((await report()) === a, `space pauses: ${a}`);
  await page.keyboard.press(" ");
  await page.keyboard.press("c");
  await page.waitForTimeout(300);
  await shot("camera");
  check((await report()).includes("Camera02"), "c changes the camera");
}
