use anyhow::{Context, Result, bail};
use chrono::Local;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::domain::token::random_hex_token;

pub struct NewSignIn {
    pub name: String,
    pub rank: Option<String>,
    pub squadron: String,
    pub reason_for_visit: String,
    pub ticket_number: Option<String>,
}

/// Starts a sign-in: writes the temporary `pending_sign_ins` row and
/// returns the session token that ties every later step (service
/// selection, submission, eventual removal) back to it.
pub async fn create(pool: &SqlitePool, input: NewSignIn) -> Result<String> {
    let token = random_hex_token();
    let now = Local::now();

    sqlx::query(
        "INSERT INTO pending_sign_ins
            (session_token, name, rank, squadron, reason_for_visit, ticket_number, time_in, sign_in_date, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&token)
    .bind(&input.name)
    .bind(&input.rank)
    .bind(&input.squadron)
    .bind(&input.reason_for_visit)
    .bind(&input.ticket_number)
    .bind(now.format("%H:%M").to_string())
    .bind(now.format("%Y-%m-%d").to_string())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await
    .context("failed to save sign-in")?;

    Ok(token)
}

/// Whether `token` names a sign-in still in progress (exists in
/// `pending_sign_ins` — hasn't been removed from the queue yet).
pub async fn pending_exists(pool: &SqlitePool, token: &str) -> Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM pending_sign_ins WHERE session_token = ?")
        .bind(token)
        .fetch_optional(pool)
        .await
        .context("failed to look up sign-in session")?;
    Ok(row.is_some())
}

