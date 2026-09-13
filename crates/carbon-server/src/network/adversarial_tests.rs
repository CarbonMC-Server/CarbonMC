//! Loopback integration checks exercise the actual listener and transport wiring.
use super::*;
use crate::state::ServerState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn listener() -> (
    SocketAddr,
    watch::Sender<bool>,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let config = ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        online_mode: false,
        ..ServerConfig::default()
    };
    let listener = bind(&config).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, rx) = watch::channel(false);
    let state = Arc::new(ServerState::new("world".into(), 0, shutdown.clone()));
    let task = tokio::spawn(serve(listener, config, state, rx));
    (address, shutdown, task)
}

async fn status(address: SocketAddr) -> anyhow::Result<TcpStream> {
    let mut socket = TcpStream::connect(address).await?;
    let mut payload = BytesMut::new();
    carbon_protocol::encode_varint(PROTOCOL_VERSION, &mut payload);
    payload.extend_from_slice(&encode_string("localhost"));
    payload.put_u16(address.port());
    carbon_protocol::encode_varint(1, &mut payload);
    socket.write_all(&frame_packet(0, &payload)).await?;
    socket.write_all(&frame_packet(0, &[])).await?;
    let response = timeout(
        Duration::from_secs(2),
        carbon_protocol::read_frame(&mut socket),
    )
    .await??;
    if response.first() != Some(&0) {
        bail!("missing status response");
    }
    Ok(socket)
}

#[tokio::test]
async fn real_listener_rejects_per_ip_excess_and_reuses_released_slots() {
    let (address, shutdown, task) = listener().await;
    let mut held = Vec::new();
    for _ in 0..transport::MAX_CONNECTIONS_PER_IP {
        held.push(status(address).await.unwrap());
    }
    let mut excess = TcpStream::connect(address).await.unwrap();
    let mut byte = [0];
    let read = timeout(Duration::from_secs(2), excess.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(read, Ok(0) | Err(_)));
    drop(held.pop());
    let recovered = timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(socket) = status(address).await {
                break socket;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(recovered);
    shutdown.send(true).unwrap();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for mut socket in held {
        let result = timeout(Duration::from_secs(2), socket.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(result, Ok(0) | Err(_)));
    }
}

#[tokio::test]
async fn malformed_connections_do_not_prevent_valid_status_requests() {
    let (address, shutdown, task) = listener().await;
    for bytes in [vec![0], vec![0x80; 5], vec![0x81, 0x80, 0x80, 0x01]] {
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket.write_all(&bytes).await.unwrap();
        let mut buffer = [0];
        let result = timeout(Duration::from_secs(2), socket.read(&mut buffer))
            .await
            .unwrap();
        assert!(matches!(result, Ok(0) | Err(_)));
        drop(status(address).await.unwrap());
    }
    shutdown.send(true).unwrap();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn dropped_player_registration_removes_joined_state() {
    let state: Arc<dyn ServerApi> =
        Arc::new(ServerState::new("world".into(), 0, watch::channel(false).0));
    let id = Uuid::new_v4();
    assert!(state.add_player(PlayerSnapshot {
        id,
        name: "Cleanup".into(),
        world: "world".into(),
        position: BlockPosition::default(),
        game_mode: GameMode::Survival
    }));
    let registration = PlayerRegistration {
        state: Arc::clone(&state),
        id,
    };
    drop(registration);
    assert!(state.players().is_empty());
}

#[tokio::test]
async fn login_negotiates_compression_before_finished_and_configuration() {
    let (address, shutdown, task) = listener().await;
    let mut socket = TcpStream::connect(address).await.unwrap();
    let mut handshake = BytesMut::new();
    carbon_protocol::encode_varint(PROTOCOL_VERSION, &mut handshake);
    handshake.extend(encode_string("localhost"));
    handshake.put_u16(address.port());
    carbon_protocol::encode_varint(2, &mut handshake);
    socket
        .write_all(&frame_packet(0, &handshake))
        .await
        .unwrap();
    let mut login = encode_string("CompressionTest");
    login.extend([0; 16]);
    socket.write_all(&frame_packet(0, &login)).await.unwrap();
    let negotiation = timeout(
        Duration::from_secs(2),
        carbon_protocol::read_frame(&mut socket),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(negotiation, [3, 0x80, 2]);
    let finished =
        compression::decode(carbon_protocol::read_frame(&mut socket).await.unwrap()).unwrap();
    assert_eq!(finished[0], 2);
    // A small packet uses an uncompressed envelope after negotiation.
    socket.write_all(&[2, 0, 3]).await.unwrap();
    let brand =
        compression::decode(carbon_protocol::read_frame(&mut socket).await.unwrap()).unwrap();
    assert_eq!(brand[0], 1);
    assert!(brand.windows(6).any(|value| value == b"Carbon"));
    // Complete the expected configuration responses and validate the entire
    // bundled registry snapshot through the real compressed transport.
    for id in [12, 14] {
        let packet =
            compression::decode(carbon_protocol::read_frame(&mut socket).await.unwrap()).unwrap();
        assert_eq!(packet[0], id);
    }
    let mut selection = vec![1];
    for value in ["minecraft", "core", MINECRAFT_VERSION] {
        selection.extend(encode_string(value));
    }
    socket
        .write_all(&compression::encode_frames(&frame_packet(7, &selection)).unwrap())
        .await
        .unwrap();
    let mut reconstructed = Vec::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let packet =
                compression::decode(carbon_protocol::read_frame(&mut socket).await.unwrap())
                    .unwrap();
            let (id, prefix) = decode_varint(&packet).unwrap();
            reconstructed.extend(frame_packet(id, &packet[prefix..]));
            if packet == [3] {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(reconstructed, CONFIGURATION_SNAPSHOT);
    shutdown.send(true).unwrap();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
