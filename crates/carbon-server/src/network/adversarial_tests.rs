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
