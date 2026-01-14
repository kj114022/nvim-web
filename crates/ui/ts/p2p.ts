/**
 * P2P Mesh for Browser-to-Browser Communication
 * 
 * Uses WebRTC data channels with DTLS encryption for E2EE.
 * Server acts only as signaling relay.
 */

interface PeerConnection {
  pc: RTCPeerConnection;
  dc?: RTCDataChannel;
  peerId: string;
}

interface ChatMessage {
  from: string;
  to?: string;
  message: string;
  timestamp: number;
}

type MessageHandler = (msg: ChatMessage) => void;

const ICE_SERVERS: RTCIceServer[] = [
  { urls: 'stun:stun.l.google.com:19302' },
  { urls: 'stun:stun1.l.google.com:19302' },
];

export class P2PMesh {
  private peers: Map<string, PeerConnection> = new Map();
  private localId: string;
  private onMessage: MessageHandler;
  private signalCallback: (to: string, type: string, payload: string) => void;

  constructor(
    localId: string,
    onMessage: MessageHandler,
    signalCallback: (to: string, type: string, payload: string) => void
  ) {
    this.localId = localId;
    this.onMessage = onMessage;
    this.signalCallback = signalCallback;
  }

  /**
   * Connect to a new peer (as initiator)
   */
  async connectToPeer(peerId: string): Promise<void> {
    if (this.peers.has(peerId)) return;

    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });
    const peerConn: PeerConnection = { pc, peerId };
    this.peers.set(peerId, peerConn);

    // Create data channel for chat
    const dc = pc.createDataChannel('chat', { ordered: true });
    peerConn.dc = dc;
    this.setupDataChannel(dc, peerId);

    // Handle ICE candidates
    pc.onicecandidate = (e) => {
      if (e.candidate) {
        this.signalCallback(peerId, 'ice', JSON.stringify(e.candidate));
      }
    };

    // Create and send offer
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    this.signalCallback(peerId, 'offer', JSON.stringify(offer));
  }

  /**
   * Handle incoming signaling message
   */
  async handleSignal(from: string, type: string, payload: string): Promise<void> {
    let peerConn = this.peers.get(from);

    if (type === 'offer') {
      // Received offer - we're the responder
      if (!peerConn) {
        const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });
        peerConn = { pc, peerId: from };
        this.peers.set(from, peerConn);

        pc.ondatachannel = (e) => {
          peerConn!.dc = e.channel;
          this.setupDataChannel(e.channel, from);
        };

        pc.onicecandidate = (e) => {
          if (e.candidate) {
            this.signalCallback(from, 'ice', JSON.stringify(e.candidate));
          }
        };
      }

      const offer = JSON.parse(payload) as RTCSessionDescriptionInit;
      await peerConn.pc.setRemoteDescription(offer);
      const answer = await peerConn.pc.createAnswer();
      await peerConn.pc.setLocalDescription(answer);
      this.signalCallback(from, 'answer', JSON.stringify(answer));

    } else if (type === 'answer' && peerConn) {
      const answer = JSON.parse(payload) as RTCSessionDescriptionInit;
      await peerConn.pc.setRemoteDescription(answer);

    } else if (type === 'ice' && peerConn) {
      const candidate = JSON.parse(payload) as RTCIceCandidateInit;
      await peerConn.pc.addIceCandidate(candidate);
    }
  }

  /**
   * Send chat message to specific peer or broadcast
   */
  sendMessage(message: string, to?: string): void {
    const msg: ChatMessage = {
      from: this.localId,
      to,
      message,
      timestamp: Date.now(),
    };

    if (to) {
      // Unicast
      const peerConn = this.peers.get(to);
      if (peerConn?.dc?.readyState === 'open') {
        peerConn.dc.send(JSON.stringify(msg));
      }
    } else {
      // Broadcast to all peers
      this.peers.forEach((peerConn) => {
        if (peerConn.dc?.readyState === 'open') {
          peerConn.dc.send(JSON.stringify(msg));
        }
      });
    }
  }

  /**
   * Disconnect from a peer
   */
  disconnect(peerId: string): void {
    const peerConn = this.peers.get(peerId);
    if (peerConn) {
      peerConn.dc?.close();
      peerConn.pc.close();
      this.peers.delete(peerId);
    }
  }

  /**
   * Disconnect from all peers
   */
  disconnectAll(): void {
    this.peers.forEach((_, peerId) => this.disconnect(peerId));
  }

  /**
   * Get connected peer IDs
   */
  getConnectedPeers(): string[] {
    return Array.from(this.peers.entries())
      .filter(([_, p]) => p.dc?.readyState === 'open')
      .map(([id]) => id);
  }

  private setupDataChannel(dc: RTCDataChannel, peerId: string): void {
    dc.onopen = () => {
      console.log(`[P2P] Data channel open with ${peerId}`);
    };

    dc.onclose = () => {
      console.log(`[P2P] Data channel closed with ${peerId}`);
    };

    dc.onerror = (e) => {
      console.error(`[P2P] Data channel error with ${peerId}:`, e);
    };

    dc.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data) as ChatMessage;
        this.onMessage(msg);
      } catch (err) {
        console.error('[P2P] Failed to parse message:', err);
      }
    };
  }
}
