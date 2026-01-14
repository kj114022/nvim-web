// Worker entry point - imports same WASM module which detects worker context

async function start(): Promise<void> {
  try {
    console.log("[worker.ts] Initializing WASM in worker context...");
    // Dynamic import of WASM module (path relative to built output)
    const module = await import("../pkg/nvim_web_ui.js");
    await module.default();
    // main_js() is automatically called by wasm-bindgen start attribute
    // It detects worker context and calls worker_entry() internally
    console.log("[worker.ts] WASM initialized successfully");
  } catch (error) {
    console.error("[worker.ts] Failed to initialize:", error);
  }
}

start();

export {};
