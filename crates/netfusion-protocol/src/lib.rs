pub mod scheduler;

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use x25519_dalek::{PublicKey, StaticSecret};

pub const PROTOCOL_MAGIC: &[u8; 4] = b"NFB1";
pub const MAX_FRAME_LEN: usize = 4 * 1024 * 1024; // 4 MiB hard cap
pub const DEFAULT_CHUNK_SIZE: usize = 32 * 1024; // 32 KiB multipath striping unit

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid protocol magic or malformed handshake")]
    InvalidHandshake,
    #[error("Authentication failed: invalid pre-shared key or replay detected")]
    AuthenticationFailed,
    #[error("AEAD encryption/decryption integrity check failed")]
    CryptoIntegrityError,
    #[error("Frame exceeds maximum size ({0} bytes)")]
    FrameTooLarge(usize),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHello {
    pub session_id: [u8; 16],
    pub subflow_id: u32,
    pub interface_name: String,
    pub client_pub: [u8; 32],
    pub client_nonce: [u8; 16],
    pub timestamp_ms: u64,
    pub auth_hmac: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    pub accepted: bool,
    pub message: String,
    pub server_pub: [u8; 32],
    pub server_nonce: [u8; 16],
    pub server_verify_tag: [u8; 32],
}

/// Encrypted multipath tunnel frames exchanged after X25519 + HKDF + HMAC mutual auth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BondingFrame {
    Ping {
        seq: u64,
        sent_ms: u64,
    },
    Pong {
        seq: u64,
        sent_ms: u64,
    },
    SubflowTelemetry {
        subflow_id: u32,
        throughput_bps: u64,
        link_capacity_bps: u64,
        rtt_ms: u32,
        loss_permille: u16,
        is_primary: bool,
        custom_weight: u32,
    },
    SetPolicy {
        policy: scheduler::BondingPolicy,
    },
    TcpOpen {
        stream_id: u32,
        target_host: String,
        target_port: u16,
    },
    TcpOpenAck {
        stream_id: u32,
        ok: bool,
        error: Option<String>,
    },
    StreamChunk {
        stream_id: u32,
        offset: u64,
        eof: bool,
        #[serde(with = "serde_bytes_vec")]
        data: Vec<u8>,
    },
    StreamAck {
        stream_id: u32,
        acked_offset: u64,
    },
    StreamClose {
        stream_id: u32,
        reason: String,
    },
    SpeedTestDownRequest {
        stream_id: u32,
        total_bytes: u64,
    },
    SpeedTestUpChunk {
        stream_id: u32,
        offset: u64,
        eof: bool,
        #[serde(with = "serde_bytes_vec")]
        data: Vec<u8>,
    },
    SpeedTestUpResult {
        stream_id: u32,
        total_bytes: u64,
        elapsed_ms: u64,
    },
}

