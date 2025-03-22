import {
  Runtime,
  Instant,
  spawn,
  spawn_primary,
  sleep,
} from "../pkg/bach_js.js";

Deno.test("spawn", async () => {
  let rt = new Runtime();
  await rt.run(() => {
    spawn_primary(async () => {
      console.log("Hello, world!");
      console.log(Instant.now().toString());
      await sleep(1);
      console.log("After sleep");
      // TODO this doesn't work yet
      // console.log(Instant.now().toString());
    });
  });
});
