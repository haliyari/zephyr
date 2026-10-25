use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer, State};
use smoltcp::time::Instant as SmolInstant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address, Ipv4Cidr};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

// Memory-backed QueueDevice for smoltcp
pub struct QueueDevice {
    pub rx_queue: VecDeque<Vec<u8>>,
    pub tx_queue: VecDeque<Vec<u8>>,
}

impl QueueDevice {
    pub fn new() -> Self {
        Self {
            rx_queue: VecDeque::new(),
            tx_queue: VecDeque::new(),
        }
    }
}

impl Device for QueueDevice {
    type RxToken<'a> = QueueRxToken;
    type TxToken<'a> = QueueTxToken<'a>;

    fn receive(&mut self, _timestamp: SmolInstant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if let Some(packet) = self.rx_queue.pop_front() {
            Some((QueueRxToken { packet }, QueueTxToken { tx_queue: &mut self.tx_queue }))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: SmolInstant) -> Option<Self::TxToken<'_>> {
        Some(QueueTxToken { tx_queue: &mut self.tx_queue })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1400;
        caps
    }
}

pub struct QueueRxToken {
    packet: Vec<u8>,
}

impl RxToken for QueueRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.packet)
    }
}

pub struct QueueTxToken<'a> {
    tx_queue: &'a mut VecDeque<Vec<u8>>,
}

impl<'a> TxToken for QueueTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);
        self.tx_queue.push_back(buf);
        r
    }
}

pub struct ProxyStream {
    pub tx: mpsc::Sender<Vec<u8>>,
    pub rx: mpsc::Receiver<Vec<u8>>,
}

pub enum StackCommand {
    Connect {
        dest: SocketAddr,
        reply: oneshot::Sender<Result<ProxyStream, String>>,
    },
}

struct ActiveConn {
    handle: SocketHandle,
    to_client_tx: mpsc::Sender<Vec<u8>>,
    from_client_rx: mpsc::Receiver<Vec<u8>>,
    reply_sender: Option<oneshot::Sender<Result<ProxyStream, String>>>,
    pending_to_send: VecDeque<u8>,
}

pub struct UserspaceStackHandle {
    pub cmd_tx: mpsc::Sender<StackCommand>,
}

impl UserspaceStackHandle {
    pub async fn dial(&self, dest: SocketAddr) -> Result<ProxyStream, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(StackCommand::Connect { dest, reply: reply_tx })
            .await
            .map_err(|e| format!("Stack runner channel closed: {}", e))?;
        reply_rx
            .await
            .map_err(|e| format!("Stack connect response dropped: {}", e))?
    }
}

