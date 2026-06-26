//! Homepage handler.
//!
//! - `GET /`     — routing logic: redirect to `/login`, `/onboard`, or
//!                 render the homepage.
//! - `GET /home` — render the homepage (federation list). If the user has
//!                 no signer yet, redirects to `/onboard`.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Serialize;
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{self, AuthUser};
use crate::db;
use crate::error::AppError;
use crate::models::SignerRow;

/// Homepage template.
#[derive(Template, WebTemplate)]
#[template(path = "home.html")]
struct HomeTemplate {
    /// Logged-in user's email.
    email: String,
    /// Federation versions the user belongs to that are live (active) or
    /// historic (superseded) — i.e. not pending.
    federations: Vec<FederationView>,
    /// Pending versions the user has been **added** to by an in-flight migration
    /// (requirement 2's "invitations"): the user is already a member of the
    /// pending version, awaiting the migration transaction's signatures.
    invitations: Vec<FederationView>,
    /// Onboarded signers (always non-empty when we render this template).
    signers: Vec<SignerView>,
}

/// View-model for one row in the federations list.
#[derive(Debug, Serialize)]
struct FederationView {
    /// Federation (version) id — used to build the `/federations/:id` link.
    id: Uuid,
    /// Lineage id — links to the version-history / relay view.
    lineage_id: Uuid,
    label: String,
    threshold: i32,
    total_signers: i32,
    /// Position in the lineage (`0` = original).
    version_index: i32,
    /// Lifecycle status: `active` | `superseded` | `pending` | `abandoned`.
    status: String,
    created_at: String,
}

/// View-model for one onboarded signer.
#[derive(Debug, Serialize)]
struct SignerView {
    label: String,
    fingerprint: String,
    derivation_path: String,
    device_type: String,
    network: String,
    xpub_truncated: String,
    created_at: String,
}

impl From<SignerRow> for SignerView {
    fn from(row: SignerRow) -> Self {
        let label = row.label.unwrap_or_else(|| "Trezor".to_string());
        let xpub_truncated = truncate_middle(&row.xpub, 14, 12);
        Self {
            label,
            fingerprint: row.fingerprint,
            derivation_path: row.derivation_path,
            device_type: row.device_type,
            network: row.network,
            xpub_truncated,
            created_at: row.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        }
    }
}

fn truncate_middle(s: &str, head: usize, tail: usize) -> String {
    if s.chars().count() <= head + tail + 3 {
        return s.to_string();
    }
    let head_part: String = s.chars().take(head).collect();
    let tail_part: String = s
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head_part}…{tail_part}")
}

/// `GET /`
///
/// Pure router:
/// - no session   -> `/login`
/// - no signers   -> `/onboard`
/// - otherwise    -> `/home`
pub async fn root(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Response, AppError> {
    let user = match auth::current_user(&session, &state.db).await {
        Ok(Some(u)) => u,
        Ok(None) => return Ok(Redirect::to("/login").into_response()),
        Err(e) => {
            return Err(match e {
                auth::AuthLookupError::Session(e) => AppError::Session(e),
                auth::AuthLookupError::Sqlx(e) => AppError::Sqlx(e),
            });
        }
    };

    if db::user_has_signer(&state.db, user.id).await? {
        Ok(Redirect::to("/home").into_response())
    } else {
        Ok(Redirect::to("/onboard").into_response())
    }
}

/// `GET /home`
pub async fn home(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Response, AppError> {
    let signer_rows = db::list_signers_for_user(&state.db, user.id).await?;
    if signer_rows.is_empty() {
        return Ok(Redirect::to("/onboard").into_response());
    }

    // The user's memberships span every version they belong to (any status).
    // Split pending versions out as "invitations" (req 2): they were added by an
    // in-flight migration that hasn't been signed/broadcast yet.
    let mut federations = Vec::new();
    let mut invitations = Vec::new();
    for f in db::list_federations_for_user(&state.db, user.id).await? {
        let view = FederationView {
            id: f.id,
            lineage_id: f.lineage_id,
            label: f.label,
            threshold: f.threshold,
            total_signers: f.total_signers,
            version_index: f.version_index,
            status: f.status.clone(),
            created_at: f.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        };
        if f.status == "pending" {
            invitations.push(view);
        } else {
            federations.push(view);
        }
    }

    let signers = signer_rows.into_iter().map(SignerView::from).collect();

    Ok(HomeTemplate {
        email: user.email,
        federations,
        invitations,
        signers,
    }
    .into_response())
}
