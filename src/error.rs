use std::fmt;

#[derive(Debug)]
pub enum RoamError {
    Api { status: u16, message: String },
    Http(reqwest::Error),
    Config(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    TomlDe(toml::de::Error),
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

/// Structured error data for the message channel
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorInfo {
    Api { status: u16, body: String },
    Network(String),
    Write(String),
}

impl ErrorInfo {
    pub fn from_roam_error(e: &RoamError) -> Self {
        match e {
            RoamError::Api { status, message } => ErrorInfo::Api {
                status: *status,
                body: message.clone(),
            },
            _ => ErrorInfo::Network(e.to_string()),
        }
    }
}

/// Ready-to-render error popup data
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorPopup {
    pub title: String,
    pub message: String,
    pub hint: String,
}

impl ErrorPopup {
    pub fn from_error_info(info: &ErrorInfo) -> Self {
        match info {
            ErrorInfo::Api { status, body } => Self::from_api(*status, body),
            ErrorInfo::Network(msg) => Self {
                title: "Network Error".into(),
                message: truncate(msg, 80),
                hint: "Check your internet connection".into(),
            },
            ErrorInfo::Write(msg) => Self {
                title: "Write Failed".into(),
                message: truncate(msg, 80),
                hint: "Your changes may not have been saved".into(),
            },
        }
    }

    fn from_api(status: u16, body: &str) -> Self {
        let extracted_message = extract_json_message(body);

        match status {
            429 => Self {
                title: "Rate Limited".into(),
                message: extracted_message.unwrap_or_else(|| "Too many requests".into()),
                hint: "Wait a moment and try again".into(),
            },
            401 => Self {
                title: "Unauthorized".into(),
                message: "Invalid API token".into(),
                hint: "Check your config.toml".into(),
            },
            403 => Self {
                title: "Forbidden".into(),
                message: "Access denied to this graph".into(),
                hint: "Check graph permissions".into(),
            },
            500 => Self {
                title: "Server Error".into(),
                message: "Roam servers returned an error".into(),
                hint: "Try again later".into(),
            },
            _ => Self {
                title: format!("API Error ({})", status),
                message: extracted_message.unwrap_or_else(|| truncate(body, 200)),
                hint: "Try again later".into(),
            },
        }
    }
}

fn extract_json_message(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("message")?.as_str().map(String::from))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

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

    #[test]
    fn error_popup_from_429_extracts_message() {
        let info = ErrorInfo::Api {
            status: 429,
            body: r#"{"graph-name":"avelino-graph","message":"You've crossed your quotas of 50 req/min/graph, please try again later."}"#.into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Rate Limited");
        assert!(popup.message.contains("crossed your quotas"));
        assert_eq!(popup.hint, "Wait a moment and try again");
    }

    #[test]
    fn error_popup_from_429_fallback() {
        let info = ErrorInfo::Api {
            status: 429,
            body: "rate limited plain text".into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Rate Limited");
        assert_eq!(popup.message, "Too many requests");
    }

    #[test]
    fn error_popup_from_401() {
        let info = ErrorInfo::Api {
            status: 401,
            body: "".into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Unauthorized");
        assert_eq!(popup.message, "Invalid API token");
        assert_eq!(popup.hint, "Check your config.toml");
    }

    #[test]
    fn error_popup_from_403() {
        let info = ErrorInfo::Api {
            status: 403,
            body: "".into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Forbidden");
    }

    #[test]
    fn error_popup_from_500() {
        let info = ErrorInfo::Api {
            status: 500,
            body: "".into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Server Error");
    }

    #[test]
    fn error_popup_from_unknown_status_with_json() {
        let info = ErrorInfo::Api {
            status: 502,
            body: r#"{"message":"bad gateway"}"#.into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "API Error (502)");
        assert_eq!(popup.message, "bad gateway");
    }

    #[test]
    fn error_popup_from_unknown_status_plain_text() {
        let info = ErrorInfo::Api {
            status: 502,
            body: "some plain error".into(),
        };
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "API Error (502)");
        assert_eq!(popup.message, "some plain error");
    }

    #[test]
    fn error_popup_from_network() {
        let info = ErrorInfo::Network("connection refused".into());
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Network Error");
        assert_eq!(popup.message, "connection refused");
        assert_eq!(popup.hint, "Check your internet connection");
    }

    #[test]
    fn error_popup_from_write() {
        let info = ErrorInfo::Write("timeout writing block".into());
        let popup = ErrorPopup::from_error_info(&info);
        assert_eq!(popup.title, "Write Failed");
        assert_eq!(popup.message, "timeout writing block");
        assert_eq!(popup.hint, "Your changes may not have been saved");
    }

    #[test]
    fn error_popup_truncates_long_message() {
        let long_msg = "a".repeat(100);
        let info = ErrorInfo::Network(long_msg);
        let popup = ErrorPopup::from_error_info(&info);
        assert!(popup.message.len() <= 83); // 80 + "..."
        assert!(popup.message.ends_with("..."));
    }

    #[test]
    fn error_info_from_roam_api_error() {
        let err = RoamError::Api {
            status: 429,
            message: "rate limited".into(),
        };
        let info = ErrorInfo::from_roam_error(&err);
        match info {
            ErrorInfo::Api { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(body, "rate limited");
            }
            _ => panic!("Expected ErrorInfo::Api"),
        }
    }

    #[test]
    fn error_info_from_roam_http_error_becomes_network() {
        let err = RoamError::Config("bad config".into());
        let info = ErrorInfo::from_roam_error(&err);
        assert!(matches!(info, ErrorInfo::Network(_)));
    }
}
