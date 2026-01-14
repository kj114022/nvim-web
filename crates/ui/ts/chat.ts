/**
 * Chat Panel UI Component
 * 
 * Renders a collapsible chat panel for P2P messaging.
 */

import { P2PMesh, type ChatMessage } from './p2p';

interface ChatUI {
  container: HTMLElement;
  messagesDiv: HTMLElement;
  input: HTMLInputElement;
  sendBtn: HTMLButtonElement;
  mesh: P2PMesh | null;
}

let chatUI: ChatUI | null = null;

/**
 * Initialize chat panel
 */
export function initChatPanel(
  localId: string,
  signalCallback: (to: string, type: string, payload: string) => void
): void {
  if (chatUI) return;

  // Create container
  const container = document.createElement('div');
  container.id = 'chat-panel';
  container.innerHTML = `
    <style>
      #chat-panel {
        position: fixed;
        bottom: 20px;
        right: 20px;
        width: 320px;
        max-height: 400px;
        background: #1e1e2e;
        border: 1px solid #45475a;
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0,0,0,0.3);
        display: flex;
        flex-direction: column;
        font-family: system-ui, -apple-system, sans-serif;
        z-index: 10000;
        transition: transform 0.2s ease;
      }
      #chat-panel.collapsed {
        transform: translateY(calc(100% - 40px));
      }
      #chat-header {
        padding: 10px 14px;
        background: #313244;
        border-radius: 8px 8px 0 0;
        cursor: pointer;
        display: flex;
        justify-content: space-between;
        align-items: center;
        color: #cdd6f4;
        font-weight: 500;
      }
      #chat-header .badge {
        background: #89b4fa;
        color: #1e1e2e;
        padding: 2px 8px;
        border-radius: 10px;
        font-size: 12px;
      }
      #chat-messages {
        flex: 1;
        overflow-y: auto;
        padding: 10px;
        min-height: 200px;
        max-height: 300px;
      }
      .chat-msg {
        margin-bottom: 8px;
        padding: 8px 10px;
        border-radius: 6px;
        background: #313244;
        color: #cdd6f4;
        font-size: 13px;
      }
      .chat-msg .sender {
        font-weight: 600;
        color: #89b4fa;
        margin-right: 6px;
      }
      .chat-msg .time {
        color: #6c7086;
        font-size: 11px;
        float: right;
      }
      .chat-msg.self {
        background: #45475a;
      }
      #chat-input-row {
        display: flex;
        padding: 10px;
        border-top: 1px solid #45475a;
      }
      #chat-input {
        flex: 1;
        background: #313244;
        border: 1px solid #45475a;
        border-radius: 4px;
        padding: 8px 10px;
        color: #cdd6f4;
        outline: none;
      }
      #chat-input:focus {
        border-color: #89b4fa;
      }
      #chat-send {
        margin-left: 8px;
        background: #89b4fa;
        color: #1e1e2e;
        border: none;
        border-radius: 4px;
        padding: 8px 14px;
        cursor: pointer;
        font-weight: 500;
      }
      #chat-send:hover {
        background: #b4befe;
      }
    </style>
    <div id="chat-header">
      <span>Chat</span>
      <span class="badge" id="peer-count">0 peers</span>
    </div>
    <div id="chat-messages"></div>
    <div id="chat-input-row">
      <input type="text" id="chat-input" placeholder="Type a message..." />
      <button id="chat-send">Send</button>
    </div>
  `;

  document.body.appendChild(container);

  const messagesDiv = container.querySelector('#chat-messages') as HTMLElement;
  const input = container.querySelector('#chat-input') as HTMLInputElement;
  const sendBtn = container.querySelector('#chat-send') as HTMLButtonElement;
  const header = container.querySelector('#chat-header') as HTMLElement;

  // Toggle collapse
  header.addEventListener('click', () => {
    container.classList.toggle('collapsed');
  });

  // Create P2P mesh
  const mesh = new P2PMesh(localId, (msg) => {
    addMessage(msg, msg.from === localId);
  }, signalCallback);

  // Send message
  const sendMessage = () => {
    const text = input.value.trim();
    if (!text) return;
    
    mesh.sendMessage(text);
    addMessage({ from: localId, message: text, timestamp: Date.now() }, true);
    input.value = '';
  };

  sendBtn.addEventListener('click', sendMessage);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') sendMessage();
  });

  chatUI = { container, messagesDiv, input, sendBtn, mesh };
}

/**
 * Add message to chat
 */
function addMessage(msg: ChatMessage, isSelf: boolean): void {
  if (!chatUI) return;

  const time = new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const div = document.createElement('div');
  div.className = `chat-msg${isSelf ? ' self' : ''}`;
  div.innerHTML = `<span class="sender">${escapeHtml(msg.from)}</span><span class="time">${time}</span><br>${escapeHtml(msg.message)}`;
  
  chatUI.messagesDiv.appendChild(div);
  chatUI.messagesDiv.scrollTop = chatUI.messagesDiv.scrollHeight;
}

/**
 * Handle incoming WebRTC signal
 */
export function handleSignal(from: string, type: string, payload: string): void {
  chatUI?.mesh?.handleSignal(from, type, payload);
}

/**
 * Connect to a peer
 */
export function connectPeer(peerId: string): void {
  chatUI?.mesh?.connectToPeer(peerId);
  updatePeerCount();
}

/**
 * Update peer count badge
 */
function updatePeerCount(): void {
  if (!chatUI) return;
  const count = chatUI.mesh?.getConnectedPeers().length || 0;
  const badge = chatUI.container.querySelector('#peer-count');
  if (badge) badge.textContent = `${count} peer${count !== 1 ? 's' : ''}`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
