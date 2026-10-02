//! The signed-in account: reading it, editing it, adding a second identifier.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::PgExecutor;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::extract::{ApiJson, Session};
use crate::auth;
use crate::domain::identity::Identifier;
use crate::error::{ApiError, ErrorBody, ErrorCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Es,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Es => "es",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Account {
    pub id: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Empty until the person has chosen one.
    pub display_name: String,
    pub language: Language,
    /// The holder has confirmed they are 18 or over. Required before signing.
    pub adult_confirmed: bool,
}

type AccountRow = (
    Uuid,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<OffsetDateTime>,
);

const ACCOUNT_COLUMNS: &str = "id, email, phone, display_name, language, adult_confirmed_at";

fn from_row((id, email, phone, display_name, language, adult_confirmed_at): AccountRow) -> Account {
    Account {
        id: id.to_string(),
        email,
        phone,
        display_name,
        language: if language == "es" {
            Language::Es
        } else {
            Language::En
        },
        adult_confirmed: adult_confirmed_at.is_some(),
    }
}

pub async fn load(db: impl PgExecutor<'_>, id: Uuid) -> Result<Account, ApiError> {
    let row: AccountRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ACCOUNT_COLUMNS} FROM account WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok(from_row(row))
}

/// The signed-in account.
#[utoipa::path(
    get,
    path = "/v1/me",
    responses(
        (status = 200, description = "The account", body = Account),
        (status = 401, description = "Not signed in", body = ErrorBody)
    )
)]
pub async fn me(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<Account>, ApiError> {
    Ok(Json(load(&state.db, session.account_id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccount {
    /// 1 to 100 characters.
    pub display_name: Option<String>,
    pub language: Option<Language>,
    /// Only `true` is meaningful: a confirmation cannot be taken back.
    pub adult_confirmed: Option<bool>,
}

/// Changes the display name or language, or records that the holder is an adult.
#[utoipa::path(
    patch,
    path = "/v1/me",
    request_body = UpdateAccount,
    responses(
        (status = 200, description = "The updated account", body = Account),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 422, description = "Invalid request", body = ErrorBody)
    )
)]
pub async fn update_me(
    State(state): State<AppState>,
    session: Session,
    ApiJson(update): ApiJson<UpdateAccount>,
) -> Result<Json<Account>, ApiError> {
    let display_name = match update.display_name.as_deref().map(str::trim) {
        Some(name) if name.is_empty() || name.chars().count() > 100 => {
            return Err(ErrorCode::InvalidRequest.into());
        }
        name => name,
    };

    sqlx::query(
        "UPDATE account
         SET display_name = coalesce($2, display_name),
             language = coalesce($3, language),
             adult_confirmed_at = CASE WHEN $4 THEN coalesce(adult_confirmed_at, now())
                                       ELSE adult_confirmed_at END
         WHERE id = $1",
    )
    .bind(session.account_id)
    .bind(display_name)
    .bind(update.language.map(Language::as_str))
    .bind(update.adult_confirmed == Some(true))
    .execute(&state.db)
    .await?;

    Ok(Json(load(&state.db, session.account_id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddIdentifier {
    /// An email address, or a phone number in international form.
    pub identifier: String,
    /// The one-time code sent to it.
    pub code: String,
}

/// Verifies a second identifier and attaches it to the account, or replaces
/// the one of the same kind. An account with both a verified email and a
/// verified phone can meet the higher risk tier.
#[utoipa::path(
    post,
    path = "/v1/me/identifiers",
    request_body = AddIdentifier,
    responses(
        (status = 200, description = "The updated account", body = Account),
        (status = 401, description = "Not signed in, or the code is wrong", body = ErrorBody),
        (status = 409, description = "The identifier belongs to another account", body = ErrorBody),
        (status = 422, description = "Not an email address or phone number", body = ErrorBody)
    )
)]
pub async fn add_identifier(
    State(state): State<AppState>,
    session: Session,
    ApiJson(body): ApiJson<AddIdentifier>,
) -> Result<Json<Account>, ApiError> {
    let identifier = Identifier::parse(&body.identifier)?;
    let settings = &state.settings;
    auth::verify_code(
        &state.db,
        &settings.app_secret,
        &settings.auth,
        &identifier,
        &body.code,
    )
    .await?;

    let update = match identifier {
        Identifier::Email(_) => "UPDATE account SET email = $2 WHERE id = $1",
        Identifier::Phone(_) => "UPDATE account SET phone = $2 WHERE id = $1",
    };
    let result = sqlx::query(update)
        .bind(session.account_id)
        .bind(identifier.as_str())
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => Ok(Json(load(&state.db, session.account_id).await?)),
        Err(error) if is_unique_violation(&error) => Err(ErrorCode::IdentifierInUse.into()),
        Err(error) => Err(error.into()),
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|e| e.code().as_deref() == Some("23505"))
}
