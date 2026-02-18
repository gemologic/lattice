use std::fmt::Write;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::Sha256;
use tokio::time::MissedTickBehavior;

use crate::db::models::{SystemEventRecord, WebhookRecord};
use crate::db::queries;
use crate::error::AppResult;
use crate::state::AppState;

const DISPATCH_POLL_INTERVAL_MS: u64 = 1000;
const RETRY_DELAY_SECONDS: u64 = 30;
const MAX_RETRY_QUEUE: usize = 512;
const DISPATCH_BATCH_SIZE: i64 = 100;

#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub project: String,
    pub task_id: Option<String>,
    pub task_number: Option<i64>,
    pub task_display_key: Option<String>,
    pub actor: String,
    pub detail: Value,
    pub created_at: String,
}

#[derive(Debug, Clone)]
struct PendingDelivery {
    webhook: WebhookRecord,
    payload: WebhookPayload,
    due_at: Instant,
}

pub fn spawn_dispatcher(state: AppState) {
    tokio::spawn(async move {
        if let Err(error) = run_dispatcher(state).await {
            tracing::error!(error = ?error, "webhook dispatcher terminated");
        }
    });
}

pub async fn send_test_webhook(
    state: &AppState,
    project_slug: &str,
    webhook_id: &str,
) -> AppResult<()> {
    let webhook = queries::get_project_webhook(&state.db, project_slug, webhook_id).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to build webhook client")?;
    let payload = WebhookPayload {
        event: "test".to_string(),
        project: project_slug.to_string(),
        task_id: None,
        task_number: None,
        task_display_key: None,
        actor: "system".to_string(),
        detail: json!({ "message": "test webhook from lattice" }),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };

    deliver_webhook(&client, &webhook, &payload)
        .await
        .context("failed to deliver test webhook")?;
    Ok(())
}

async fn run_dispatcher(state: AppState) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to build webhook client")?;

    let (mut last_created_at, mut last_event_id) =
        match queries::latest_system_event_cursor(&state.db, &[]).await {
            Ok(Some((created_at, event_id))) => (Some(created_at), Some(event_id)),
            Ok(None) => (None, None),
            Err(error) => {
                tracing::error!(error = ?error, "failed to initialize webhook dispatcher cursor");
                (None, None)
            }
        };
    let mut retry_queue: Vec<PendingDelivery> = Vec::new();
    let mut interval = tokio::time::interval(Duration::from_millis(DISPATCH_POLL_INTERVAL_MS));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        process_retry_queue(&client, &mut retry_queue).await;

        let events = match queries::list_system_events(
            &state.db,
            &[],
            last_created_at.as_deref(),
            last_event_id.as_deref(),
            DISPATCH_BATCH_SIZE,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(error = ?error, "failed to query task events for webhook dispatch");
                continue;
            }
        };

        for event in events {
            last_created_at = Some(event.created_at.clone());
            last_event_id = Some(event.id.clone());
            dispatch_event(&state, &client, &mut retry_queue, event).await;
        }
    }
}

async fn dispatch_event(
    state: &AppState,
    client: &reqwest::Client,
    retry_queue: &mut Vec<PendingDelivery>,
    event: SystemEventRecord,
) {
    let payload = payload_from_system_event(event);
    let webhooks = match queries::list_active_project_webhooks(&state.db, &payload.project).await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(error = ?error, "failed to load project webhooks for dispatch");
            return;
        }
    };

    for webhook in webhooks {
        if !webhook_subscribed_to_event(&webhook, &payload.event) {
            continue;
        }

        if let Err(error) = deliver_webhook(client, &webhook, &payload).await {
            tracing::warn!(
                error = ?error,
                webhook_id = %webhook.id,
                event = %payload.event,
                "webhook delivery failed, scheduling one retry"
            );
            schedule_retry(retry_queue, webhook, payload.clone());
        }
    }
}

async fn process_retry_queue(client: &reqwest::Client, retry_queue: &mut Vec<PendingDelivery>) {
    let now = Instant::now();
    let mut still_pending = Vec::new();

    for pending in retry_queue.drain(..) {
        if pending.due_at > now {
            still_pending.push(pending);
            continue;
        }

        if let Err(error) = deliver_webhook(client, &pending.webhook, &pending.payload).await {
            tracing::warn!(
                error = ?error,
                webhook_id = %pending.webhook.id,
                event = %pending.payload.event,
                "webhook retry delivery failed and will be dropped"
            );
        }
    }

    *retry_queue = still_pending;
}

