export const sound = null;
export async function actions({ page, check, report, shot }) {
  const first = await report();
  check(first.includes("playing"), `begins playing: ${first}`);
  await page.waitForTimeout(1000);
  const later = await report();
  check(Number(later.match(/frame (\d+)/)[1]) > Number(first.match(/frame (\d+)/)[1]), `the movie goes on: ${later}`);
  check(await page.locator("#sliders input").count() === 6, "six sliders");
  await page.locator("#slider0").fill("0.5");
  const noon = await report();
  check(noon.includes("paused") && noon.includes("sun 0.500"), `the sun slider: ${noon}`);
  await page.waitForTimeout(300);
  await shot("noon");
  await page.keyboard.down("ArrowUp");
  await page.waitForTimeout(400);
  await page.keyboard.up("ArrowUp");
  const moved = await report();
  check(moved.match(/camera (\S+)/)[1] !== noon.match(/camera (\S+)/)[1] || moved !== noon, `flies: ${moved}`);
  await page.keyboard.press(" ");
  check((await report()).includes("playing"), "space plays again");
}
