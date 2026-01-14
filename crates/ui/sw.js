// Service Worker for nvim-web PWA
// Enhanced with offline mode, background sync, and stale-while-revalidate
/// <reference lib="webworker" />
// ServiceWorker global scope typing
const sw = self;
const CACHE_VERSION = "3";
const CACHE_NAME = `nvim-web-v${CACHE_VERSION}`;
const OFFLINE_CACHE = "nvim-web-offline-v1";
// Core files to cache for offline use
const PRECACHE_URLS = [
    "/",
    "/index.html",
    "/manifest.json",
    "/pkg/nvim_web_ui.js",
    "/pkg/nvim_web_ui_bg.wasm",
    "/fs/fs_driver.js",
    "/fs/session_storage.js",
];
// Install - cache core files
sw.addEventListener("install", (event) => {
    console.log(`[SW] Installing service worker v${CACHE_VERSION}...`);
    event.waitUntil(caches
        .open(CACHE_NAME)
        .then((cache) => {
        console.log("[SW] Caching core assets");
        return cache.addAll(PRECACHE_URLS);
    })
        .then(() => sw.skipWaiting()));
});
// Activate - clean old caches
sw.addEventListener("activate", (event) => {
    console.log(`[SW] Activating service worker v${CACHE_VERSION}...`);
    event.waitUntil(caches
        .keys()
        .then((cacheNames) => {
        return Promise.all(cacheNames
            .filter((name) => name.startsWith("nvim-web-") &&
            name !== CACHE_NAME &&
            name !== OFFLINE_CACHE)
            .map((name) => {
            console.log("[SW] Deleting old cache:", name);
            return caches.delete(name);
        }));
    })
        .then(() => sw.clients.claim()));
});
// Fetch - stale-while-revalidate strategy
sw.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    // Skip WebSocket and non-GET requests
    if (event.request.method !== "GET")
        return;
    if (url.protocol === "ws:" || url.protocol === "wss:")
        return;
    event.respondWith(caches.match(event.request).then((cachedResponse) => {
        // Return cached version immediately (stale)
        const fetchPromise = fetch(event.request)
            .then((networkResponse) => {
            // Update cache with fresh response (revalidate)
            if (networkResponse && networkResponse.status === 200) {
                const responseClone = networkResponse.clone();
                caches.open(CACHE_NAME).then((cache) => {
                    void cache.put(event.request, responseClone);
                });
            }
            return networkResponse;
        })
            .catch(() => {
            // Network failed - return cached or offline fallback
            if (cachedResponse)
                return cachedResponse;
            if (event.request.mode === "navigate") {
                return caches.match("/");
            }
            return new Response(null, { status: 503 });
        });
        return cachedResponse || fetchPromise;
    }));
});
// Handle messages from main thread
sw.addEventListener("message", (event) => {
    const data = event.data;
    if (!data)
        return;
    const { type, payload } = data;
    switch (type) {
        case "SKIP_WAITING":
            void sw.skipWaiting();
            break;
        case "CACHE_URLS":
            if (payload?.urls) {
                void caches.open(CACHE_NAME).then((cache) => cache.addAll(payload.urls));
            }
            break;
        case "GET_VERSION":
            event.source?.postMessage({ type: "VERSION", version: CACHE_VERSION });
            break;
        case "IS_ONLINE":
            fetch("/")
                .then(() => {
                event.source?.postMessage({ type: "ONLINE_STATUS", online: true });
            })
                .catch(() => {
                event.source?.postMessage({ type: "ONLINE_STATUS", online: false });
            });
            break;
    }
});
sw.addEventListener("sync", (event) => {
    const syncEvent = event;
    if (syncEvent.tag === "sync-pending-writes") {
        syncEvent.waitUntil(notifyClientsToSync());
    }
});
// Notify all clients to sync
async function notifyClientsToSync() {
    const allClients = await sw.clients.matchAll();
    allClients.forEach((client) => {
        client.postMessage({ type: "SYNC_WRITES" });
    });
}
// Push notification support (future)
sw.addEventListener("push", (event) => {
    if (event.data) {
        const data = event.data.json();
        void sw.registration.showNotification(data.title || "nvim-web", {
            body: data.body,
            icon: "/icons/icon-192.png",
        });
    }
});
console.log(`[SW] Service worker v${CACHE_VERSION} loaded`);
export {};
