use std::{cell::OnceCell, sync::Arc, time::Duration};

use chrono::Utc;
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, CsrfToken, EndpointNotSet, EndpointSet,
    HttpClientError, PkceCodeChallenge, RedirectUrl, RefreshToken, RequestTokenError, Scope,
    StandardErrorResponse, StandardRevocableToken, TokenResponse, TokenUrl,
    basic::{
        BasicErrorResponse, BasicErrorResponseType, BasicRevocationErrorResponse,
        BasicTokenIntrospectionResponse, BasicTokenResponse,
    },
};
use schema::minecraft_profile::MinecraftProfileResponse;

use crate::{
    constants,
    models::{
        FinishedAuthorization, MinecraftAccessToken, MinecraftLoginWithXboxRequest,
        MinecraftLoginWithXboxResponse, MsaTokens, PendingAuthorization, TokenWithExpiry,
        XboxLiveAuthenticateRequest, XboxLiveAuthenticateRequestProperties,
        XboxLiveAuthenticateResponse, XboxLiveSecurityTokenRequest,
        XboxLiveSecurityTokenRequestProperties, XboxLiveSecurityTokenResponse, XstsToken,
    },
};

type OAuthClient = oauth2::Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

pub struct Authenticator {
    oauth2_client: OnceCell<OAuthClient>,
    client: reqwest::Client,
}

// #[derive(thiserror::Error, Debug)]
// pub enum AuthenticatorError {
//     #[error("Failed to make http request")]
//     Reqwest(#[from] reqwest::Error),
//     #[error("Failed to deserialize response")]
//     SerdeJson(#[from] serde_json::Error),
// }

#[derive(thiserror::Error, Debug)]
pub enum MsaAuthorizationError {
    #[error("Connection error: {0}")]
    ConnectionError(HttpClientError<reqwest::Error>),
    #[error("Invalid grant (token is expired, invalid or revoked)")]
    InvalidGrant,
    /// The Microsoft token endpoint rejected the request. `code` is the
    /// server error code (e.g. `invalid_request` or a Microsoft-specific
    /// code) and `description` is the server-provided explanation, which
    /// for Microsoft endpoints contains the actionable AADSTS/STS code.
    /// Neither field carries secrets (no tokens, codes or verifiers).
    #[error("Microsoft token endpoint error [{code}]{description_suffix}")]
    ExternalError {
        code: String,
        description_suffix: String,
    },
    #[error("Internal error")]
    InternalError,
}

/// Format an optional server-provided error description for display/logging.
/// Newlines are flattened and length is capped; the text comes from the
/// OAuth server and contains no credentials.
fn format_error_description(description: Option<&String>) -> String {
    let Some(description) = description else {
        return String::new();
    };
    let flat: String = description
        .chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.is_empty() {
        return String::new();
    }
    const MAX_LEN: usize = 500;
    let truncated: String = flat.chars().take(MAX_LEN).collect();
    if truncated.len() < flat.len() {
        format!(": {truncated}…")
    } else {
        format!(": {truncated}")
    }
}

impl MsaAuthorizationError {
    pub fn is_connection_error(&self) -> bool {
        match self {
            Self::ConnectionError(_) => true,
            _ => false,
        }
    }
}

