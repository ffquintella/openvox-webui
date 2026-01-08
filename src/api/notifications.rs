//! Notification API endpoints

use crate::middleware::AuthUser;
use crate::models::{
    BulkMarkReadRequest, CreateNotificationRequest, MarkNotificationReadRequest,
    NotificationQuery,
};
use crate::utils::AppError;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        Json, Sse,
    },
    routing::{delete, get, post, put},
    Extension, Router,
};
use futures::stream::Stream;
use serde_json::json;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

/// Create notification routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notifications).post(create_notification))
        .route("/stats", get(get_notification_stats))
        .route("/mark-all-read", post(mark_all_read))
        .route("/bulk-mark-read", post(bulk_mark_read))
        .route("/stream", get(notification_stream))
        .route("/:id", get(get_notification))
        .route("/:id/read", put(mark_notification_read))
        .route("/:id/dismiss", post(dismiss_notification))
        .route("/:id", delete(delete_notification_handler))
}

/// List notifications
async fn list_notifications(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();
    let organization_id = auth_user.organization_id.to_string();

    let notifications = state
        .notification_service
        .get_notifications(&user_id, Some(&organization_id), query)
        .await?;

    Ok(Json(json!({ "notifications": notifications })))
}

/// Create a new notification
async fn create_notification(
    State(state): State<AppState>,
    Json(req): Json<CreateNotificationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let notification = state.notification_service.create_notification(req).await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "notification": notification })),
    ))
}

/// Get a single notification
async fn get_notification(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    let notification = state
        .notification_service
        .get_notification(&id, &user_id)
        .await?;

    Ok(Json(json!({ "notification": notification })))
}

/// Mark notification as read/unread
async fn mark_notification_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
    Json(req): Json<MarkNotificationReadRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    let notification = state
        .notification_service
        .mark_as_read(&id, &user_id, req.read)
        .await?;

    Ok(Json(json!({ "notification": notification })))
}

/// Mark all notifications as read
async fn mark_all_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    let count = state
        .notification_service
        .mark_all_as_read(&user_id)
        .await?;

    Ok(Json(json!({
        "success": true,
        "count": count,
        "message": format!("Marked {} notifications as read", count)
    })))
}

/// Bulk mark notifications as read/unread
async fn bulk_mark_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<BulkMarkReadRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    let count = state
        .notification_service
        .bulk_mark_read(&user_id, req)
        .await?;

    Ok(Json(json!({
        "success": true,
        "count": count,
        "message": format!("Updated {} notifications", count)
    })))
}

/// Dismiss a notification
async fn dismiss_notification(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    state
        .notification_service
        .dismiss_notification(&id, &user_id)
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "Notification dismissed"
    })))
}

/// Delete a notification
async fn delete_notification_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();

    state
        .notification_service
        .delete_notification(&id, &user_id)
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "Notification deleted"
    })))
}

/// Get notification statistics
async fn get_notification_stats(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.to_string();
    let organization_id = auth_user.organization_id.to_string();

    let stats = state
        .notification_service
        .get_stats(&user_id, Some(&organization_id))
        .await?;

    Ok(Json(json!({ "stats": stats })))
}

/// Server-Sent Events stream for real-time notifications
async fn notification_stream(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = auth_user.id.to_string();
    let receiver = state.notification_service.subscribe();

    use futures::StreamExt as FuturesStreamExt;

    let stream = FuturesStreamExt::filter_map(BroadcastStream::new(receiver), move |result| {
            let user_id = user_id.clone();
            Box::pin(async move {
                match result {
                    Ok(event) => {
                        // Filter events for this user
                        let should_send = match &event {
                            crate::services::NotificationEvent::New(notification) => {
                                notification.user_id == user_id
                            }
                            crate::services::NotificationEvent::Updated(notification) => {
                                notification.user_id == user_id
                            }
                            crate::services::NotificationEvent::Deleted(_) => true,
                            crate::services::NotificationEvent::BulkRead(_) => true,
                        };

                        if should_send {
                            // Convert to SSE event
                            let event_data = match event {
                                crate::services::NotificationEvent::New(notification) => {
                                    json!({
                                        "type": "new",
                                        "notification": notification
                                    })
                                }
                                crate::services::NotificationEvent::Updated(notification) => {
                                    json!({
                                        "type": "updated",
                                        "notification": notification
                                    })
                                }
                                crate::services::NotificationEvent::Deleted(id) => {
                                    json!({
                                        "type": "deleted",
                                        "notification_id": id
                                    })
                                }
                                crate::services::NotificationEvent::BulkRead(ids) => {
                                    json!({
                                        "type": "bulk_read",
                                        "notification_ids": ids
                                    })
                                }
                            };

                            Some(Ok::<_, Infallible>(
                                Event::default().json_data(event_data).unwrap(),
                            ))
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            })
        });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
