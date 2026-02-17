pub mod jwt;
pub mod models;
pub mod oauth;
pub mod token;

use async_trait::async_trait;

use self::models::{AuthResponse, LoginRequest, RegisterRequest, UserInfo};
use crate::auth::token::service::TokenService;
use crate::core::error::AppError;
use crate::core::models::User;
use crate::core::repository::Repository;
use crate::credential::keypair::service::KeyPairService;

#[async_trait]
pub trait UserRepository: Repository<User> {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError>;
}

#[derive(Default)]
pub struct AuthService;

impl AuthService {
    pub fn new() -> Self {
        Self
    }

    pub async fn register(
        &self,
        repo: &dyn UserRepository,
        keypair_svc: &KeyPairService,
        token_svc: &TokenService,
        req: RegisterRequest,
    ) -> Result<(AuthResponse, String), AppError> {
        if repo.find_by_email(&req.email).await?.is_some() {
            return Err(AppError::Validation("Email already registered".into()));
        }

        let password_hash = self.hash_password(&req.password)?;
        let now = chrono::Utc::now();
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            email: req.email,
            name: req.name,
            password_hash,
            created_at: now,
            updated_at: now,
        };

        let user = repo.create(&user).await?;
        let (access_jwt, refresh_jwt) =
            token_svc.create_session_pair(keypair_svc, &user).await?;

        let response = AuthResponse {
            token: access_jwt,
            user: UserInfo {
                id: user.id,
                email: user.email,
                name: user.name,
            },
        };

        Ok((response, refresh_jwt))
    }

    pub async fn login(
        &self,
        repo: &dyn UserRepository,
        keypair_svc: &KeyPairService,
        token_svc: &TokenService,
        req: LoginRequest,
    ) -> Result<(AuthResponse, String), AppError> {
        let user = repo
            .find_by_email(&req.email)
            .await?
            .ok_or_else(|| AppError::Auth("Invalid email or password".into()))?;

        self.verify_password(&req.password, &user.password_hash)?;
        let (access_jwt, refresh_jwt) =
            token_svc.create_session_pair(keypair_svc, &user).await?;

        let response = AuthResponse {
            token: access_jwt,
            user: UserInfo {
                id: user.id,
                email: user.email,
                name: user.name,
            },
        };

        Ok((response, refresh_jwt))
    }

    fn hash_password(&self, password: &str) -> Result<String, AppError> {
        use argon2::{
            Argon2,
            password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
        };
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(format!("Password hashing failed: {e}")))
    }

    fn verify_password(&self, password: &str, hash: &str) -> Result<(), AppError> {
        use argon2::{
            Argon2,
            password_hash::{PasswordHash, PasswordVerifier},
        };
        let parsed = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid password hash: {e}")))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Auth("Invalid email or password".into()))
    }
}