impl
    From<
        RequestTokenError<
            HttpClientError<reqwest::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    > for MsaAuthorizationError
{
    fn from(
        value: RequestTokenError<
            HttpClientError<reqwest::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    ) -> Self {
        match value {
            RequestTokenError::ServerResponse(server_response) => {
                // Log the complete server rejection (code + description +
                // help URI). These fields are server diagnostics and carry
                // no secrets, but they are the only way to tell apart e.g.
                // an unregistered redirect URI, an SPA/platform mismatch, or
                // a redeemed/expired code.
                log::error!(
                    "Microsoft token endpoint rejected the request: {}",
                    server_response
                );
                match server_response.error() {
                    BasicErrorResponseType::InvalidGrant => Self::InvalidGrant,
                    other => Self::ExternalError {
                        code: other.to_string(),
                        description_suffix: format_error_description(
                            server_response.error_description(),
                        ),
                    },
                }
            }
            RequestTokenError::Request(error) => Self::ConnectionError(error),
            RequestTokenError::Parse(..) => Self::InternalError,
            RequestTokenError::Other(_) => Self::InternalError,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum XboxAuthenticateError {
    #[error("Connection error: {0}")]
    ConnectionError(#[from] reqwest::Error),
    #[error("Serialization error")]
    SerializationError,
    #[error("Non-OK Http Status: {0}")]
    NonOkHttpStatus(reqwest::StatusCode),
    #[error("Missing xbox user identity")]
    MissingXui,
    #[error("Missing userhash")]
    MissingUhs,
}

impl XboxAuthenticateError {
    pub fn is_connection_error(&self) -> bool {
        match self {
            Self::ConnectionError(_) => true,
            _ => false,
        }
    }
}

impl Authenticator {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            oauth2_client: OnceCell::new(),
        }
    }

    fn oauth2_client(&self) -> &OAuthClient {
        self.oauth2_client.get_or_init(|| {
            Client::new(ClientId::new(constants::CLIENT_ID.to_string()))
                .set_auth_type(oauth2::AuthType::RequestBody)
                .set_auth_uri(AuthUrl::new(constants::AUTH_URL.to_string()).unwrap())
                .set_token_uri(TokenUrl::new(constants::TOKEN_URL.to_string()).unwrap())
                .set_redirect_uri(RedirectUrl::new(constants::REDIRECT_URL.to_string()).unwrap())
        })
    }

    pub fn create_authorization(&mut self) -> PendingAuthorization {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (url, csrf_token) = self
            .oauth2_client()
            .authorize_url(CsrfToken::new_random)
            .add_extra_param("prompt", "select_account")
            .add_scope(Scope::new("XboxLive.signin".to_string()))
            .add_scope(Scope::new("XboxLive.offline_access".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        PendingAuthorization {
            url,
            csrf_token,
            pkce_verifier,
        }
    }

    pub async fn finish_authorization(
        &mut self,
        finished: FinishedAuthorization,
    ) -> Result<MsaTokens, MsaAuthorizationError> {
        let token_response = self
            .oauth2_client()
            .exchange_code(AuthorizationCode::new(finished.code))
            .set_pkce_verifier(finished.pending.pkce_verifier)
            .request_async(&self.client)
            .await;

        let token_response = token_response?;

        let expires_in = token_response
            .expires_in()
            .unwrap_or(Duration::from_secs(3600));
        let expires_at = Utc::now() + expires_in;
        Ok(MsaTokens {
            access: TokenWithExpiry {
                token: token_response.access_token().secret().as_str().into(),
                expiry: expires_at,
            },
            refresh: token_response
                .refresh_token()
                .map(|v| v.secret().as_str().into()),
        })
    }

    pub async fn refresh_msa(
        &mut self,
        refresh: &str,
        force_client_id: &Option<Arc<str>>,
    ) -> Result<Option<MsaTokens>, MsaAuthorizationError> {
        let client = if let Some(force_client_id) = force_client_id
            && &**force_client_id != constants::CLIENT_ID
        {
            &Client::new(ClientId::new(force_client_id.to_string()))
                .set_auth_type(oauth2::AuthType::RequestBody)
                .set_auth_uri(AuthUrl::new(constants::AUTH_URL.to_string()).unwrap())
                .set_token_uri(TokenUrl::new(constants::TOKEN_URL.to_string()).unwrap())
                .set_redirect_uri(RedirectUrl::new(constants::REDIRECT_URL.to_string()).unwrap())
        } else {
            self.oauth2_client()
        };

        let token_response = client
            .exchange_refresh_token(&RefreshToken::new(refresh.to_string()))
            .request_async(&self.client)
            .await;

        if let Err(RequestTokenError::ServerResponse(ref err)) = token_response
            && let BasicErrorResponseType::InvalidGrant = err.error()
        {
            return Ok(None);
        }

        let token_response = token_response?;

        let expires_in = token_response
            .expires_in()
            .unwrap_or(Duration::from_secs(3600));
        let expires_at = Utc::now() + expires_in;
        Ok(Some(MsaTokens {
            access: TokenWithExpiry {
                token: token_response.access_token().secret().as_str().into(),
                expiry: expires_at,
            },
            refresh: token_response
                .refresh_token()
                .map(|v| v.secret().as_str().into()),
        }))
    }

    pub async fn authenticate_xbox(
        &mut self,
        msa_access: &str,
    ) -> Result<TokenWithExpiry, XboxAuthenticateError> {
        let request = XboxLiveAuthenticateRequest {
            properties: XboxLiveAuthenticateRequestProperties {
                auth_method: "RPS",
                site_name: "user.auth.xboxlive.com",
                rps_ticket: &format!("d={}", msa_access),
            },
            relying_party: "http://auth.xboxlive.com",
            token_type: "JWT",
        };

        let response = self
            .client
            .post(constants::XBOX_AUTHENTICATE_URL)
            .json(&request)
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(XboxAuthenticateError::NonOkHttpStatus(response.status()));
        }

        let bytes = response.bytes().await?;

        let response: XboxLiveAuthenticateResponse = serde_json::from_slice(&bytes)
            .map_err(|_| XboxAuthenticateError::SerializationError)?;

        let skew = Utc::now() - response.issue_instant;
        Ok(TokenWithExpiry {
            token: response.token,
            expiry: response.not_after + skew,
        })
    }

    pub async fn obtain_xsts(&mut self, xbl: &str) -> Result<XstsToken, XboxAuthenticateError> {
        let request = XboxLiveSecurityTokenRequest {
            properties: XboxLiveSecurityTokenRequestProperties {
                sandbox_id: "RETAIL",
                user_tokens: &[xbl],
            },
            relying_party: "rp://api.minecraftservices.com/",
            token_type: "JWT",
        };

        let response = self
            .client
            .post(constants::XSTS_AUTHORIZE_URL)
            .json(&request)
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(XboxAuthenticateError::NonOkHttpStatus(response.status()));
        }

        let bytes = response.bytes().await?;

        let response: XboxLiveSecurityTokenResponse = serde_json::from_slice(&bytes)
            .map_err(|_| XboxAuthenticateError::SerializationError)?;

        let skew = Utc::now() - response.issue_instant;
        Ok(XstsToken {
            token: response.token,
            expiry: response.not_after + skew,
            userhash: response
                .display_claims
                .xui
                .first()
                .ok_or(XboxAuthenticateError::MissingXui)?
                .get("uhs")
                .ok_or(XboxAuthenticateError::MissingUhs)?
                .as_str()
                .into(),
        })
    }

    pub async fn authenticate_minecraft(
        &mut self,
        xsts: &str,
        userhash: &str,
    ) -> Result<TokenWithExpiry, XboxAuthenticateError> {
        let request = MinecraftLoginWithXboxRequest {
            identity_token: &format!("XBL3.0 x={};{}", userhash, xsts),
        };

        let response = self
            .client
            .post(constants::MINECRAFT_LOGIN_WITH_XBOX_URL)
            .json(&request)
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(XboxAuthenticateError::NonOkHttpStatus(response.status()));
        }

        let bytes = response.bytes().await?;

        let response: MinecraftLoginWithXboxResponse = serde_json::from_slice(&bytes)
            .map_err(|_| XboxAuthenticateError::SerializationError)?;

        Ok(TokenWithExpiry {
            token: response.access_token,
            expiry: Utc::now() + Duration::from_secs(response.expires_in as u64),
        })
    }

    pub async fn get_minecraft_profile(
        &mut self,
        access_token: &MinecraftAccessToken,
    ) -> Result<MinecraftProfileResponse, XboxAuthenticateError> {
        let response = self
            .client
            .get(constants::MINECRAFT_PROFILE_URL)
            .bearer_auth(access_token.secret())
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(XboxAuthenticateError::NonOkHttpStatus(response.status()));
        }

        let bytes = response.bytes().await?;

        serde_json::from_slice(&bytes).map_err(|_| XboxAuthenticateError::SerializationError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_error(
        code: BasicErrorResponseType,
        description: Option<&str>,
    ) -> RequestTokenError<
        HttpClientError<reqwest::Error>,
        StandardErrorResponse<BasicErrorResponseType>,
    > {
        RequestTokenError::ServerResponse(StandardErrorResponse::new(
            code,
            description.map(str::to_string),
            None,
        ))
    }

    #[test]
    fn server_rejection_keeps_code_and_description() {
        // Microsoft-specific codes arrive as `Extension`; the user-facing
        // message must contain both the code and the AADSTS explanation
        // instead of a bare "External error".
        let err = MsaAuthorizationError::from(server_error(
            BasicErrorResponseType::Extension("invalid_resource".to_string()),
            Some("AADSTS9002326: Cross-origin token redemption is permitted only for SPA"),
        ));
        let msg = format!("{err}");
        assert!(msg.contains("invalid_resource"), "got: {msg}");
        assert!(msg.contains("AADSTS9002326"), "got: {msg}");
        assert!(!matches!(err, MsaAuthorizationError::InvalidGrant));
    }

    #[test]
    fn server_rejection_without_description_still_names_code() {
        let err =
            MsaAuthorizationError::from(server_error(BasicErrorResponseType::InvalidRequest, None));
        let msg = format!("{err}");
        assert!(msg.contains("invalid_request"), "got: {msg}");
    }

    #[test]
    fn invalid_grant_still_maps_to_invalid_grant() {
        let err = MsaAuthorizationError::from(server_error(
            BasicErrorResponseType::InvalidGrant,
            Some("AADSTS70000: ..."),
        ));
        assert!(matches!(err, MsaAuthorizationError::InvalidGrant));
    }
}