/// Compact binary encoding for `BondingFrame` so `StreamChunk` payloads are NOT bloated by JSON arrays!
impl BondingFrame {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        match self {
            BondingFrame::StreamChunk {
                stream_id,
                offset,
                eof,
                data,
            } => {
                // Fast-path zero-bloat binary layout: [0x01][stream_id:4][offset:8][eof:1][data...]
                let mut buf = Vec::with_capacity(14 + data.len());
                buf.push(0x01);
                buf.extend_from_slice(&stream_id.to_be_bytes());
                buf.extend_from_slice(&offset.to_be_bytes());
                buf.push(if *eof { 1 } else { 0 });
                buf.extend_from_slice(data);
                Ok(buf)
            }
            BondingFrame::SpeedTestUpChunk {
                stream_id,
                offset,
                eof,
                data,
            } => {
                let mut buf = Vec::with_capacity(14 + data.len());
                buf.push(0x02);
                buf.extend_from_slice(&stream_id.to_be_bytes());
                buf.extend_from_slice(&offset.to_be_bytes());
                buf.push(if *eof { 1 } else { 0 });
                buf.extend_from_slice(data);
                Ok(buf)
            }
            other => {
                let json = serde_json::to_vec(other)
                    .map_err(|e| ProtocolError::Serialization(e.to_string()))?;
                let mut buf = Vec::with_capacity(1 + json.len());
                buf.push(0x00);
                buf.extend_from_slice(&json);
                Ok(buf)
            }
        }
    }

    pub fn decode(raw: &[u8]) -> Result<Self, ProtocolError> {
        if raw.is_empty() {
            return Err(ProtocolError::Serialization("empty frame".into()));
        }
        match raw[0] {
            0x01 => {
                if raw.len() < 14 {
                    return Err(ProtocolError::Serialization("truncated StreamChunk".into()));
                }
                let stream_id = u32::from_be_bytes(raw[1..5].try_into().unwrap());
                let offset = u64::from_be_bytes(raw[5..13].try_into().unwrap());
                let eof = raw[13] != 0;
                let data = raw[14..].to_vec();
                Ok(BondingFrame::StreamChunk {
                    stream_id,
                    offset,
                    eof,
                    data,
                })
            }
            0x02 => {
                if raw.len() < 14 {
                    return Err(ProtocolError::Serialization(
                        "truncated SpeedTestUpChunk".into(),
                    ));
                }
                let stream_id = u32::from_be_bytes(raw[1..5].try_into().unwrap());
                let offset = u64::from_be_bytes(raw[5..13].try_into().unwrap());
                let eof = raw[13] != 0;
                let data = raw[14..].to_vec();
                Ok(BondingFrame::SpeedTestUpChunk {
                    stream_id,
                    offset,
                    eof,
                    data,
                })
            }
            0x00 => serde_json::from_slice(&raw[1..])
                .map_err(|e| ProtocolError::Serialization(e.to_string())),
            tag => Err(ProtocolError::Serialization(format!(
                "unknown frame tag {tag}"
            ))),
        }
    }
}

mod serde_bytes_vec {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let v: Vec<u8> = serde::Deserialize::deserialize(d)?;
        Ok(v)
    }
}

fn psk_digest(psk: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"NETFUSION-PSK-V1:");
    hasher.update(psk.as_bytes());
    hasher.finalize().into()
}

fn compute_client_hmac(
    psk_hash: &[u8; 32],
    session_id: &[u8; 16],
    subflow_id: u32,
    client_pub: &[u8; 32],
    client_nonce: &[u8; 16],
    timestamp_ms: u64,
) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(psk_hash).expect("HMAC key size valid");
    mac.update(b"NFB1-CLIENT-AUTH");
    mac.update(session_id);
    mac.update(&subflow_id.to_be_bytes());
    mac.update(client_pub);
    mac.update(client_nonce);
    mac.update(&timestamp_ms.to_be_bytes());
    mac.finalize().into_bytes().into()
}

fn derive_session_keys(
    shared_secret: &[u8; 32],
    psk_hash: &[u8; 32],
    client_nonce: &[u8; 16],
    server_nonce: &[u8; 16],
) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let mut salt = [0u8; 32];
    salt[..16].copy_from_slice(client_nonce);
    salt[16..].copy_from_slice(server_nonce);

    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(shared_secret);
    ikm[32..].copy_from_slice(psk_hash);

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut c2s_key = [0u8; 32];
    let mut s2c_key = [0u8; 32];
    let mut verify_tag = [0u8; 32];
    hk.expand(b"NFB1-C2S-AEAD", &mut c2s_key).unwrap();
    hk.expand(b"NFB1-S2C-AEAD", &mut s2c_key).unwrap();
    hk.expand(b"NFB1-SERVER-VERIFY", &mut verify_tag).unwrap();
    (c2s_key, s2c_key, verify_tag)
}

pub struct CipherHalf {
    cipher: ChaCha20Poly1305,
    dir_byte: u8,
    seq: u64,
}

impl CipherHalf {
    fn new(key_bytes: &[u8; 32], dir_byte: u8) -> Self {
        let key = Key::from_slice(key_bytes);
        Self {
            cipher: ChaCha20Poly1305::new(key),
            dir_byte,
            seq: 0,
        }
    }

