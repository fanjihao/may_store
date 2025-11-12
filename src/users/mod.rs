pub mod new;
pub mod view;
pub mod update;
pub mod invitation;
pub mod role;

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use password_hash::{SaltString};
use rand::thread_rng;

pub fn hash_password(plain: &str) -> Result<(String, String), String> {
	let salt = SaltString::generate(&mut thread_rng());
	let argon = Argon2::default();
	let hash = argon.hash_password(plain.as_bytes(), &salt)
		.map_err(|e| e.to_string())?
		.to_string();
	Ok((hash, "argon2id".to_string()))
}

pub fn verify_password(plain: &str, stored_hash: &str) -> Result<bool, String> {
	let parsed = PasswordHash::new(stored_hash).map_err(|e| e.to_string())?;
	let argon = Argon2::default();
	Ok(argon.verify_password(plain.as_bytes(), &parsed).is_ok())
}