pub fn spawn_userspace_stack(
    assigned_ip: [u8; 4],
    gateway_ip: [u8; 4],
    mut sstp_rx_packets: mpsc::Receiver<Vec<u8>>,
    sstp_tx_packets: mpsc::Sender<Vec<u8>>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> UserspaceStackHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<StackCommand>(256);

    tokio::spawn(async move {
        let mut dev = QueueDevice::new();
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = rand_seed();

        let now = SmolInstant::now();
        let mut iface = Interface::new(config, &mut dev, now);
        let client_ip = Ipv4Address::new(assigned_ip[0], assigned_ip[1], assigned_ip[2], assigned_ip[3]);
        iface.update_ip_addrs(|addrs| {
            addrs.push(IpCidr::Ipv4(Ipv4Cidr::new(client_ip, 24))).unwrap();
        });
        let gw = Ipv4Address::new(gateway_ip[0], gateway_ip[1], gateway_ip[2], gateway_ip[3]);
        iface.routes_mut().add_default_ipv4_route(gw).unwrap();

        let mut sockets = SocketSet::new(vec![]);
        let mut active_conns: HashMap<u64, ActiveConn> = HashMap::new();
        let mut conn_counter = 0u64;
        let mut ephemeral_port = 40000u16;

        let mut poll_interval = tokio::time::interval(Duration::from_millis(5));

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break;
                }
                Some(in_packet) = sstp_rx_packets.recv() => {
                    dev.rx_queue.push_back(in_packet);
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        StackCommand::Connect { dest, reply } => {
                            let ip = match dest.ip() {
                                std::net::IpAddr::V4(v4) => {
                                    let oct = v4.octets();
                                    Ipv4Address::new(oct[0], oct[1], oct[2], oct[3])
                                }
                                std::net::IpAddr::V6(_) => {
                                    let _ = reply.send(Err("IPv6 direct routing not supported over IPv4 SSTP tunnel".to_string()));
                                    continue;
                                }
                            };
                            let remote_endpoint = IpEndpoint::new(IpAddress::Ipv4(ip), dest.port());

                            let rx_buf = SocketBuffer::new(vec![0; 65536]);
                            let tx_buf = SocketBuffer::new(vec![0; 65536]);
                            let mut tcp_socket = TcpSocket::new(rx_buf, tx_buf);

                            ephemeral_port = if ephemeral_port >= 65000 { 40000 } else { ephemeral_port + 1 };
                            if let Err(e) = tcp_socket.connect(iface.context(), remote_endpoint, ephemeral_port) {
                                let _ = reply.send(Err(format!("smoltcp connect error: {:?}", e)));
                                continue;
                            }

                            let handle = sockets.add(tcp_socket);
                            conn_counter += 1;
                            let cid = conn_counter;

                            let (to_client_tx, _to_client_rx) = mpsc::channel::<Vec<u8>>(128);
                            let (_from_client_tx, from_client_rx) = mpsc::channel::<Vec<u8>>(128);

                            let active = ActiveConn {
                                handle,
                                to_client_tx,
                                from_client_rx,
                                reply_sender: Some(reply),
                                pending_to_send: VecDeque::new(),
                            };
                            active_conns.insert(cid, active);

                            // Trigger immediate poll
                            let now = SmolInstant::now();
                            iface.poll(now, &mut dev, &mut sockets);
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    // Periodic poll step below
                }
            }

            // Inbound: read data from from_client_rx channels into pending_to_send
            for (_, conn) in active_conns.iter_mut() {
                while let Ok(data) = conn.from_client_rx.try_recv() {
                    conn.pending_to_send.extend(data);
                }
            }

            // Feed pending_to_send into smoltcp sockets
            for (_, conn) in active_conns.iter_mut() {
                let socket = sockets.get_mut::<TcpSocket>(conn.handle);
                if socket.is_active() && socket.may_send() && !conn.pending_to_send.is_empty() {
                    let (slice1, _) = conn.pending_to_send.as_slices();
                    if !slice1.is_empty() {
                        if let Ok(sent) = socket.send_slice(slice1) {
                            conn.pending_to_send.drain(..sent);
                        }
                    }
                }
            }

            // Drive smoltcp interface
            let now = SmolInstant::now();
            iface.poll(now, &mut dev, &mut sockets);

            // Send generated IP packets to SSTP TLS
            while let Some(out_packet) = dev.tx_queue.pop_front() {
                if sstp_tx_packets.send(out_packet).await.is_err() {
                    break;
                }
            }

            // Check connection status & read from smoltcp sockets to client
            let mut to_remove = Vec::new();
            for (cid, conn) in active_conns.iter_mut() {
                let socket = sockets.get_mut::<TcpSocket>(conn.handle);

                // Check if connection succeeded
                if let Some(reply) = conn.reply_sender.take() {
                    if socket.is_active() && socket.state() == State::Established {
                        let (to_client_tx, to_client_rx) = mpsc::channel::<Vec<u8>>(128);
                        let (from_client_tx, from_client_rx) = mpsc::channel::<Vec<u8>>(128);
                        conn.to_client_tx = to_client_tx;
                        conn.from_client_rx = from_client_rx;
                        let _ = reply.send(Ok(ProxyStream {
                            tx: from_client_tx,
                            rx: to_client_rx,
                        }));
                    } else if socket.state() == State::Closed || socket.state() == State::TimeWait {
                        let _ = reply.send(Err("smoltcp connection rejected by remote".to_string()));
                        to_remove.push(*cid);
                        continue;
                    } else {
                        // Still connecting (SynSent), keep reply_sender
                        conn.reply_sender = Some(reply);
                    }
                }

                // Read available data from smoltcp socket to client
                if socket.is_active() && socket.may_recv() {
                    let mut buf = [0u8; 8192];
                    if let Ok(n) = socket.recv_slice(&mut buf) {
                        if n > 0 {
                            let _ = conn.to_client_tx.try_send(buf[..n].to_vec());
                        }
                    }
                }

                // Check if closed
                if socket.state() == State::Closed || socket.state() == State::TimeWait {
                    to_remove.push(*cid);
                }
            }

            for cid in to_remove {
                if let Some(conn) = active_conns.remove(&cid) {
                    sockets.remove(conn.handle);
                }
            }
        }
    });

    UserspaceStackHandle { cmd_tx }
}

fn rand_seed() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(123456789)
}