/// Attaches the Issue Receipt service's submitted fields to a pending
/// sign-in and marks that service as picked. This is the moment a
/// sign-in becomes visible on the queue — nothing else does it.
pub async fn submit_issue_receipt(pool: &SqlitePool, token: &str, fields_json: &str) -> Result<()> {
    if !pending_exists(pool, token).await? {
        bail!("no pending sign-in for this session");
    }

    let mut tx = pool.begin().await.context("failed to start transaction")?;

    sqlx::query(
        "INSERT INTO pending_issue_receipt (session_token, fields_json, submitted_at) VALUES (?, ?, ?)
         ON CONFLICT (session_token) DO UPDATE SET fields_json = excluded.fields_json, submitted_at = excluded.submitted_at",
    )
    .bind(token)
    .bind(fields_json)
    .bind(Local::now().to_rfc3339())
    .execute(&mut *tx)
    .await
    .context("failed to save issue-receipt fields")?;

    sqlx::query("UPDATE pending_sign_ins SET service_key = 'temporary_issue_receipt' WHERE session_token = ?")
        .bind(token)
        .execute(&mut *tx)
        .await
        .context("failed to mark service selected")?;

    tx.commit().await.context("failed to commit issue-receipt submission")?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueEntry {
    pub session_token: String,
    pub display_name: String,
    pub reason_for_visit: String,
    pub department: String,
}

/// "Rank Name" when a rank was given (military/civilian staff), just
/// "Name" when it wasn't (local nationals and contractors typically
/// have none) — same convention the old per-portal queues used, now
/// computed from two real fields instead of one free-text one.
fn display_name(name: &str, rank: Option<&str>) -> String {
    match rank {
        Some(rank) if !rank.trim().is_empty() => format!("{rank} {name}"),
        _ => name.to_string(),
    }
}

#[derive(sqlx::FromRow)]
struct QueueRow {
    session_token: String,
    name: String,
    rank: Option<String>,
    reason_for_visit: String,
    department: String,
}

/// Everyone currently on the queue — every pending sign-in that has
/// also picked a service. `department` filters to one portal's view;
/// `None` is the general/admin view across all departments.
pub async fn list_queue(pool: &SqlitePool, department: Option<&str>) -> Result<Vec<QueueEntry>> {
    let rows: Vec<QueueRow> = sqlx::query_as(
        "SELECT p.session_token, p.name, p.rank, p.reason_for_visit, s.department
         FROM pending_sign_ins p
         JOIN services s ON s.key = p.service_key
         WHERE p.service_key IS NOT NULL AND (? IS NULL OR s.department = ?)
         ORDER BY p.created_at ASC",
    )
    .bind(department)
    .bind(department)
    .fetch_all(pool)
    .await
    .context("failed to list queue")?;

    Ok(rows
        .into_iter()
        .map(|r| QueueEntry {
            session_token: r.session_token,
            display_name: display_name(&r.name, r.rank.as_deref()),
            reason_for_visit: r.reason_for_visit,
            department: r.department,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct PendingSignInRow {
    session_token: String,
    name: String,
    rank: Option<String>,
    squadron: String,
    reason_for_visit: String,
    ticket_number: Option<String>,
    time_in: String,
    sign_in_date: String,
    service_key: Option<String>,
}

/// Removes a customer from the queue (appointment complete): archives
/// their sign-in and service data into the permanent tables, then
/// deletes the temporary rows. Returns `false` if there's no such
/// queued (service-submitted) session — e.g. already removed.
pub async fn remove_from_queue(pool: &SqlitePool, token: &str, completed_by_portal: &str) -> Result<bool> {
    let mut tx = pool.begin().await.context("failed to start transaction")?;

    let row: Option<PendingSignInRow> = sqlx::query_as(
        "SELECT session_token, name, rank, squadron, reason_for_visit, ticket_number, time_in, sign_in_date, service_key
         FROM pending_sign_ins WHERE session_token = ? AND service_key IS NOT NULL",
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to look up queue entry")?;

    let Some(row) = row else {
        return Ok(false);
    };
    let service_key = row.service_key.clone().expect("filtered by service_key IS NOT NULL");
    let completed_at = Local::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO sign_ins
            (session_token, name, rank, squadron, reason_for_visit, ticket_number, time_in, sign_in_date,
             service_key, completed_at, completed_by_portal)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.session_token)
    .bind(&row.name)
    .bind(&row.rank)
    .bind(&row.squadron)
    .bind(&row.reason_for_visit)
    .bind(&row.ticket_number)
    .bind(&row.time_in)
    .bind(&row.sign_in_date)
    .bind(&service_key)
    .bind(&completed_at)
    .bind(completed_by_portal)
    .execute(&mut *tx)
    .await
    .context("failed to archive sign-in")?;

    // Each service's temp data gets copied into its own permanent
    // table. Only one service exists today; a second one adds another
    // arm here, not a restructure.
    match service_key.as_str() {
        "temporary_issue_receipt" => {
            sqlx::query(
                "INSERT INTO issue_receipts (session_token, fields_json, submitted_at)
                 SELECT session_token, fields_json, submitted_at FROM pending_issue_receipt WHERE session_token = ?",
            )
            .bind(token)
            .execute(&mut *tx)
            .await
            .context("failed to archive issue-receipt fields")?;
        }
        other => bail!("no archiving logic for service '{other}'"),
    }

    sqlx::query("DELETE FROM pending_sign_ins WHERE session_token = ?")
        .bind(token)
        .execute(&mut *tx)
        .await
        .context("failed to clear pending sign-in")?;

    tx.commit().await.context("failed to commit queue removal")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::db::connect_in_memory;

    fn sample_sign_in() -> NewSignIn {
        NewSignIn {
            name: "Jane Doe".to_string(),
            rank: Some("SSgt".to_string()),
            squadron: "375th CS".to_string(),
            reason_for_visit: "Laptop won't boot".to_string(),
            ticket_number: None,
        }
    }

    #[tokio::test]
    async fn create_then_pending_exists() {
        let pool = connect_in_memory().await;
        let token = create(&pool, sample_sign_in()).await.unwrap();
        assert!(pending_exists(&pool, &token).await.unwrap());
        assert!(!pending_exists(&pool, "bogus").await.unwrap());
    }

    #[tokio::test]
    async fn not_on_queue_until_service_submitted() {
        let pool = connect_in_memory().await;
        let token = create(&pool, sample_sign_in()).await.unwrap();

        assert!(list_queue(&pool, None).await.unwrap().is_empty());

        submit_issue_receipt(&pool, &token, r#"{"ticket_number":"TCK-1"}"#).await.unwrap();

        let queue = list_queue(&pool, None).await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].display_name, "SSgt Jane Doe");
        assert_eq!(queue[0].department, "cst");
    }

    #[tokio::test]
    async fn display_name_omits_rank_when_absent() {
        let pool = connect_in_memory().await;
        let token = create(
            &pool,
            NewSignIn {
                name: "Local Contractor".to_string(),
                rank: None,
                squadron: "N/A".to_string(),
                reason_for_visit: "Badge issue".to_string(),
                ticket_number: None,
            },
        )
        .await
        .unwrap();
        submit_issue_receipt(&pool, &token, "{}").await.unwrap();

        let queue = list_queue(&pool, None).await.unwrap();
        assert_eq!(queue[0].display_name, "Local Contractor");
    }

    #[tokio::test]
    async fn list_queue_filters_by_department() {
        let pool = connect_in_memory().await;
        let token = create(&pool, sample_sign_in()).await.unwrap();
        submit_issue_receipt(&pool, &token, "{}").await.unwrap();

        assert_eq!(list_queue(&pool, Some("cst")).await.unwrap().len(), 1);
        assert!(list_queue(&pool, Some("neets")).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn remove_archives_and_clears_pending() {
        let pool = connect_in_memory().await;
        let token = create(&pool, sample_sign_in()).await.unwrap();
        submit_issue_receipt(&pool, &token, r#"{"remarks":"expedite"}"#).await.unwrap();

        let removed = remove_from_queue(&pool, &token, "cst").await.unwrap();
        assert!(removed);

        assert!(!pending_exists(&pool, &token).await.unwrap());
        assert!(list_queue(&pool, None).await.unwrap().is_empty());

        let permanent: (String,) = sqlx::query_as("SELECT completed_by_portal FROM sign_ins WHERE session_token = ?")
            .bind(&token)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(permanent.0, "cst");

        let archived_fields: (String,) =
            sqlx::query_as("SELECT fields_json FROM issue_receipts WHERE session_token = ?")
                .bind(&token)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(archived_fields.0, r#"{"remarks":"expedite"}"#);
    }

    #[tokio::test]
    async fn removing_a_session_not_on_the_queue_is_a_no_op() {
        let pool = connect_in_memory().await;
        let token = create(&pool, sample_sign_in()).await.unwrap();
        // Never submitted a service, so not queued yet.
        let removed = remove_from_queue(&pool, &token, "cst").await.unwrap();
        assert!(!removed);
        assert!(pending_exists(&pool, &token).await.unwrap());
    }
}
