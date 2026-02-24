use std::fmt;

#[derive(Debug)]
pub enum RoamError {
    Api { status: u16, message: String },
    Http(reqwest::Error),
    Config(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    TomlDe(toml::de::Error),
    #[allow(dead_code)]
    Terminal(String),
}

impl fmt::Display for RoamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { status, message } => write!(f, "API error ({}): {}", status, message),
            Self::Http(e) => write!(f, "HTTP error: {}", e),
            Self::Config(msg) => write!(f, "Config error: {}", msg),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Json(e) => write!(f, "JSON error: {}", e),
            Self::TomlDe(e) => write!(f, "TOML parse error: {}", e),
            Self::Terminal(msg) => write!(f, "Terminal error: {}", msg),
        }
    }
}

impl std::error::Error for RoamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::TomlDe(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for RoamError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

impl From<std::io::Error> for RoamError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for RoamError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<toml::de::Error> for RoamError {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlDe(e)
    }
}

pub type Result<T> = std::result::Result<T, RoamError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_displays_status_and_message() {
        let err = RoamError::Api {
            status: 401,
            message: "Unauthorized".into(),
        };
        assert_eq!(err.to_string(), "API error (401): Unauthorized");
    }

    #[test]
    fn config_error_displays_message() {
        let err = RoamError::Config("missing api_token".into());
        assert_eq!(err.to_string(), "Config error: missing api_token");
    }

    #[test]
    fn terminal_error_displays_message() {
        let err = RoamError::Terminal("failed to enter raw mode".into());
        assert_eq!(
            err.to_string(),
            "Terminal error: failed to enter raw mode"
        );
    }

    #[test]
    fn io_error_converts_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: RoamError = io_err.into();
        assert!(matches!(err, RoamError::Io(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn json_error_converts_from_serde() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: RoamError = json_err.into();
        assert!(matches!(err, RoamError::Json(_)));
    }

    #[test]
    fn toml_error_converts_from_toml_de() {
        let toml_err = toml::from_str::<toml::Value>("= invalid").unwrap_err();
        let err: RoamError = toml_err.into();
        assert!(matches!(err, RoamError::TomlDe(_)));
    }
}
