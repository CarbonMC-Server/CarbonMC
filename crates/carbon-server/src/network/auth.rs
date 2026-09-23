use super::{
    crypto::{signed_hex, LoginKey},
    transport::Connection,
};
use anyhow::{ensure, Context};
use carbon_protocol::{decode_encryption_response, encode_encryption_request, LoginStart};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use uuid::Uuid;

const SESSION_ENDPOINT: &str = "https://sessionserver.mojang.com/session/minecraft/hasJoined";
const MAX_RESPONSE: usize = 64 * 1024;

pub(super) struct Authentication {
    key: Arc<LoginKey>,
    client: reqwest::Client,
    slots: Arc<Semaphore>,
    endpoint: String,
}
#[derive(Debug, Deserialize)]
pub(super) struct Profile {
    pub(super) id: String,
    pub(super) name: String,
}
impl Authentication {
    pub(super) async fn new() -> anyhow::Result<Self> {
        let key = tokio::task::spawn_blocking(LoginKey::new).await??;
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build()?;
        Ok(Self {
            key: Arc::new(key),
            client,
            slots: Arc::new(Semaphore::new(4)),
            endpoint: SESSION_ENDPOINT.into(),
        })
    }
    pub(super) async fn login(
        &self,
        stream: &mut Connection,
        login: &LoginStart,
    ) -> anyhow::Result<Profile> {
        let permit = Arc::new(
            self.slots
                .clone()
                .try_acquire_owned()
                .context("authentication capacity exceeded")?,
        );
        let mut challenge = [0; 16];
        openssl::rand::rand_bytes(&mut challenge)?;
        stream
            .write_all(&encode_encryption_request(&self.key.public_der, &challenge))
            .await?;
        let response = decode_encryption_response(&stream.read_frame().await?)?;
        let key = self.key.clone();
        // Blocking RSA work cannot be cancelled once running. Keep its slot
        // until it exits, even if setup times out or shutdown drops this future.
        let worker_permit = permit.clone();
        let secret = tokio::task::spawn_blocking(move || {
            let _permit = worker_permit;
            key.accept(&response.shared_secret, &response.challenge, &challenge)
        })
        .await??;
        stream.enable_encryption(&secret)?;
        let mut digest = openssl::sha::Sha1::new();
        digest.update(secret.as_ref());
        digest.update(&self.key.public_der);
        let hash = signed_hex(digest.finish());
        let result = self
            .verify(&login.username, Uuid::from_bytes(login.player_id), &hash)
            .await;
        drop(permit);
        result
    }
    async fn verify(
        &self,
        username: &str,
        claimed_id: Uuid,
        hash: &str,
    ) -> anyhow::Result<Profile> {
        let mut response = self
            .client
            .get(&self.endpoint)
            .query(&[("username", username), ("serverId", hash)])
            .send()
            .await?;
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "account session was not verified"
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|n| n <= MAX_RESPONSE as u64),
            "authentication response too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                chunk.len() <= MAX_RESPONSE.saturating_sub(bytes.len()),
                "authentication response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        validated_profile(&bytes, username, claimed_id)
    }
}
fn validated_profile(
    bytes: &[u8],
    expected_name: &str,
    expected_id: Uuid,
) -> anyhow::Result<Profile> {
    ensure!(
        bytes.len() <= MAX_RESPONSE,
        "authentication response too large"
    );
    let profile: Profile = serde_json::from_slice(bytes)?;
    ensure!(
        profile.id.len() == 32 && profile.id.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid account identity"
    );
    let id = Uuid::parse_str(&profile.id)?;
    ensure!(
        !id.is_nil() && id == expected_id,
        "account identity mismatch"
    );
    ensure!(
        !profile.name.is_empty()
            && profile.name.len() <= 16
            && profile
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && profile.name.eq_ignore_ascii_case(expected_name),
        "account name mismatch"
    );
    Ok(profile)
}
#[cfg(test)]
mod tests {
    use super::*;
    async fn test_service(response: Vec<u8>) -> (Authentication, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/hasJoined", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let byte = socket.read_u8().await.unwrap();
                request.push(byte);
                assert!(request.len() < 8192);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            // Oversized headers/bodies may make the verifier close early.
            let _ = socket.write_all(&response).await;
            String::from_utf8(request).unwrap()
        });
        let authentication = Authentication {
            key: Arc::new(LoginKey::new().unwrap()),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            slots: Arc::new(Semaphore::new(4)),
            endpoint,
        };
        (authentication, task)
    }

    #[tokio::test]
    async fn session_http_response_must_be_successful_bounded_and_matching() {
        let id = Uuid::new_v4();
        let body =
            serde_json::json!({"id":id.simple().to_string(),"name":"CarbonPlayer"}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (auth, service) = test_service(response.into_bytes()).await;
        assert_eq!(
            auth.verify("CarbonPlayer", id, "-abc").await.unwrap().name,
            "CarbonPlayer"
        );
        let request = service.await.unwrap();
        assert!(request.starts_with("GET /hasJoined?username=CarbonPlayer&serverId=-abc HTTP/1.1"));
        for response in [
            b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\nContent-Length: 0\r\n\r\n"
                .to_vec(),
            b"HTTP/1.1 500 Error\r\nContent-Length: 0\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 65537\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10001\r\n{}\r\n0\r\n\r\n",
                " ".repeat(MAX_RESPONSE + 1)
            )
            .into_bytes(),
        ] {
            let (auth, service) = test_service(response).await;
            assert!(auth.verify("CarbonPlayer", id, "-abc").await.is_err());
            service.await.unwrap();
        }
    }

    #[tokio::test]
    async fn encrypted_login_binds_identity_and_streams_complete_configuration() {
        encrypted_login(true).await;
        encrypted_login(false).await;
    }

    async fn encrypted_login(verified: bool) {
        use carbon_protocol::{decode_varint, encode_varint, frame_packet};
        use openssl::{encrypt::Encrypter, pkey::PKey, rsa::Padding};
        let id = Uuid::new_v4();
        let body =
            serde_json::json!({"id":id.simple().to_string(),"name":"CarbonPlayer"}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = if verified {
            response
        } else {
            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into()
        };
        let (auth, service) = test_service(response.into_bytes()).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, rx) = tokio::sync::watch::channel(false);
        let state = Arc::new(crate::state::ServerState::new(
            "world".into(),
            0,
            shutdown.clone(),
        ));
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut connection = Connection::new(socket);
            connection.observe_shutdown(rx);
            let config = carbon_config::ServerConfig {
                online_mode: true,
                ..Default::default()
            };
            // Exercise production login wiring, including identity assignment,
            // Login Finished, acknowledgement and the full registry exchange.
            let result = super::super::handle_login(
                &mut connection,
                carbon_protocol::PROTOCOL_VERSION,
                &config,
                state.clone(),
                Some(&auth),
            )
            .await;
            assert!(result.is_err()); // Rejected session or client stops before play.
            assert!(carbon_api::ServerApi::players(state.as_ref()).is_empty());
            assert_eq!(auth.slots.available_permits(), 4);
        });
        let socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut client = Connection::new(socket);
        let mut login = carbon_protocol::encode_string("carbonplayer");
        login.extend(id.as_bytes());
        client.write_all(&frame_packet(0, &login)).await.unwrap();
        let request = client.read_frame().await.unwrap();
        let mut position = 0;
        fn varint(bytes: &[u8], position: &mut usize) -> i32 {
            let (value, count) = decode_varint(&bytes[*position..]).unwrap();
            *position += count;
            value
        }
        assert_eq!(varint(&request, &mut position), 1);
        assert_eq!(varint(&request, &mut position), 0); // Empty server ID.
        let key_len = varint(&request, &mut position) as usize;
        let public_der = &request[position..position + key_len];
        position += key_len;
        let challenge_len = varint(&request, &mut position) as usize;
        let challenge = &request[position..position + challenge_len];
        assert_eq!(challenge_len, 16);
        assert_eq!(&request[position + challenge_len..], &[1]);
        let public = PKey::public_key_from_der(public_der).unwrap();
        let secret = [37; 16];
        let mut response = bytes::BytesMut::new();
        for plain in [secret.as_slice(), challenge] {
            let mut encrypter = Encrypter::new(&public).unwrap();
            encrypter.set_rsa_padding(Padding::PKCS1).unwrap();
            let mut ciphertext = vec![0; public.size()];
            let length = encrypter.encrypt(plain, &mut ciphertext).unwrap();
            encode_varint(length as i32, &mut response);
            response.extend_from_slice(&ciphertext[..length]);
        }
        client.write_all(&frame_packet(1, &response)).await.unwrap();
        client.enable_encryption(&secret).unwrap();
        if !verified {
            // Failed session verification must not send Login Finished or admit
            // an offline fallback identity, even after valid RSA negotiation.
            assert!(client.read_frame().await.is_err());
            server.await.unwrap();
            service.await.unwrap();
            return;
        }
        assert_eq!(client.read_frame().await.unwrap(), [3, 0x80, 2]);
        let finished =
            super::super::compression::decode(client.read_frame().await.unwrap()).unwrap();
        assert_eq!(finished[0], 2);
        assert_eq!(&finished[1..17], id.as_bytes());
        assert_eq!(
            &finished[17..30],
            carbon_protocol::encode_string("CarbonPlayer")
        );
        client
            .write_all(&super::super::compression::encode_frames(&frame_packet(3, &[])).unwrap())
            .await
            .unwrap();
        for expected in [1, 12, 14] {
            let packet =
                super::super::compression::decode(client.read_frame().await.unwrap()).unwrap();
            assert_eq!(packet[0], expected);
        }
        let mut selection = vec![1];
        for value in ["minecraft", "core", carbon_protocol::MINECRAFT_VERSION] {
            selection.extend(carbon_protocol::encode_string(value));
        }
        client
            .write_all(
                &super::super::compression::encode_frames(&frame_packet(7, &selection)).unwrap(),
            )
            .await
            .unwrap();
        let mut reconstructed = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let packet =
                    super::super::compression::decode(client.read_frame().await.unwrap()).unwrap();
                let (packet_id, prefix) = decode_varint(&packet).unwrap();
                reconstructed.extend(frame_packet(packet_id, &packet[prefix..]));
                if packet == [3] {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(reconstructed, carbon_protocol::CONFIGURATION_SNAPSHOT);
        shutdown.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
        let mut digest = openssl::sha::Sha1::new();
        digest.update(&secret);
        digest.update(public_der);
        let hash = signed_hex(digest.finish());
        assert!(service
            .await
            .unwrap()
            .contains(&format!("username=carbonplayer&serverId={hash}")));
    }

    #[tokio::test]
    async fn production_session_client_rejects_plain_http() {
        let mut auth = Authentication::new().await.unwrap();
        auth.endpoint = "http://127.0.0.1:1/hasJoined".into();
        let error = auth
            .verify("Player", Uuid::new_v4(), "abc")
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<reqwest::Error>().unwrap().is_builder());
    }

    #[tokio::test]
    async fn session_body_stalls_expire_without_verifying_identity() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut auth, unused) = test_service(Vec::new()).await;
        unused.abort();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        auth.endpoint = format!("http://{}/hasJoined", listener.local_addr().unwrap());
        let service = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
                assert!(request.len() < 8192);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            auth.verify("Player", Uuid::new_v4(), "abc"),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.downcast_ref::<reqwest::Error>().unwrap().is_timeout());
        service.abort();
    }

    #[tokio::test]
    async fn authentication_capacity_is_bounded_and_cancelled_logins_release_slots() {
        let (auth, service) = test_service(Vec::new()).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let mut connection = Connection::new(socket);
        let login = LoginStart {
            username: "Player".into(),
            player_id: *Uuid::new_v4().as_bytes(),
        };
        let held = auth.slots.clone().acquire_many_owned(4).await.unwrap();
        assert!(auth
            .login(&mut connection, &login)
            .await
            .unwrap_err()
            .to_string()
            .contains("capacity"));
        drop(held);
        assert!(tokio::time::timeout(
            Duration::from_millis(30),
            auth.login(&mut connection, &login)
        )
        .await
        .is_err());
        assert_eq!(auth.slots.available_permits(), 4);
        drop(client);
        service.abort();
    }

    #[tokio::test]
    async fn production_login_honors_setup_deadline_and_shutdown_during_authentication() {
        for stop in [false, true] {
            let (auth, service) = test_service(Vec::new()).await;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let socket = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (peer, _) = listener.accept().await.unwrap();
            let (shutdown, rx) = tokio::sync::watch::channel(false);
            let state = Arc::new(crate::state::ServerState::new(
                "world".into(),
                0,
                shutdown.clone(),
            ));
            let task = tokio::spawn(async move {
                let mut connection = Connection::new(peer);
                connection.observe_shutdown(rx);
                let result = super::super::handle_login(
                    &mut connection,
                    carbon_protocol::PROTOCOL_VERSION,
                    &carbon_config::ServerConfig::default(),
                    state,
                    Some(&auth),
                )
                .await;
                assert!(result.is_err());
                assert_eq!(auth.slots.available_permits(), 4);
            });
            let mut client = Connection::new(socket);
            let mut login = carbon_protocol::encode_string("Player");
            login.extend(Uuid::new_v4().as_bytes());
            client
                .write_all(&carbon_protocol::frame_packet(0, &login))
                .await
                .unwrap();
            assert_eq!(client.read_frame().await.unwrap()[0], 1);
            if stop {
                shutdown.send(true).unwrap();
            } else {
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(31)).await;
            }
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap();
            if !stop {
                tokio::time::resume();
            }
            service.abort();
        }
    }

    #[test]
    fn profile_must_match_both_claims_and_valid_account_syntax() {
        let id = Uuid::new_v4();
        let json =
            serde_json::json!({"id":id.simple().to_string(),"name":"CarbonPlayer"}).to_string();
        assert!(validated_profile(json.as_bytes(), "carbonplayer", id).is_ok());
        assert!(validated_profile(json.as_bytes(), "SomeoneElse", id).is_err());
        assert!(validated_profile(json.as_bytes(), "CarbonPlayer", Uuid::new_v4()).is_err());
        for name in ["", "Bad Name", "Bad\nName", "aaaaaaaaaaaaaaaaa"] {
            let json = serde_json::json!({"id":id.simple().to_string(),"name":name}).to_string();
            assert!(validated_profile(json.as_bytes(), name, id).is_err());
        }
        assert!(validated_profile(&vec![b' '; MAX_RESPONSE + 1], "CarbonPlayer", id).is_err());
        assert!(validated_profile(b"{}", "CarbonPlayer", id).is_err());
    }
}
