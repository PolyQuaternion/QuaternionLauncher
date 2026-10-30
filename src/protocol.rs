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
    /// Parse a URI string like `polytoria://clientbeta/<token>`, `polytoria://launch/<token>`, `polytoria://creator/<token>`, etc.
    pub fn parse_uri(raw_uri: &str) -> Result<LaunchTarget> {
        let trimmed = raw_uri.trim();
        if !trimmed.starts_with("polytoria://") && !trimmed.starts_with("polytoria:") {
            return Err(anyhow!("Unsupported URL scheme (expected 'polytoria://')"));
        }

        let mut is_creator = false;
        let mut token: Option<String> = None;
        let mut extra_godot_args: Vec<String> = Vec::new();

        // 1. Try standard URL parse first to extract query params
        if let Ok(uri) = Url::parse(trimmed) {
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
        }

        // 2. If token not found in query params, parse path/host components
        if token.is_none() {
            let without_scheme = if let Some(stripped) = trimmed.strip_prefix("polytoria://") {
                stripped
            } else if let Some(stripped) = trimmed.strip_prefix("polytoria:") {
                stripped
            } else {
                trimmed
            };

            let path_part = without_scheme.split('?').next().unwrap_or("").trim_matches('/');
            let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();

            if segments.len() >= 2 {
                let mode = segments[0].to_lowercase();
                if mode.contains("creator") {
                    is_creator = true;
                }
                token = Some(segments[1].to_string());
            } else if segments.len() == 1 {
                let seg = segments[0];
                let mode = seg.to_lowercase();
                if mode.contains("creator") {
                    is_creator = true;
                } else if !["launch", "client", "play", "join", "test"].iter().any(|&m| mode.starts_with(m)) {
                    // Direct token: polytoria://<token>
                    token = Some(seg.to_string());
                }
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
    fn test_parse_clientbeta_url() {
        let target = ProtocolParser::parse_uri("polytoria://clientbeta/10a9117651a148c414fb96fa52aa7a48").unwrap();
        assert_eq!(target.token, Some("10a9117651a148c414fb96fa52aa7a48".to_string()));
        assert!(!target.is_creator);
    }

    #[test]
    fn test_parse_client_beta_dash_url() {
        let target = ProtocolParser::parse_uri("polytoria://client-beta/10a9117651a148c414fb96fa52aa7a48").unwrap();
        assert_eq!(target.token, Some("10a9117651a148c414fb96fa52aa7a48".to_string()));
        assert!(!target.is_creator);
    }

    #[test]
    fn test_parse_creatorbeta_url() {
        let target = ProtocolParser::parse_uri("polytoria://creatorbeta/10a9117651a148c414fb96fa52aa7a48").unwrap();
        assert_eq!(target.token, Some("10a9117651a148c414fb96fa52aa7a48".to_string()));
        assert!(target.is_creator);
    }

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
