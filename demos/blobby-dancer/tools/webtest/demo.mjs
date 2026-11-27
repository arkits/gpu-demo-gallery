export const sound = "music000";
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("clip 0 "), `begins: ${await report()}`);
  check(/\d{4,} triangles/.test(await report()), `a body: ${await report()}`);
  const looks = (r) => r.match(/looks (\d+) > (\d+)/).slice(1).join(">");
  const before = looks(await report());
  await page.click('#modes [data-key="n"]');
  await page.waitForTimeout(300);
  check(looks(await report()) !== before, `next look: ${before} then ${await report()}`);
  await page.keyboard.press("c");
  await page.waitForTimeout(200);
  const [, ms, length] = (await report()).match(/\((\d+) ms of (\d+)\)/).map(Number);
  check(ms >= length - 1700, `next move: ${await report()}`);
  await shot("moving");
  await page.keyboard.press(" ");
  const at = await report();
  check(at.endsWith("paused"), `pauses: ${at}`);
  await page.waitForTimeout(300);
  check(await report() === at, "paused stays");
  await page.keyboard.press(" ");
}
