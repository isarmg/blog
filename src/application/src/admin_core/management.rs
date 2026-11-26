use super::{
    AdministratorService, AdministratorStore, AuthenticatedIdentity, Error, PasswordWorkError,
    RequestId, require_persistable_time,
};

/// The authorization snapshot must be revalidated by the store in the same
/// transaction as the mutation, after any password hashing work has finished.
#[derive(Clone, Debug)]
pub struct AdministratorManagementContext {
    pub identity: AuthenticatedIdentity,
    pub request_id: Option<RequestId>,
    pub now_micros: u64,
}

impl AdministratorManagementContext {
    /// # Errors
    /// The timestamp cannot be represented in persistent storage.
    pub fn validate(&self) -> Result<(), Error> {
        require_persistable_time(self.now_micros)?;
        Ok(())
    }
}

/// Hashes and internal records never implement Serialize for a public response.
pub enum AdministratorMutation {
    UpdateOwnAccount {
        username: String,
        expected_password_hash: String,
        password_hash: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ManagementError<StoreError: std::error::Error + Send + Sync + 'static> {
    #[error("administrator authorization is no longer valid")]
    Unauthorized,
    #[error("administrator already exists or cannot be changed in this state")]
    Conflict,
    #[error("administrator input is invalid")]
    InvalidInput,
    #[error("current administrator password is incorrect")]
    InvalidCurrentPassword,
    #[error(transparent)]
    PasswordWork(#[from] PasswordWorkError),
    #[error(transparent)]
    Policy(#[from] Error),
    #[error(transparent)]
    Authentication(#[from] crate::admin_auth::Error),
    #[error("administrator storage failed: {0}")]
    Store(#[source] StoreError),
}

impl<Store: AdministratorStore + 'static> AdministratorService<Store> {
    /// Verify the current credential before atomically updating the session's own
    /// account. The store repeats session/CSRF checks under its mutation lock.
    pub(crate) async fn update_own_account(
        &self,
        context: &AdministratorManagementContext,
        username: &str,
        current_password: &str,
        new_password: Option<&str>,
    ) -> Result<(), ManagementError<Store::StoreError>> {
        context.validate()?;
        let username = crate::admin_auth::normalize_administrator_username(username)?;
        if current_password.len() > 1024 {
            return Err(ManagementError::InvalidInput);
        }
        let record = self
            .store
            .administrator_by_username(&context.identity.username)
            .await
            .map_err(ManagementError::Store)?
            .ok_or(ManagementError::Unauthorized)?;
        if !record.active || record.administrator_id != context.identity.administrator_id {
            return Err(ManagementError::Unauthorized);
        }
        if !self
            .verify_password(current_password, &record.password_hash)
            .await?
        {
            return Err(ManagementError::InvalidCurrentPassword);
        }
        let password_hash = match new_password {
            Some(value) => Some(self.hash_password(value).await?),
            None => None,
        };
        self.store
            .manage_administrator(
                context,
                AdministratorMutation::UpdateOwnAccount {
                    username,
                    expected_password_hash: record.password_hash,
                    password_hash,
                },
            )
            .await
    }
}
