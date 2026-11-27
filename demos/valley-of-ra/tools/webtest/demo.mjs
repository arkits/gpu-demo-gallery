export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("anubis 0"), `idle: ${await report()}`);
  await page.keyboard.down("8"); await page.waitForTimeout(50); await page.keyboard.up("8");
  check((await report()).includes("anubis 1"), `punch: ${await report()}`);
  await page.click('#modes [data-key="o"]');
  await page.waitForTimeout(600);
  await shot("background");
}
