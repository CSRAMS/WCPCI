use std::fmt::{Display, Formatter};

use rocket::response::Redirect;

#[derive(Debug, Clone, Copy)]
pub enum MsgType {
    Info,
    Success,
    Error,
}

impl Display for MsgType {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            MsgType::Info => write!(f, "info"),
            MsgType::Success => write!(f, "success"),
            MsgType::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub msg: String,
    pub msg_type: MsgType,
}

impl Message {
    pub fn new(msg: String, msg_type: MsgType) -> Self {
        Self { msg, msg_type }
    }

    pub fn info(msg: &str) -> Self {
        Self::new(msg.to_string(), MsgType::Info)
    }

    pub fn success(msg: &str) -> Self {
        Self::new(msg.to_string(), MsgType::Success)
    }

    pub fn error(msg: &str) -> Self {
        Self::new(msg.to_string(), MsgType::Error)
    }

    pub fn to(&self, url: &str) -> Redirect {
        let encoded = urlencoding::encode(&self.msg).to_string();
        let formatted = format!("{url}?msg={encoded}&msg_type={}", self.msg_type);
        Redirect::to(formatted)
    }

    pub fn to_with_params(&self, url: &str, params: Vec<(&str, &str)>) -> Redirect {
        let encoded = urlencoding::encode(&self.msg).to_string();
        let mut formatted = format!("{url}?msg={encoded}&msg_type={}", self.msg_type);
        for (key, value) in params {
            let encoded_key = urlencoding::encode(key);
            let encoded_value = urlencoding::encode(value);
            formatted.push_str(&format!("&{encoded_key}={encoded_value}"));
        }
        Redirect::to(formatted)
    }
}
