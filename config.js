// nvim-web client configuration
// This file is loaded by the browser before the WASM module initializes
window.NVIM_CONFIG = {
    // WebSocket configuration
    wsPort: 9001,
    wsPath: "/ws",

    // WebTransport configuration (if available)
    webtransportPort: 9002,
    webtransportEnabled: true,

    // Session settings
    sessionTimeout: 300000, // 5 minutes in ms
    reconnectDelay: 1000,
    maxReconnectAttempts: 5,

    // Collaboration settings
    collaborationEnabled: true,
    crdtSyncInterval: 100, // ms
    cursorBroadcastInterval: 50, // ms

    // UI settings
    fontSize: 14,
    fontFamily: "JetBrains Mono, Fira Code, Monaco, Consolas, monospace",
    lineHeight: 1.2,
    cursorBlink: true,
    smoothScroll: true,

    // Performance
    renderThrottle: 16, // ~60fps
    inputDebounce: 10, // ms
};
