//! Utility functions for the API server

use crate::Error;
use std::time::{Duration, Instant};

/// Generate a unique request ID
pub fn generate_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Parse duration from string
pub fn parse_duration(s: &str) -> Result<Duration, Error> {
    if s.ends_with('s') {
        let seconds: u64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| Error::Configuration(format!("Invalid duration: {}", s)))?;
        Ok(Duration::from_secs(seconds))
    } else if s.ends_with("ms") {
        let millis: u64 = s[..s.len() - 2]
            .parse()
            .map_err(|_| Error::Configuration(format!("Invalid duration: {}", s)))?;
        Ok(Duration::from_millis(millis))
    } else {
        Err(Error::Configuration(format!(
            "Invalid duration format: {}",
            s
        )))
    }
}

/// Format duration for display
pub fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

/// Measure execution time of a function
pub async fn measure_time<F, Fut, T>(f: F) -> (T, Duration)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = f().await;
    let duration = start.elapsed();
    (result, duration)
}

/// Validate email format
pub fn validate_email(email: &str) -> bool {
    use regex::Regex;
    lazy_static::lazy_static! {
        static ref EMAIL_REGEX: Regex = Regex::new(
            r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$"
        ).unwrap();
    }
    EMAIL_REGEX.is_match(email)
}

/// Generate a secure random string
pub fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789)(*&^%$#@!~";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Hash a password using Argon2
pub fn hash_password(password: &str) -> Result<String, Error> {
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

    let salt = SaltString::generate(&mut rand::thread_rng());
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| Error::Authentication(format!("Failed to hash password: {}", e)))?;

    Ok(password_hash.to_string())
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool, Error> {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};

    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| Error::Authentication(format!("Invalid hash format: {}", e)))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
