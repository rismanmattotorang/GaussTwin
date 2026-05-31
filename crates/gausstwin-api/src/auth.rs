use crate::{
    config::AuthConfig,
    error::{Error, Result},
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// JWT claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Issued at
    pub iat: i64,
    /// Expiration time
    pub exp: i64,
    /// Roles
    pub roles: Vec<String>,
    /// Permissions
    pub permissions: Vec<String>,
}

/// User credentials
#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    /// Username or email
    pub username: String,
    /// Password
    pub password: String,
}

/// Authentication token
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthToken {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: Option<String>,
    /// Token type
    pub token_type: String,
    /// Expires in seconds
    pub expires_in: i64,
}

/// Authentication manager
pub struct AuthManager {
    /// Configuration
    config: AuthConfig,
    /// JWT encoding key
    encoding_key: EncodingKey,
    /// JWT decoding key
    decoding_key: DecodingKey,
    /// Argon2 hasher
    argon2: Arc<Argon2<'static>>,
}

impl AuthManager {
    /// Create a new authentication manager
    pub fn new(config: &AuthConfig) -> Result<Self> {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());

        let argon2 = Arc::new(Argon2::default());

        Ok(Self {
            config: config.clone(),
            encoding_key,
            decoding_key,
            argon2,
        })
    }

    /// Generate a JWT token
    pub fn generate_token(
        &self,
        user_id: &str,
        roles: Vec<String>,
        permissions: Vec<String>,
    ) -> Result<AuthToken> {
        let now = Utc::now();
        let exp = now + Duration::seconds(self.config.token_expiration.try_into().unwrap());

        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            roles,
            permissions,
        };

        let access_token = encode(&Header::default(), &claims, &self.encoding_key)?;

        let refresh_token = if self.config.enable_refresh_tokens {
            let refresh_exp =
                now + Duration::seconds(self.config.refresh_token_expiration.try_into().unwrap());
            let refresh_claims = Claims {
                sub: user_id.to_string(),
                iat: now.timestamp(),
                exp: refresh_exp.timestamp(),
                roles: vec!["refresh".to_string()],
                permissions: vec![],
            };

            Some(encode(
                &Header::default(),
                &refresh_claims,
                &self.encoding_key,
            )?)
        } else {
            None
        };

        Ok(AuthToken {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.config.token_expiration.try_into().unwrap(),
        })
    }

    /// Verify a JWT token
    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        let validation = Validation::default();
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;

        Ok(token_data.claims)
    }

    /// Hash a password
    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        Ok(hash)
    }

    /// Verify a password
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        let hash = PasswordHash::new(hash)?;
        Ok(self
            .argon2
            .verify_password(password.as_bytes(), &hash)
            .is_ok())
    }

    /// Generate a session ID
    pub fn generate_session_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Check if a token is expired
    pub fn is_token_expired(&self, claims: &Claims) -> bool {
        let now = Utc::now().timestamp();
        claims.exp <= now
    }

    /// Check if a user has a role
    pub fn has_role(&self, claims: &Claims, role: &str) -> bool {
        claims.roles.contains(&role.to_string())
    }

    /// Check if a user has a permission
    pub fn has_permission(&self, claims: &Claims, permission: &str) -> bool {
        claims.permissions.contains(&permission.to_string())
    }

    /// Refresh an access token
    pub fn refresh_token(&self, refresh_token: &str) -> Result<AuthToken> {
        let claims = self.verify_token(refresh_token)?;

        if !self.has_role(&claims, "refresh") {
            return Err(Error::PermissionDenied("Invalid refresh token".into()));
        }

        self.generate_token(&claims.sub, claims.roles, claims.permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let config = AuthConfig::default();
        let auth = AuthManager::new(&config).unwrap();

        let password = "test_password";
        let hash = auth.hash_password(password).unwrap();

        assert!(auth.verify_password(password, &hash).unwrap());
        assert!(!auth.verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_token_generation() {
        let config = AuthConfig::default();
        let auth = AuthManager::new(&config).unwrap();

        let user_id = "test_user";
        let roles = vec!["user".to_string()];
        let permissions = vec!["read".to_string()];

        let token = auth
            .generate_token(user_id, roles.clone(), permissions.clone())
            .unwrap();
        let claims = auth.verify_token(&token.access_token).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.roles, roles);
        assert_eq!(claims.permissions, permissions);
    }
}