    fn next_nonce(&mut self) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[3] = self.dir_byte;
        n[4..12].copy_from_slice(&self.seq.to_be_bytes());
        self.seq = self.seq.wrapping_add(1);
        n
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        let nonce_bytes = self.next_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| ProtocolError::CryptoIntegrityError)
    }

    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        let nonce_bytes = self.next_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| ProtocolError::CryptoIntegrityError)
    }
}

pub async fn write_encrypted_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    sealer: &mut CipherHalf,
    frame: &BondingFrame,
) -> Result<usize, ProtocolError> {
    let encoded = frame.encode()?;
    if encoded.len() > MAX_FRAME_LEN {
        return Err(ProtocolError::FrameTooLarge(encoded.len()));
    }
    let ciphertext = sealer.seal(&encoded)?;
    let len = ciphertext.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&ciphertext).await?;
    writer.flush().await?;
    Ok(4 + ciphertext.len())
}

pub async fn read_encrypted_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    opener: &mut CipherHalf,
) -> Result<BondingFrame, ProtocolError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME_LEN + 64 {
        return Err(ProtocolError::FrameTooLarge(len));
    }
    let mut ciphertext = vec![0u8; len];
    reader.read_exact(&mut ciphertext).await?;
    let plaintext = opener.open(&ciphertext)?;
    BondingFrame::decode(&plaintext)
}

/// Performs client-side mutually authenticated X25519 + ChaCha20-Poly1305 handshake.
pub async fn client_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    session_id: [u8; 16],
    subflow_id: u32,
    interface_name: String,
    psk: &str,
) -> Result<(CipherHalf, CipherHalf), ProtocolError> {
    let psk_hash = psk_digest(psk);

    let (client_secret, client_pub, client_nonce) = {
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let pub_key: [u8; 32] = PublicKey::from(&secret).to_bytes();

        let mut nonce = [0u8; 16];
        rng.fill_bytes(&mut nonce);
        (secret, pub_key, nonce)
    };

    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let auth_hmac = compute_client_hmac(
        &psk_hash,
        &session_id,
        subflow_id,
        &client_pub,
        &client_nonce,
        timestamp_ms,
    );

    let hello = ClientHello {
        session_id,
        subflow_id,
        interface_name,
        client_pub,
        client_nonce,
        timestamp_ms,
        auth_hmac,
    };

    let payload =
        serde_json::to_vec(&hello).map_err(|e| ProtocolError::Serialization(e.to_string()))?;
    stream.write_all(PROTOCOL_MAGIC).await?;
    stream.write_all(&(payload.len() as u32).to_be_bytes()).await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;

    let mut magic = [0u8; 4];
    stream.read_exact(&mut magic).await?;
    if &magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::InvalidHandshake);
    }
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    if resp_len > 16384 {
        return Err(ProtocolError::InvalidHandshake);
    }
    let mut resp_buf = vec![0u8; resp_len];
    stream.read_exact(&mut resp_buf).await?;

    let server_hello: ServerHello =
        serde_json::from_slice(&resp_buf).map_err(|_| ProtocolError::InvalidHandshake)?;
    if !server_hello.accepted {
        return Err(ProtocolError::AuthenticationFailed);
    }

    let server_pub = PublicKey::from(server_hello.server_pub);
    let shared = client_secret.diffie_hellman(&server_pub).to_bytes();
    let (c2s_key, s2c_key, expected_verify_tag) =
        derive_session_keys(&shared, &psk_hash, &client_nonce, &server_hello.server_nonce);

    if server_hello.server_verify_tag != expected_verify_tag {
        return Err(ProtocolError::AuthenticationFailed);
    }

    Ok((CipherHalf::new(&c2s_key, 0x01), CipherHalf::new(&s2c_key, 0x02)))
}

#[derive(Debug, Clone)]
pub struct VerifiedSubflowMeta {
    pub session_id: [u8; 16],
    pub subflow_id: u32,
    pub interface_name: String,
}

