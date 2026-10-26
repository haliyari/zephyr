use hmac::{Hmac, Mac};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_rustls::TlsConnector;

#[derive(Debug)]
struct NoCertVerifier;

impl ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

const SSTP_VERSION: u8 = 0x10;
const CALL_CONNECT_REQUEST: u16 = 1;
const CALL_CONNECT_ACK: u16 = 2;
const CALL_CONNECTED: u16 = 4;
const CALL_DISCONNECT: u16 = 6;
const ECHO_REQUEST: u16 = 8;
#[allow(dead_code)]
const ECHO_RESPONSE: u16 = 9;

const ATTR_ENCAPSULATED_PROTOCOL: u8 = 1;
const ATTR_CRYPTO_BINDING: u8 = 3;
const ATTR_CRYPTO_BINDING_REQ: u8 = 4;
const PROTOCOL_PPP: u8 = 1;

const PPP_LCP: u16 = 0xc021;
const PPP_PAP: u16 = 0xc023;
const PPP_IPCP: u16 = 0x8021;
const CONF_REQ: u8 = 1;
const CONF_ACK: u8 = 2;
const CONF_NAK: u8 = 3;

pub struct SstpSession {
    pub assigned_ip: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub tx_packets: mpsc::Sender<Vec<u8>>,
    pub rx_packets: mpsc::Receiver<Vec<u8>>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

pub fn crypto_binding(hash_proto: u8, nonce: &[u8], cert_der: &[u8]) -> Vec<u8> {
    let cert_hash = if hash_proto == 2 {
        let mut h = Sha256::new();
        h.update(cert_der);
        h.finalize().to_vec()
    } else {
        let mut h = Sha1::new();
        h.update(cert_der);
        h.finalize().to_vec()
    };

    let mut value = vec![0u8; 100];
    value[3] = hash_proto;
    value[4..36].copy_from_slice(nonce);
    value[36..36 + cert_hash.len()].copy_from_slice(&cert_hash);

    let hlak = [0u8; 32];
    type HmacSha256 = Hmac<Sha256>;
    type HmacSha1 = Hmac<Sha1>;

    let cmk = if hash_proto == 2 {
        let mut mac = HmacSha256::new_from_slice(&hlak).unwrap();
        mac.update(b"SSTP inner method derived CMK");
        mac.update(&[32, 0, 1]);
        mac.finalize().into_bytes().to_vec()
    } else {
        let mut mac = HmacSha1::new_from_slice(&hlak).unwrap();
        mac.update(b"SSTP inner method derived CMK");
        mac.update(&[20, 0, 1]);
        mac.finalize().into_bytes().to_vec()
    };

    let mut packet = vec![0u8; 112];
    packet[0] = SSTP_VERSION;
    packet[1] = 1;
    packet[2] = 0;
    packet[3] = 112;
    packet[4] = 0;
    packet[5] = CALL_CONNECTED as u8;
    packet[6] = 0;
    packet[7] = 1; // 1 attribute
    packet[8] = 0;
    packet[9] = ATTR_CRYPTO_BINDING;
    packet[10] = 0;
    packet[11] = 104; // attribute len (4 + 100)
    packet[12..].copy_from_slice(&value);

    let compound_mac = if hash_proto == 2 {
        let mut mac = HmacSha256::new_from_slice(&cmk).unwrap();
        mac.update(&packet);
        mac.finalize().into_bytes().to_vec()
    } else {
        let mut mac = HmacSha1::new_from_slice(&cmk).unwrap();
        mac.update(&packet);
        mac.finalize().into_bytes().to_vec()
    };

    value[68..68 + compound_mac.len()].copy_from_slice(&compound_mac);
    value
}

pub fn encode_sstp_control(msg: u16, attrs: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut total_len = 8;
    for (_, val) in attrs {
        total_len += 4 + val.len();
    }
    let mut buf = Vec::with_capacity(total_len);
    buf.push(SSTP_VERSION);
    buf.push(1); // Control
    buf.extend_from_slice(&(total_len as u16).to_be_bytes());
    buf.extend_from_slice(&msg.to_be_bytes());
    buf.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
    for (attr_id, val) in attrs {
        buf.push(0);
        buf.push(*attr_id);
        let attr_len = (4 + val.len()) as u16;
        buf.extend_from_slice(&attr_len.to_be_bytes());
        buf.extend_from_slice(val);
    }
    buf
}

pub fn encode_sstp_data(payload: &[u8]) -> Vec<u8> {
    let total_len = 4 + payload.len();
    let mut buf = Vec::with_capacity(total_len);
    buf.push(SSTP_VERSION);
    buf.push(0); // Data
    buf.extend_from_slice(&(total_len as u16).to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

pub fn encode_ppp(proto: u16, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.push(0xff);
    buf.push(0x03);
    buf.extend_from_slice(&proto.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

pub fn encode_ppp_control(code: u8, id: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.push(code);
    buf.push(id);
    let len = (4 + payload.len()) as u16;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

pub async fn read_sstp_packet<R: AsyncReadExt + Unpin>(stream: &mut R) -> Result<(bool, Vec<u8>), String> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await.map_err(|e| e.to_string())?;
    if header[0] != SSTP_VERSION {
        return Err("Invalid SSTP version".to_string());
    }
    let is_control = (header[1] & 1) != 0;
    let len = u16::from_be_bytes([header[2], header[3]]) as usize;
    if len < 4 {
        return Err("Packet too short".to_string());
    }
    let mut body = vec![0u8; len - 4];
    stream.read_exact(&mut body).await.map_err(|e| e.to_string())?;
    Ok((is_control, body))
}

pub async fn connect_sstp(
    server: &str,
    port: u16,
    sni: &str,
    username: &str,
    password: &str,
) -> Result<SstpSession, String> {
    let target = format!("{}:{}", server, port);
    let tcp = TcpStream::connect(&target)
        .await
        .map_err(|e| format!("TCP connect to {} failed: {}", target, e))?;

    let root_store = rustls::RootCertStore::empty();
    let mut client_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    client_config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertVerifier));

    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(sni.to_string())
        .map_err(|e| format!("Invalid SNI '{}': {}", sni, e))?;

    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| format!("TLS handshake with {} failed: {}", target, e))?;

    let cert_der = {
        let (_, conn) = tls.get_ref();
        conn.peer_certificates()
            .and_then(|certs| certs.first())
            .ok_or_else(|| "Server sent no peer certificate".to_string())?
            .as_ref()
            .to_vec()
    };

    let http_req = format!(
        "SSTP_DUPLEX_POST /sra_{{BA195980-CD49-458b-9E23-C84EE0ADCD75}}/ HTTP/1.1\r\nHost: {}\r\nContent-Length: 18446744073709551615\r\n\r\n",
        sni
    );
    tls.write_all(http_req.as_bytes())
        .await
        .map_err(|e| format!("Failed to send SSTP HTTP handshake: {}", e))?;

    let mut http_resp = vec![0u8; 1024];
    let n = tls
        .read(&mut http_resp)
        .await
        .map_err(|e| format!("Failed to read SSTP HTTP response: {}", e))?;
    let resp_str = String::from_utf8_lossy(&http_resp[..n]);
    if !resp_str.contains("200") {
        return Err(format!("Server rejected SSTP HTTP request: {}", resp_str));
    }

    let req = encode_sstp_control(
        CALL_CONNECT_REQUEST,
        &[(ATTR_ENCAPSULATED_PROTOCOL, vec![0, PROTOCOL_PPP])],
    );
    tls.write_all(&req)
        .await
        .map_err(|e| format!("Failed to write CALL_CONNECT_REQUEST: {}", e))?;

    let (is_ctrl, body) = read_sstp_packet(&mut tls).await?;
    if !is_ctrl || body.len() < 4 || u16::from_be_bytes([body[0], body[1]]) != CALL_CONNECT_ACK {
        return Err("Expected CALL_CONNECT_ACK from server".to_string());
    }

    let mut nonce = vec![0u8; 32];
    let mut hash_proto = 2u8;
    let mut offset = 4;
    while offset + 4 <= body.len() {
        let attr_id = body[offset + 1];
        let attr_len = u16::from_be_bytes([body[offset + 2], body[offset + 3]]) as usize;
        if attr_id == ATTR_CRYPTO_BINDING_REQ && attr_len >= 4 + 36 {
            let val = &body[offset + 4..offset + attr_len];
            hash_proto = if (val[3] & 2) != 0 { 2 } else { 1 };
            nonce.copy_from_slice(&val[4..36]);
            break;
        }
        offset += attr_len;
    }

    // LCP
    let lcp_req = encode_ppp(PPP_LCP, &encode_ppp_control(CONF_REQ, 1, &[1, 4, 0x05, 0xdc]));
    tls.write_all(&encode_sstp_data(&lcp_req)).await.map_err(|e| e.to_string())?;

    let mut client_configured = false;
    let mut server_configured = false;
    for _ in 0..16 {
        let (is_ctrl, mut body) = read_sstp_packet(&mut tls).await?;
        if is_ctrl { continue; }
        if body.starts_with(&[0xff, 0x03]) { body = body[2..].to_vec(); }
        if body.len() < 2 || u16::from_be_bytes([body[0], body[1]]) != PPP_LCP { continue; }
        let ppp_body = &body[2..];
        let code = ppp_body[0];
        let id = ppp_body[1];
        let payload = &ppp_body[4..];

        if code == CONF_REQ {
            let mut wants_pap = false;
            let mut off = 0;
            while off + 2 <= payload.len() {
                let opt_type = payload[off];
                let opt_len = payload[off + 1] as usize;
                if opt_type == 3 && opt_len >= 4 {
                    let auth_proto = u16::from_be_bytes([payload[off + 2], payload[off + 3]]);
                    if auth_proto == PPP_PAP { wants_pap = true; }
                }
                off += opt_len;
            }

            if !wants_pap {
                let nak_opt = &[3, 4, 0xc0, 0x23];
                let nak = encode_ppp(PPP_LCP, &encode_ppp_control(CONF_NAK, id, nak_opt));
                tls.write_all(&encode_sstp_data(&nak)).await.map_err(|e| e.to_string())?;
                continue;
            }

            let ack = encode_ppp(PPP_LCP, &encode_ppp_control(CONF_ACK, id, payload));
            tls.write_all(&encode_sstp_data(&ack)).await.map_err(|e| e.to_string())?;
            server_configured = true;
        } else if code == CONF_ACK && id == 1 {
            client_configured = true;
        }

        if client_configured && server_configured { break; }
    }

    if !client_configured || !server_configured {
        return Err("LCP negotiation failed to converge".to_string());
    }

    // PAP
    let u_bytes = username.as_bytes();
    let p_bytes = password.as_bytes();
    let mut pap_payload = vec![u_bytes.len() as u8];
    pap_payload.extend_from_slice(u_bytes);
    pap_payload.push(p_bytes.len() as u8);
    pap_payload.extend_from_slice(p_bytes);
    let pap_req = encode_ppp(PPP_PAP, &encode_ppp_control(CONF_REQ, 2, &pap_payload));
    tls.write_all(&encode_sstp_data(&pap_req)).await.map_err(|e| e.to_string())?;

    let mut pap_ok = false;
    for _ in 0..10 {
        let (is_ctrl, mut body) = read_sstp_packet(&mut tls).await?;
        if is_ctrl { continue; }
        if body.starts_with(&[0xff, 0x03]) { body = body[2..].to_vec(); }
        if body.len() >= 4 && u16::from_be_bytes([body[0], body[1]]) == PPP_PAP && body[2] == CONF_ACK {
            pap_ok = true;
            break;
        }
    }
    if !pap_ok {
        return Err("PAP authentication was rejected by server".to_string());
    }

    // Call Connected
    let binding = crypto_binding(hash_proto, &nonce, &cert_der);
    let connected_msg = encode_sstp_control(CALL_CONNECTED, &[(ATTR_CRYPTO_BINDING, binding)]);
    tls.write_all(&connected_msg).await.map_err(|e| e.to_string())?;

    // IPCP
    let ipcp_req = encode_ppp(PPP_IPCP, &encode_ppp_control(CONF_REQ, 3, &[3, 6, 0, 0, 0, 0]));
    tls.write_all(&encode_sstp_data(&ipcp_req)).await.map_err(|e| e.to_string())?;

    let mut assigned_ip = None;
    let mut gateway_ip = [ipcp_req[0], 0, 0, 1];
    let mut ipcp_client_ok = false;
    let mut ipcp_server_ok = false;
    let mut req_id = 3u8;

    for _ in 0..20 {
        let (is_ctrl, mut body) = read_sstp_packet(&mut tls).await?;
        if is_ctrl { continue; }
        if body.starts_with(&[0xff, 0x03]) { body = body[2..].to_vec(); }
        if body.len() < 2 || u16::from_be_bytes([body[0], body[1]]) != PPP_IPCP { continue; }
        let ppp_body = &body[2..];
        let code = ppp_body[0];
        let id = ppp_body[1];
        let payload = &ppp_body[4..];

        if code == CONF_REQ {
            if payload.len() >= 6 && payload[0] == 3 {
                gateway_ip = [payload[2], payload[3], payload[4], payload[5]];
            }
            let ack = encode_ppp(PPP_IPCP, &encode_ppp_control(CONF_ACK, id, payload));
            tls.write_all(&encode_sstp_data(&ack)).await.map_err(|e| e.to_string())?;
            ipcp_server_ok = true;
        } else if code == CONF_NAK && id == req_id {
            if payload.len() >= 6 && payload[0] == 3 {
                let ip_bytes = [payload[2], payload[3], payload[4], payload[5]];
                assigned_ip = Some(ip_bytes);
                req_id += 1;
                let new_req = encode_ppp(PPP_IPCP, &encode_ppp_control(CONF_REQ, req_id, &[3, 6, ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]]));
                tls.write_all(&encode_sstp_data(&new_req)).await.map_err(|e| e.to_string())?;
            }
        } else if code == CONF_ACK && id == req_id {
            ipcp_client_ok = true;
        }

        if ipcp_client_ok && ipcp_server_ok && assigned_ip.is_some() {
            break;
        }
    }

    let ip = assigned_ip.ok_or_else(|| "Failed to negotiate IPv4 address in IPCP".to_string())?;

    let (mut tls_read, mut tls_write) = tokio::io::split(tls);
    let (tx_to_sstp, mut rx_from_stack) = mpsc::channel::<Vec<u8>>(1024);
    let (tx_to_stack, rx_to_stack) = mpsc::channel::<Vec<u8>>(1024);
    let (tx_ctrl_to_sstp, mut rx_ctrl_from_inbound) = mpsc::channel::<Vec<u8>>(128);
    let (shutdown_tx, mut shutdown_rx1) = tokio::sync::broadcast::channel::<()>(4);
    let mut shutdown_rx2 = shutdown_tx.subscribe();
    let shutdown_tx_outbound = shutdown_tx.clone();
    let shutdown_tx_inbound = shutdown_tx.clone();

    // Outbound Task: stack & control packets -> SSTP TLS
    tokio::spawn(async move {
        let mut echo_ticker = tokio::time::interval(Duration::from_secs(15));
        echo_ticker.tick().await;

        loop {
            tokio::select! {
                _ = shutdown_rx1.recv() => {
                    let _ = tls_write.write_all(&encode_sstp_control(CALL_DISCONNECT, &[])).await;
                    let _ = tls_write.shutdown().await;
                    break;
                }
                Some(ctrl_pkt) = rx_ctrl_from_inbound.recv() => {
                    if tls_write.write_all(&ctrl_pkt).await.is_err() {
                        let _ = shutdown_tx_outbound.send(());
                        break;
                    }
                }
                _ = echo_ticker.tick() => {
                    if tls_write.write_all(&encode_sstp_control(ECHO_REQUEST, &[])).await.is_err() {
                        let _ = shutdown_tx_outbound.send(());
                        break;
                    }
                }
                Some(ipv4_packet) = rx_from_stack.recv() => {
                    let ppp = encode_ppp(0x0021, &ipv4_packet);
                    if tls_write.write_all(&encode_sstp_data(&ppp)).await.is_err() {
                        let _ = shutdown_tx_outbound.send(());
                        break;
                    }
                }
            }
        }
    });

    // Inbound Task: SSTP TLS -> stack & keepalive control responder
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx2.recv() => {
                    break;
                }
                res = read_sstp_packet(&mut tls_read) => {
                    match res {
                        Ok((is_ctrl, mut body)) => {
                            if is_ctrl {
                                if body.len() >= 2 {
                                    let msg = u16::from_be_bytes([body[0], body[1]]);
                                    if msg == ECHO_REQUEST {
                                        // MS-SSTP Server Keepalive: reply with ECHO_RESPONSE immediately
                                        let resp = encode_sstp_control(ECHO_RESPONSE, &[]);
                                        let _ = tx_ctrl_to_sstp.send(resp).await;
                                    } else if msg == CALL_DISCONNECT || msg == 5 { // CALL_ABORT
                                        let _ = shutdown_tx_inbound.send(());
                                        break;
                                    }
                                }
                            } else {
                                if body.starts_with(&[0xff, 0x03]) {
                                    body = body[2..].to_vec();
                                }
                                if body.len() >= 2 {
                                    let proto = u16::from_be_bytes([body[0], body[1]]);
                                    if proto == 0x0021 {
                                        // IPv4 Data Packet
                                        let ipv4_packet = body[2..].to_vec();
                                        if tx_to_stack.send(ipv4_packet).await.is_err() {
                                            let _ = shutdown_tx_inbound.send(());
                                            break;
                                        }
                                    } else if proto == PPP_LCP {
                                        // PPP LCP Watchdog / Keepalive Handler
                                        let ppp_body = &body[2..];
                                        if !ppp_body.is_empty() {
                                            let code = ppp_body[0];
                                            let id = if ppp_body.len() > 1 { ppp_body[1] } else { 0 };
                                            if code == 9 {
                                                // LCP Echo-Request (Code 9): MUST reply with Echo-Reply (Code 10)
                                                // Otherwise SoftEther/VPN Gate server drops connection after 2-3 minutes!
                                                let mut reply = Vec::with_capacity(ppp_body.len());
                                                reply.push(10); // Echo-Reply Code
                                                reply.push(id); // Matching Identifier
                                                if ppp_body.len() >= 4 {
                                                    reply.extend_from_slice(&ppp_body[2..4]); // Length
                                                    if ppp_body.len() >= 8 {
                                                        // Replace sender's magic number with client magic (0x00000000)
                                                        reply.extend_from_slice(&[0, 0, 0, 0]);
                                                        reply.extend_from_slice(&ppp_body[8..]); // Preserve data
                                                    } else {
                                                        reply.extend_from_slice(&ppp_body[4..]);
                                                    }
                                                } else {
                                                    reply.extend_from_slice(&[0, 4]);
                                                }
                                                let lcp_reply_pkt = encode_ppp(PPP_LCP, &reply);
                                                let sstp_data = encode_sstp_data(&lcp_reply_pkt);
                                                let _ = tx_ctrl_to_sstp.send(sstp_data).await;
                                            } else if code == 5 {
                                                // LCP Terminate-Request (Code 5): Reply with Terminate-Ack (Code 6)
                                                let term_ack = encode_ppp(PPP_LCP, &encode_ppp_control(6, id, &[]));
                                                let _ = tx_ctrl_to_sstp.send(encode_sstp_data(&term_ack)).await;
                                                let _ = shutdown_tx_inbound.send(());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            let _ = shutdown_tx_inbound.send(());
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(SstpSession {
        assigned_ip: ip,
        gateway_ip,
        tx_packets: tx_to_sstp,
        rx_packets: rx_to_stack,
        shutdown_tx,
    })
}