fn schedule_retry(
    retry_queue: &mut Vec<PendingDelivery>,
    webhook: WebhookRecord,
    payload: WebhookPayload,
) {
    if retry_queue.len() >= MAX_RETRY_QUEUE {
        tracing::warn!(
            webhook_id = %webhook.id,
            event = %payload.event,
            "retry queue full, dropping webhook retry"
        );
        return;
    }

    retry_queue.push(PendingDelivery {
        webhook,
        payload,
        due_at: Instant::now() + Duration::from_secs(RETRY_DELAY_SECONDS),
    });
}

fn payload_from_system_event(event: SystemEventRecord) -> WebhookPayload {
    let detail = serde_json::from_str::<Value>(&event.detail)
        .unwrap_or_else(|_| Value::String(event.detail.clone()));
    WebhookPayload {
        event: event.action,
        project: event.project_slug.clone(),
        task_id: event.task_id,
        task_number: event.task_number,
        task_display_key: event
            .task_number
            .map(|task_number| queries::display_key(&event.project_slug, task_number)),
        actor: event.actor,
        detail,
        created_at: event.created_at,
    }
}

fn webhook_subscribed_to_event(webhook: &WebhookRecord, event: &str) -> bool {
    match queries::parse_webhook_events(&webhook.events) {
        Ok(events) => events.iter().any(|candidate| candidate == event),
        Err(error) => {
            tracing::warn!(
                error = ?error,
                webhook_id = %webhook.id,
                "webhook has invalid events config and will be skipped"
            );
            false
        }
    }
}

async fn deliver_webhook(
    client: &reqwest::Client,
    webhook: &WebhookRecord,
    payload: &WebhookPayload,
) -> anyhow::Result<()> {
    let body = webhook_body(webhook, payload)?;

    let mut request = client
        .post(&webhook.url)
        .header("Content-Type", "application/json")
        .body(body.clone());

    if webhook.platform == "generic" {
        if let Some(secret) = webhook
            .secret
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            request = request.header("X-Lattice-Signature", hmac_signature(secret, &body)?);
        }
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("request failed for webhook '{}'", webhook.id))?;

    if response.status().is_success() {
        return Ok(());
    }

    Err(anyhow!(
        "webhook '{}' returned status {}",
        webhook.id,
        response.status()
    ))
}

fn webhook_body(webhook: &WebhookRecord, payload: &WebhookPayload) -> anyhow::Result<Vec<u8>> {
    let body = match webhook.platform.as_str() {
        "slack" => slack_payload(payload),
        "discord" => discord_payload(payload),
        _ => {
            serde_json::to_value(payload).context("failed to serialize generic webhook payload")?
        }
    };

    serde_json::to_vec(&body).context("failed to encode webhook payload")
}

fn slack_payload(payload: &WebhookPayload) -> Value {
    let task_label = payload
        .task_display_key
        .as_ref()
        .map_or("task".to_string(), ToOwned::to_owned);
    let detail = compact_json(&payload.detail);

    json!({
        "text": format!("[{}] {} {}", payload.project, payload.event, task_label),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("*{}* `{}` in *{}*", payload.event, task_label, payload.project)
                }
            },
            {
                "type": "context",
                "elements": [
                    {
                        "type": "mrkdwn",
                        "text": format!("actor: {} • {}", payload.actor, payload.created_at)
                    }
                ]
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": detail
                }
            }
        ]
    })
}

fn discord_payload(payload: &WebhookPayload) -> Value {
    let task_label = payload
        .task_display_key
        .as_ref()
        .map_or("task".to_string(), ToOwned::to_owned);

    json!({
        "embeds": [
            {
                "title": format!("{} • {}", payload.event, task_label),
                "description": compact_json(&payload.detail),
                "color": discord_color_for_event(&payload.event),
                "footer": {
                    "text": format!("{} • {}", payload.project, payload.actor),
                },
                "timestamp": payload.created_at,
            }
        ]
    })
}

fn compact_json(value: &Value) -> String {
    if value.is_null() {
        return "{}".to_string();
    }

    match serde_json::to_string(value) {
        Ok(serialized) => serialized,
        Err(_) => "{}".to_string(),
    }
}

fn discord_color_for_event(event: &str) -> u32 {
    match event {
        "task.created" => 0x7A3FFF,
        "task.moved" => 0x4F9DFF,
        "task.deleted" => 0xC94C4C,
        "task.review_state_changed" => 0xE0A341,
        "question.created" => 0xF0C54A,
        "question.resolved" => 0x4BB47B,
        "spec.updated" => 0x9A65C7,
        "goal.updated" => 0x74BBD6,
        _ => 0x8A8A8A,
    }
}

fn hmac_signature(secret: &str, body: &[u8]) -> anyhow::Result<String> {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).context("failed to init hmac signer")?;
    mac.update(body);
    let bytes = mac.finalize().into_bytes();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    Ok(format!("sha256={encoded}"))
}
