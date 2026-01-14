# TCP Authentication Protocol
 
## Overview
 
This document specifies the authentication protocol used by nvim-web for secure
TCP connections to remote Neovim instances. The protocol ensures that only
authorized clients possessing a shared secret can establish a connection to the
RPC endpoint.
 
The protocol uses a Challenge-Response mechanism based on HMAC-SHA256 to prove
possession of the secret token without transmitting it over the wire. This provides
protection against replay attacks and eavesdropping (for the token itself, though
TLS is recommended for data confidentiality).
 
## Handshake Flow
 
The handshake occurs immediately upon establishing the TCP connection, before any
Neovim RPC messages are exchanged.
 
    Client                                    Server
    ======                                    ======
 
    1. Connect (TCP SYN/ACK)  -------------->
                                              <Generate 32-byte random Nonce>
 
    2. <Wait for Nonce>       <-------------- Send Nonce (32 bytes)
    
       Compute HMAC-SHA256
       (Nonce + Token)
 
    3. Send HMAC (32 bytes)   --------------> verifies HMAC
    
    4. <Authorized?>          <-------------- (Implicit)
                                              If valid: Connection stays open
                                              If invalid: Connection closed (EOF)
 
## Message Format
 
### 1. Challenge (Server -> Client)
 
| Offset | Length | Type | Description |
|--------|--------|------|-------------|
| 0      | 32     | u8[] | Cryptographically secure random nonce |
 
The server MUST generate a fresh, unique nonce for every connection attempt using a
CSPRNG (Cryptographically Secure Pseudo-Random Number Generator).
 
### 2. Response (Client -> Server)
 
| Offset | Length | Type | Description |
|--------|--------|------|-------------|
| 0      | 32     | u8[] | HMAC-SHA256(Key=Token, Message=Nonce) |
 
The client uses the shared secret `Token` as the key and the received `Nonce` as
the message to compute a standard HMAC-SHA256 signature.
 
## Security Guarantees
 
1. **Token Confidentiality**: The token is never sent over the wire. An attacker
   observing the handshake sees only the random nonce and the resulting HMAC.
 
2. **Replay Protection**: Since the nonce is random and unique per connection, 
   capturing a valid HMAC response is useless for future connections (unless the
   attacker can predict the nonce, which is prevented by using a CSPRNG).
 
3. **Timing Attack Resistance**: The server MUST verify the received HMAC using
   a constant-time comparison algorithm to prevent side-channel attacks.
 
## Implementation Details
 
- **Nonces**: 32 bytes (256 bits)
- **Hash Algorithm**: SHA-256
- **MAC Algorithm**: HMAC (RFC 2104)
- **Token Storage**: Tokens should be stored in files with restricted permissions
  (e.g., `0600` on Unix) to prevent local compromise.
 
## Error Handling
 
If authentication fails:
1. The server MUST NOT send any error message or diagnostic info.
2. The server MUST immediately close the TCP connection (send FIN/RST).
3. The client interprets an immediate EOF as specific authentication failure.
 
## Backward Compatibility
 
This protocol is not backward compatible with raw unauthenticated Neovim TCP
endpoints if they expect immediate RPC messages. However, since unauthenticated
TCP is insecure (Issue #4443), this incompatibility is a purposeful security
boundary.
