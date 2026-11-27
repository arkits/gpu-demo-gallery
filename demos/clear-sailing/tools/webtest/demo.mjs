export const sound = "soundtrack";
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("t "), `begins: ${await report()}`);
  check(/ship \d+ sails \d+/.test(await report()), `a ship: ${await report()}`);
  check(/triangles \d{4,}/.test(await report()), `triangles: ${await report()}`);
  const smoke = (r) => Number(r.match(/smoke (\d+)/)[1]);
  const before = smoke(await report());
  await page.click('#modes [data-key="f"]');
  await page.waitForTimeout(500);
  check(smoke(await report()) > before, `fire smokes: ${await report()}`);
  await shot("firing");
  await page.keyboard.press("h");
  await page.waitForTimeout(200);
  await shot("nohdr");
  await page.keyboard.press("h");
  await page.keyboard.press(" ");
  const at = await report();
  check(at.endsWith("paused"), `pauses: ${at}`);
  await page.waitForTimeout(300);
  check(await report() === at, "paused stays");
  await page.keyboard.press(" ");
}
