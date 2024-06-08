#![doc = include_str!("../README.md")]
pub mod client;
pub mod transport;
pub use russh;

/// Useful Error type for rpc.
#[derive(Debug, Clone, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub struct Error {
    pub error: String,
    pub trace: Vec<String>,
}

impl From<&anyhow::Error> for Error {
    fn from(e: &anyhow::Error) -> Error {
        Error {
            error: e.to_string(),
            trace: e.chain().skip(1).map(|e| e.to_string()).collect(),
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Error {
        (&e).into()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

#[derive(Debug, PartialEq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum NetworkType {
    Tcp,
    Unix,
}

#[derive(Debug, PartialEq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Protcol {
    Netrpc,
    Grpc,
    #[strum(serialize = "tarpc<bincode>")]
    TarpcBincode,
}

/// This is go-plugin like handshake information.
#[derive(Debug, PartialEq, Hash)]
pub struct HandshakeInformation {
    /// Currently, core_protcol_version is always 1.
    pub core_protcol_version: u32,
    pub app_protocol_version: u32,
    pub network_type: NetworkType,
    pub network_addr: std::net::SocketAddr,
    pub protcol: Protcol,
}

impl std::fmt::Display for HandshakeInformation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}|{}|{}|{}|{}",
            self.core_protcol_version,
            self.app_protocol_version,
            self.network_type,
            self.network_addr,
            self.protcol
        )
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub enum ParseHandshakeError {
    #[error("Invalid core protocol version")]
    InvalidCoreProtocolVersion,
    ParseIntError(#[from] std::num::ParseIntError),
    ParseEnumError(#[from] strum::ParseError),
    ParseAddrError(#[from] std::net::AddrParseError),
    FromUtf8Error(#[from] std::str::Utf8Error),
    #[error("Insufficient fields")]
    InsufficientFields,
}

impl std::str::FromStr for HandshakeInformation {
    type Err = ParseHandshakeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ParseHandshakeError::InsufficientFields;

        let mut parts = s.split('|');
        let core_protcol_version = parts.next().ok_or(InsufficientFields)?.parse::<u32>()?;
        if core_protcol_version != 1 {
            return Err(ParseHandshakeError::InvalidCoreProtocolVersion);
        }
        let app_protocol_version = parts.next().ok_or(InsufficientFields)?.parse::<u32>()?;
        let network_type = parts
            .next()
            .ok_or(InsufficientFields)?
            .parse::<NetworkType>()?;
        let network_addr = parts.next().ok_or(InsufficientFields)?.parse()?;
        let protcol = parts.next().ok_or(InsufficientFields)?.parse::<Protcol>()?;

        Ok(HandshakeInformation {
            core_protcol_version,
            app_protocol_version,
            network_type,
            network_addr,
            protcol,
        })
    }
}

impl std::convert::TryFrom<&[u8]> for HandshakeInformation {
    type Error = ParseHandshakeError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        std::str::from_utf8(value)?.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handshake() {
        assert!("1|1|tcp|".parse::<HandshakeInformation>().is_err());
        assert_eq!(
            HandshakeInformation {
                core_protcol_version: 1,
                app_protocol_version: 1,
                network_type: NetworkType::Tcp,
                network_addr: "127.0.0.1:1234".parse().unwrap(),
                protcol: Protcol::TarpcBincode,
            },
            "1|1|tcp|127.0.0.1:1234|tarpc<bincode>"
                .parse::<HandshakeInformation>()
                .unwrap()
        );
        assert_eq!(
            "1|1|tcp|127.0.0.1:1234|grpc",
            HandshakeInformation {
                core_protcol_version: 1,
                app_protocol_version: 1,
                network_type: NetworkType::Tcp,
                network_addr: std::net::SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                    1234
                ),
                protcol: Protcol::Grpc,
            }
            .to_string()
        );
    }
}
