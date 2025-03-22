const yielder = globalThis.requestAnimationFrame
  ? globalThis.requestAnimationFrame
  : (callback) => setTimeout(callback, 0);

function yieldNow() {
  return new Promise((resolve) => yielder(resolve));
}

export async function run(runtime) {
  while (true) {
    runtime.macrostep();

    if (!runtime.hasPrimary()) {
      return;
    }

    await yieldNow();
  }
}
