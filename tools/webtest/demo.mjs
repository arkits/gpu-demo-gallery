export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("view 2"), `chase view: ${await report()}`);
  await page.keyboard.press("4");
  check((await report()).includes("view 4"), `view key: ${await report()}`);
  await page.click('#modes [data-key="5"]');
  check((await report()).includes("view 5"), `view button: ${await report()}`);
  await page.keyboard.press("-");
  check((await report()).includes("speed 60"), `slower: ${await report()}`);
  await page.waitForTimeout(400);
  await shot("background");
}
