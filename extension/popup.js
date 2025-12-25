// nvim-web Browser Extension

const DEFAULT_URL = 'http://localhost:8080';
const WS_URL = 'ws://127.0.0.1:9001';

// DOM elements
const statusEl = document.getElementById('status');
const openBtn = document.getElementById('open');
const newSessionBtn = document.getElementById('newSession');
const serverUrlInput = document.getElementById('serverUrl');
const saveBtn = document.getElementById('save');

// Load saved settings
chrome.storage.sync.get(['serverUrl'], (result) => {
  serverUrlInput.value = result.serverUrl || DEFAULT_URL;
});

// Check connection status
async function checkStatus() {
  try {
    const response = await fetch(serverUrlInput.value || DEFAULT_URL, {
      method: 'HEAD',
      mode: 'no-cors'
    });
    statusEl.textContent = 'Connected to nvim-web';
    statusEl.className = 'status connected';
  } catch (e) {
    statusEl.textContent = 'Server not running';
    statusEl.className = 'status disconnected';
  }
}

// Open nvim-web
openBtn.addEventListener('click', () => {
  const url = serverUrlInput.value || DEFAULT_URL;
  chrome.tabs.create({ url });
});

// New session
newSessionBtn.addEventListener('click', () => {
  const url = (serverUrlInput.value || DEFAULT_URL) + '?session=new';
  chrome.tabs.create({ url });
});

// Save settings
saveBtn.addEventListener('click', () => {
  chrome.storage.sync.set({ 
    serverUrl: serverUrlInput.value 
  }, () => {
    saveBtn.textContent = 'Saved!';
    setTimeout(() => {
      saveBtn.textContent = 'Save Settings';
    }, 1000);
    checkStatus();
  });
});

// Initial status check
checkStatus();
