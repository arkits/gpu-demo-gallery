export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("room 1"), `start: ${await report()}`);
  await page.keyboard.press("3");
  check((await report()).includes("room 3"), `room key: ${await report()}`);
  await page.click('#modes [data-key="2"]');
  check((await report()).includes("room 2"), `room button: ${await report()}`);
  await page.keyboard.down("ArrowRight"); await page.waitForTimeout(300); await page.keyboard.up("ArrowRight");
  await page.waitForTimeout(400);
  await shot("background");
}