/// Performs server-side mutually authenticated X25519 + ChaCha20-Poly1305 handshake.
pub async fn server_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    psk: &str,
) -> Result<(VerifiedSubflowMeta, CipherHalf, CipherHalf), ProtocolError> {
    let mut magic = [0u8; 4];
    stream.read_exact(&mut magic).await?;
    if &magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::InvalidHandshake);
    }
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > 16384 {
        return Err(ProtocolError::InvalidHandshake);
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let hello: ClientHello =
        serde_json::from_slice(&buf).map_err(|_| ProtocolError::InvalidHandshake)?;

    let psk_hash = psk_digest(psk);
    let expected_hmac = compute_client_hmac(
        &psk_hash,
        &hello.session_id,
        hello.subflow_id,
        &hello.client_pub,
        &hello.client_nonce,
        hello.timestamp_ms,
    );

    if hello.auth_hmac != expected_hmac {
        let reject = ServerHello {
            accepted: false,
            message: "Authentication failed".into(),
            server_pub: [0u8; 32],
            server_nonce: [0u8; 16],
            server_verify_tag: [0u8; 32],
        };
        if let Ok(rej_bytes) = serde_json::to_vec(&reject) {
            let _ = stream.write_all(PROTOCOL_MAGIC).await;
            let _ = stream
                .write_all(&(rej_bytes.len() as u32).to_be_bytes())
                .await;
            let _ = stream.write_all(&rej_bytes).await;
            let _ = stream.flush().await;
        }
        return Err(ProtocolError::AuthenticationFailed);
    }

    let (server_secret, server_pub, server_nonce) = {
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let pub_key: [u8; 32] = PublicKey::from(&secret).to_bytes();

        let mut nonce = [0u8; 16];
        rng.fill_bytes(&mut nonce);
        (secret, pub_key, nonce)
    };

    let client_pub = PublicKey::from(hello.client_pub);
    let shared = server_secret.diffie_hellman(&client_pub).to_bytes();
    let (c2s_key, s2c_key, verify_tag) =
        derive_session_keys(&shared, &psk_hash, &hello.client_nonce, &server_nonce);

    let accept = ServerHello {
        accepted: true,
        message: "Authenticated NetFusion Bonding Subflow".into(),
        server_pub,
        server_nonce,
        server_verify_tag: verify_tag,
    };
    let resp_bytes =
        serde_json::to_vec(&accept).map_err(|e| ProtocolError::Serialization(e.to_string()))?;
    stream.write_all(PROTOCOL_MAGIC).await?;
    stream
        .write_all(&(resp_bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&resp_bytes).await?;
    stream.flush().await?;

    let meta = VerifiedSubflowMeta {
        session_id: hello.session_id,
        subflow_id: hello.subflow_id,
        interface_name: hello.interface_name,
    };
    // Server reads C2S (0x01) and writes S2C (0x02)
    Ok((meta, CipherHalf::new(&s2c_key, 0x02), CipherHalf::new(&c2s_key, 0x01)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{
        compute_subflow_weights, BondingPolicy, MultipathScheduler, StreamReassembler,
        SubflowDescriptor,
    };

    #[tokio::test]
    async fn test_handshake_and_encrypted_frames() {
        let (mut client_io, mut server_io) = tokio::io::duplex(65536);
        let session_id = [7u8; 16];
        let psk = "super-secret-bonding-key-2026";

        let c_fut = client_handshake(&mut client_io, session_id, 11, "Wi-Fi".into(), psk);
        let s_fut = server_handshake(&mut server_io, psk);
        let ((mut c_write, mut c_read), (meta, mut s_write, mut s_read)) =
            tokio::try_join!(c_fut, s_fut).expect("handshake succeeds");

        assert_eq!(meta.subflow_id, 11);
        assert_eq!(meta.interface_name, "Wi-Fi");

        let chunk = BondingFrame::StreamChunk {
            stream_id: 42,
            offset: 0,
            eof: false,
            data: b"hello bonded multipath world".to_vec(),
        };
        write_encrypted_frame(&mut c_write_io(&mut client_io), &mut c_write, &chunk)
            .await
            .unwrap();
        let decoded = read_encrypted_frame(&mut server_io, &mut s_read)
            .await
            .unwrap();
        match decoded {
            BondingFrame::StreamChunk {
                stream_id,
                offset,
                data,
                ..
            } => {
                assert_eq!(stream_id, 42);
                assert_eq!(offset, 0);
                assert_eq!(data, b"hello bonded multipath world");
            }
            _ => panic!("unexpected frame"),
        }

        // Server response frame
        let pong = BondingFrame::Pong {
            seq: 9,
            sent_ms: 1234,
        };
        write_encrypted_frame(&mut server_io, &mut s_write, &pong)
            .await
            .unwrap();
        let client_got = read_encrypted_frame(&mut client_io, &mut c_read)
            .await
            .unwrap();
        match client_got {
            BondingFrame::Pong { seq, .. } => assert_eq!(seq, 9),
            _ => panic!("expected pong"),
        }
    }

    fn c_write_io<T>(t: &mut T) -> &mut T {
        t
    }

    #[tokio::test]
    async fn test_handshake_rejects_wrong_psk() {
        let (mut client_io, mut server_io) = tokio::io::duplex(16384);
        let session_id = [1u8; 16];
        let (c_res, s_res) = tokio::join!(
            client_handshake(&mut client_io, session_id, 1, "Eth".into(), "wrong-key"),
            server_handshake(&mut server_io, "correct-key")
        );
        assert!(c_res.is_err());
        assert!(s_res.is_err());
    }

    #[test]
    fn test_out_of_order_multipath_reassembly() {
        let mut reasm = StreamReassembler::new(1024 * 1024);
        // Chunk 2 arrives first (via fast Ethernet)
        let out1 = reasm.push_chunk(10, b"WORLD!".to_vec(), true).unwrap();
        assert!(out1.is_empty());
        assert!(!reasm.is_complete());

        // Chunk 1 arrives second (via USB Tethering)
        let out2 = reasm.push_chunk(5, b"BOND_".to_vec(), false).unwrap();
        assert!(out2.is_empty());

        // Chunk 0 arrives last (via Wi-Fi) -> drains entire contiguous stream!
        let out3 = reasm.push_chunk(0, b"HELLO".to_vec(), false).unwrap();
        assert_eq!(out3, b"HELLOBOND_WORLD!");
        assert!(reasm.is_complete());
    }

    #[test]
    fn test_multipath_scheduler_weights_and_failover() {
        let subflows = vec![
            SubflowDescriptor {
                subflow_id: 1,
                throughput_bps: 80_000_000,
                link_capacity_bps: 100_000_000,
                rtt_ms: 15.0,
                loss_pct: 0.0,
                is_primary: true,
                custom_weight: 70,
                alive: true,
                inflight_bytes: 0,
            },
            SubflowDescriptor {
                subflow_id: 2,
                throughput_bps: 25_000_000,
                link_capacity_bps: 50_000_000,
                rtt_ms: 42.0,
                loss_pct: 0.5,
                is_primary: false,
                custom_weight: 30,
                alive: true,
                inflight_bytes: 0,
            },
        ];

        let smart = compute_subflow_weights(BondingPolicy::Smart, &subflows);
        assert!(smart[0].1 > smart[1].1);

        let failover = compute_subflow_weights(BondingPolicy::Failover, &subflows);
        assert_eq!(failover[0].1, 100.0);
        assert_eq!(failover[1].1, 0.0);

        let mut sched = MultipathScheduler::new();
        let mut c1 = 0;
        let mut c2 = 0;
        for _ in 0..20 {
            match sched.select_subflow(BondingPolicy::Custom, &subflows, 32768) {
                Some(1) => c1 += 1,
                Some(2) => c2 += 1,
                _ => {}
            }
        }
        assert!(c1 > c2 && c2 >= 4);
    }
}

