use std::time::Duration;
use tokio::time::Instant;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct Heartbeat {
    pending: Option<(i64, Instant)>,
}
impl Heartbeat {
    pub(super) fn request(&mut self) -> anyhow::Result<Option<i64>> {
        if let Some((_, deadline)) = self.pending {
            anyhow::ensure!(Instant::now() < deadline, "keepalive response timed out");
            return Ok(None);
        }
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        let id = i64::from_be_bytes(bytes[..8].try_into().expect("eight nonce bytes"));
        self.pending = Some((id, Instant::now() + RESPONSE_TIMEOUT));
        Ok(Some(id))
    }
    pub(super) fn acknowledge(&mut self, id: i64) -> anyhow::Result<()> {
        let Some((expected, deadline)) = self.pending else {
            anyhow::bail!("unsolicited keepalive response");
        };
        anyhow::ensure!(
            id == expected && Instant::now() < deadline,
            "invalid or expired keepalive response"
        );
        self.pending = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn missing_responses_do_not_reset_the_deadline() {
        let mut heartbeat = Heartbeat::default();
        let id = heartbeat.request().unwrap().unwrap();
        for _ in 0..2 {
            tokio::time::advance(Duration::from_secs(10)).await;
            assert!(heartbeat.request().unwrap().is_none());
        }
        tokio::time::advance(Duration::from_secs(10)).await;
        assert!(heartbeat.request().is_err());
        assert!(heartbeat.acknowledge(id).is_err());
    }
    #[tokio::test(start_paused = true)]
    async fn requires_the_outstanding_nonce_once() {
        let mut heartbeat = Heartbeat::default();
        assert!(heartbeat.acknowledge(0).is_err());
        let id = heartbeat.request().unwrap().unwrap();
        assert!(heartbeat.acknowledge(id.wrapping_add(1)).is_err());
        heartbeat.acknowledge(id).unwrap();
        assert!(heartbeat.acknowledge(id).is_err());
        assert!(heartbeat.request().unwrap().is_some());
    }
}
