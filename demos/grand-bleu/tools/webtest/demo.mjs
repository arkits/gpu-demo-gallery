export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("dolph5"), `first animal: ${await report()}`);
  await page.keyboard.press(" ");
  check((await report()).includes("dolph4"), `next key: ${await report()}`);
  await page.click('#modes [data-key=" "]');
  check((await report()).includes("shark"), `next button: ${await report()}`);
  await page.waitForTimeout(400);
  await shot("background");
}
