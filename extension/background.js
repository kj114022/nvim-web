// background.js - Native Messaging Host Manager
const HOST_NAME = "com.kj114022.nvim_web";
let nativePort = null;

function connectToNativeHost() {
  console.log(`[nvim-web] Connecting to native host: ${HOST_NAME}`);
  nativePort = chrome.runtime.connectNative(HOST_NAME);

  nativePort.onMessage.addListener((msg) => {
    console.log("[nvim-web] Received from host:", msg);
  });

  nativePort.onDisconnect.addListener(() => {
    console.log("[nvim-web] Disconnected from host:", chrome.runtime.lastError);
    nativePort = null;
    // Optional: Auto-reconnect after a delay if desired
    // setTimeout(connectToNativeHost, 5000);
  });
}

// Connect on startup
connectToNativeHost();

// Listen for messages from popup (optional, if we want to force reconnect)
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.type === "reconnect") {
    if (!nativePort) {
      connectToNativeHost();
      sendResponse({ status: "connecting" });
    } else {
      sendResponse({ status: "connected" });
    }
  }
});
