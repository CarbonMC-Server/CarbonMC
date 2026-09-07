use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct StatusResponse {
    pub version: StatusVersion,
    pub players: StatusPlayers,
    pub description: StatusDescription,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusVersion {
    pub name: String,
    pub protocol: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusPlayers {
    pub max: u32,
    pub online: u32,
    pub sample: Vec<StatusPlayerSample>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusPlayerSample {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusDescription {
    pub text: String,
}
