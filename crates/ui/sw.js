// Service Worker for nvim-web PWA
// Enhanced with offline mode, background sync, and stale-while-revalidate

const CACHE_NAME = 'nvim-web-v2';
const OFFLINE_CACHE = 'nvim-web-offline-v1';

// Core files to cache for offline use
const PRECACHE_URLS = [
  '/',
  '/index.html',
  '/manifest.json',
  '/pkg/nvim_web_ui.js',
  '/pkg/nvim_web_ui_bg.wasm',
  '/fs/fs_driver.js',
];

// Install - cache core files
self.addEventListener('install', (event) => {
  console.log('[SW] Installing service worker v2...');
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      console.log('[SW] Caching core assets');
      return cache.addAll(PRECACHE_URLS);
    }).then(() => self.skipWaiting())
  );
});

// Activate - clean old caches
self.addEventListener('activate', (event) => {
  console.log('[SW] Activating service worker v2...');
  event.waitUntil(
    caches.keys().then((cacheNames) => {
      return Promise.all(
        cacheNames
          .filter((name) => name !== CACHE_NAME && name !== OFFLINE_CACHE)
          .map((name) => {
            console.log('[SW] Deleting old cache:', name);
            return caches.delete(name);
          })
      );
    }).then(() => self.clients.claim())
  );
});

// Fetch - stale-while-revalidate strategy
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  
  // Skip WebSocket and non-GET requests
  if (event.request.method !== 'GET') return;
  if (url.protocol === 'ws:' || url.protocol === 'wss:') return;
  
  event.respondWith(
    caches.match(event.request).then((cachedResponse) => {
      // Return cached version immediately (stale)
      const fetchPromise = fetch(event.request)
        .then((networkResponse) => {
          // Update cache with fresh response (revalidate)
          if (networkResponse && networkResponse.status === 200) {
            const responseClone = networkResponse.clone();
            caches.open(CACHE_NAME).then((cache) => {
              cache.put(event.request, responseClone);
            });
          }
          return networkResponse;
        })
        .catch(() => {
          // Network failed - return cached or offline fallback
          if (cachedResponse) return cachedResponse;
          if (event.request.mode === 'navigate') {
            return caches.match('/');
          }
          return null;
        });

      return cachedResponse || fetchPromise;
    })
  );
});

// Handle messages from main thread
self.addEventListener('message', (event) => {
  const { type, payload } = event.data || {};
  
  switch (type) {
    case 'SKIP_WAITING':
      self.skipWaiting();
      break;
      
    case 'CACHE_URLS':
      caches.open(CACHE_NAME).then((cache) => cache.addAll(payload.urls));
      break;
      
    case 'GET_VERSION':
      event.source.postMessage({ type: 'VERSION', version: CACHE_NAME });
      break;
      
    case 'IS_ONLINE':
      // Respond with online status
      fetch('/').then(() => {
        event.source.postMessage({ type: 'ONLINE_STATUS', online: true });
      }).catch(() => {
        event.source.postMessage({ type: 'ONLINE_STATUS', online: false });
      });
      break;
  }
});

// Background sync for pending writes
self.addEventListener('sync', (event) => {
  if (event.tag === 'sync-pending-writes') {
    event.waitUntil(notifyClientsToSync());
  }
});

// Notify all clients to sync
async function notifyClientsToSync() {
  const clients = await self.clients.matchAll();
  clients.forEach((client) => {
    client.postMessage({ type: 'SYNC_WRITES' });
  });
}

// Push notification support (future)
self.addEventListener('push', (event) => {
  if (event.data) {
    const data = event.data.json();
    self.registration.showNotification(data.title || 'nvim-web', {
      body: data.body,
      icon: '/icons/icon-192.png',
    });
  }
});

console.log('[SW] Service worker v2 loaded');
