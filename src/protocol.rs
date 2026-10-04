use anyhow::{anyhow, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTarget {
    pub token: Option<String>,
    pub is_creator: bool,
    pub extra_godot_args: Vec<String>,
}

pub struct ProtocolParser;

impl ProtocolParser {
    /// Parse a URI string like `polytoria://launch/<token>` or `polytoria://<token>`
    pub fn parse_uri(raw_uri: &str) -> Result<LaunchTarget> {
        let uri = Url::parse(raw_uri).map_err(|e| anyhow!("Invalid URL format '{}': {}", raw_uri, e))?;

        let scheme = uri.scheme();
        if scheme != "polytoria" {
            return Err(anyhow!("Unsupported URL scheme '{}' (expected 'polytoria')", scheme));
        }

        let mut is_creator = false;
        let mut token: Option<String> = None;
        let mut extra_godot_args: Vec<String> = Vec::new();

        // 1. Check query parameters first (e.g. ?token=XYZ)
        for (key, value) in uri.query_pairs() {
            if key == "token" {
                token = Some(value.to_string());
            } else if key == "creator" && (value == "1" || value == "true") {
                is_creator = true;
            } else {
                extra_godot_args.push(format!("--{}", key));
                if !value.is_empty() {
                    extra_godot_args.push(value.to_string());
                }
            }
        }

        // 2. If token not in query, check path/host segments
        // Examples:
        // - polytoria://launch/<token> -> host = "launch", path = "/<token>"
        // - polytoria://<token> -> host = "<token>"
        if token.is_none() {
            let host = uri.host_str().unwrap_or("");
            let path_segments: Vec<&str> = uri
                .path_segments()
                .map(|c| c.filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();

            if host == "launch" || host == "join" {
                if let Some(t) = path_segments.first() {
                    token = Some(t.to_string());
                }
            } else if !host.is_empty() && host != "play" {
                // polytoria://<token>
                token = Some(host.to_string());
            } else if let Some(t) = path_segments.first() {
                token = Some(t.to_string());
            }
        }

        Ok(LaunchTarget {
            token,
            is_creator,
            extra_godot_args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard_launch_url() {
        let target = ProtocolParser::parse_uri("polytoria://launch/abc123token").unwrap();
        assert_eq!(target.token, Some("abc123token".to_string()));
        assert!(!target.is_creator);
    }

    #[test]
    fn test_parse_direct_token_url() {
        let target = ProtocolParser::parse_uri("polytoria://sample-token-456").unwrap();
        assert_eq!(target.token, Some("sample-token-456".to_string()));
        assert!(!target.is_creator);
    }

    #[test]
    fn test_parse_query_token_url() {
        let target = ProtocolParser::parse_uri("polytoria://launch?token=mysecretjwt").unwrap();
        assert_eq!(target.token, Some("mysecretjwt".to_string()));
    }

    #[test]
    fn test_reject_other_schemes() {
        assert!(ProtocolParser::parse_uri("quaternion://launch/abc").is_err());
        assert!(ProtocolParser::parse_uri("https://polytoria.com").is_err());
    }
}
