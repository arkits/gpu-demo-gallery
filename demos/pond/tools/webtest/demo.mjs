export const sound = "pool";
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("photograph 60, 0 drops"), `begins: ${await report()}`);
  await page.mouse.click(512, 560);
  check((await report()).includes("1 drops, 1 rings"), `click: ${await report()}`);
  await page.mouse.click(650, 620, { button: "right" });
  check((await report()).includes("2 drops"), `right button: ${await report()}`);
  await page.click('#modes [data-key="x"]');
  await page.waitForTimeout(500);
  check((await report()).match(/3 drops, \d+ rings, waves on/), `button: ${await report()}`);
  await shot("ripples");
  await page.keyboard.press("p");
  await page.keyboard.press("1");
  await page.waitForTimeout(1500);
  await page.keyboard.press(" ");
  const at = await report();
  check(Number(at.match(/photograph (\d+)/)[1]) !== 60 && at.endsWith("stopped"), `the day goes on: ${at}`);
  await page.waitForTimeout(300);
  check(await report() === at, "stopped stays");
  await page.keyboard.press("h");
  await page.waitForTimeout(100);
  await shot("help");
}
