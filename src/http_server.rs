use axum::{
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use std::net::SocketAddr;
use crate::database::{Database, DbEmail, DbError, DbGroupMember, DbUser, DbGroup};
use crate::scim_schemas::{
    ScimEmail, ScimError, ScimGroup, ScimGroupMember, ScimListResponse, ScimMeta, ScimName, ScimUser,
    ScimUserGroup, GROUP_SCHEMA, LIST_RESPONSE_SCHEMA, USER_SCHEMA,
};
use crate::settings::Settings;

// Shared Application State
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub settings: Settings,
}

// Custom response type to force the standard SCIM header: Content-Type: application/scim+json; charset=utf-8
pub struct ScimResponse<T>(pub StatusCode, pub T);

impl<T: serde::Serialize> IntoResponse for ScimResponse<T> {
    fn into_response(self) -> Response {
        let body = match serde_json::to_string(&self.1) {
            Ok(b) => b,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize response: {}", e),
                )
                    .into_response()
            }
        };

        Response::builder()
            .status(self.0)
            .header(header::CONTENT_TYPE, "application/scim+json; charset=utf-8")
            .body(axum::body::Body::from(body))
            .unwrap()
    }
}

// Dynamic base URL resolver based on configuration
fn get_base_url(settings: &Settings) -> String {
    format!("http://{}:{}", settings.http_host, settings.http_port)
}

// Convert DbError to ScimResponse
fn handle_db_error(err: DbError) -> ScimResponse<ScimError> {
    match err {
        DbError::UserNotFound(id) => ScimResponse(
            StatusCode::NOT_FOUND,
            ScimError::new("404", None, Some(format!("User not found: {}", id))),
        ),
        DbError::GroupNotFound(id) => ScimResponse(
            StatusCode::NOT_FOUND,
            ScimError::new("404", None, Some(format!("Group not found: {}", id))),
        ),
        DbError::UserAlreadyExists(username) => ScimResponse(
            StatusCode::CONFLICT,
            ScimError::new(
                "409",
                Some("uniqueness".to_string()),
                Some(format!("Username already exists: {}", username)),
            ),
        ),
        DbError::GroupAlreadyExists(name) => ScimResponse(
            StatusCode::CONFLICT,
            ScimError::new(
                "409",
                Some("uniqueness".to_string()),
                Some(format!("Group name already exists: {}", name)),
            ),
        ),
    }
}

// --- Mapper Helpers ---

fn db_user_to_scim(user: DbUser, base_url: &str, groups: Vec<DbGroup>) -> ScimUser {
    let id = user.id.clone();
    let location = format!("{}/scim/v2/Users/{}", base_url, id);
    let user_groups = if groups.is_empty() {
        None
    } else {
        Some(
            groups
                .into_iter()
                .map(|g| ScimUserGroup {
                    value: g.id.clone(),
                    r#ref: format!("{}/scim/v2/Groups/{}", base_url, g.id),
                    display: Some(g.display_name),
                })
                .collect(),
        )
    };

    ScimUser {
        schemas: vec![USER_SCHEMA.to_string()],
        id: Some(id),
        external_id: user.external_id,
        user_name: user.user_name,
        name: Some(ScimName {
            formatted: user.formatted_name,
            family_name: user.family_name,
            given_name: user.given_name,
        }),
        display_name: user.display_name,
        emails: Some(
            user.emails
                .into_iter()
                .map(|e| ScimEmail {
                    value: e.value,
                    r#type: e.r#type,
                    primary: Some(e.primary),
                })
                .collect(),
        ),
        active: Some(user.active),
        groups: user_groups,
        meta: Some(ScimMeta {
            resource_type: "User".to_string(),
            created: user.created.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            last_modified: user.last_modified.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            location,
            version: None,
        }),
    }
}

fn db_group_to_scim(group: DbGroup, base_url: &str) -> ScimGroup {
    let id = group.id.clone();
    let location = format!("{}/scim/v2/Groups/{}", base_url, id);
    let members = if group.members.is_empty() {
        None
    } else {
        Some(
            group
                .members
                .into_iter()
                .map(|m| ScimGroupMember {
                    value: m.value.clone(),
                    r#ref: Some(format!("{}/scim/v2/Users/{}", base_url, m.value)),
                    display: m.display,
                })
                .collect(),
        )
    };

    ScimGroup {
        schemas: vec![GROUP_SCHEMA.to_string()],
        id: Some(id),
        display_name: group.display_name,
        members,
        meta: Some(ScimMeta {
            resource_type: "Group".to_string(),
            created: group.created.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            last_modified: group.last_modified.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            location,
            version: None,
        }),
    }
}

// --- Query Struct ---

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ScimQuery {
    filter: Option<String>,
    #[serde(rename = "startIndex")]
    start_index: Option<usize>,
    count: Option<usize>,
}

// --- Handlers ---

// Users API
async fn get_users(
    State(state): State<AppState>,
    Query(query): Query<ScimQuery>,
) -> impl IntoResponse {
    let start_index = query.start_index.unwrap_or(1);
    let count = query.count.unwrap_or(20);
    let base_url = get_base_url(&state.settings);

    let (users, total) = state.db.list_users(query.filter, start_index, count).await;

    let mut scim_users = Vec::new();
    for u in users {
        let u_groups = state.db.get_user_groups(&u.id).await;
        scim_users.push(db_user_to_scim(u, &base_url, u_groups));
    }

    let response = ScimListResponse {
        schemas: vec![LIST_RESPONSE_SCHEMA.to_string()],
        total_results: total,
        items_per_page: scim_users.len(),
        start_index,
        resources: scim_users,
    };

    ScimResponse(StatusCode::OK, response)
}

async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);
    if let Some(user) = state.db.get_user(&id).await {
        let u_groups = state.db.get_user_groups(&id).await;
        let scim_user = db_user_to_scim(user, &base_url, u_groups);
        ScimResponse(StatusCode::OK, scim_user).into_response()
    } else {
        let err = ScimError::new("404", None, Some(format!("User not found: {}", id)));
        ScimResponse(StatusCode::NOT_FOUND, err).into_response()
    }
}

async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<ScimUser>,
) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);

    let emails = payload.emails.unwrap_or_default().into_iter().map(|e| DbEmail {
        value: e.value,
        r#type: e.r#type,
        primary: e.primary.unwrap_or(false),
    }).collect();

    let name = payload.name.unwrap_or(ScimName { formatted: None, family_name: None, given_name: None });

    let active = payload.active.unwrap_or(true);

    match state.db.create_user(
        payload.external_id,
        payload.user_name,
        name.formatted,
        name.family_name,
        name.given_name,
        payload.display_name,
        emails,
        active,
    ).await {
        Ok(user) => {
            let scim_user = db_user_to_scim(user, &base_url, vec![]);
            ScimResponse(StatusCode::CREATED, scim_user).into_response()
        }
        Err(e) => handle_db_error(e).into_response(),
    }
}

async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ScimUser>,
) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);

    let emails = payload.emails.unwrap_or_default().into_iter().map(|e| DbEmail {
        value: e.value,
        r#type: e.r#type,
        primary: e.primary.unwrap_or(false),
    }).collect();

    let name = payload.name.unwrap_or(ScimName { formatted: None, family_name: None, given_name: None });

    let active = payload.active.unwrap_or(true);

    match state.db.update_user(
        &id,
        payload.external_id,
        payload.user_name,
        name.formatted,
        name.family_name,
        name.given_name,
        payload.display_name,
        emails,
        active,
    ).await {
        Ok(user) => {
            let u_groups = state.db.get_user_groups(&id).await;
            let scim_user = db_user_to_scim(user, &base_url, u_groups);
            ScimResponse(StatusCode::OK, scim_user).into_response()
        }
        Err(e) => handle_db_error(e).into_response(),
    }
}

async fn delete_user(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.db.delete_user(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => handle_db_error(e).into_response(),
    }
}

// Groups API
async fn get_groups(
    State(state): State<AppState>,
    Query(query): Query<ScimQuery>,
) -> impl IntoResponse {
    let start_index = query.start_index.unwrap_or(1);
    let count = query.count.unwrap_or(20);
    let base_url = get_base_url(&state.settings);

    let (groups, total) = state.db.list_groups(query.filter, start_index, count).await;

    let scim_groups: Vec<ScimGroup> = groups
        .into_iter()
        .map(|g| db_group_to_scim(g, &base_url))
        .collect();

    let response = ScimListResponse {
        schemas: vec![LIST_RESPONSE_SCHEMA.to_string()],
        total_results: total,
        items_per_page: scim_groups.len(),
        start_index,
        resources: scim_groups,
    };

    ScimResponse(StatusCode::OK, response)
}

async fn get_group(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);
    if let Some(group) = state.db.get_group(&id).await {
        let scim_group = db_group_to_scim(group, &base_url);
        ScimResponse(StatusCode::OK, scim_group).into_response()
    } else {
        let err = ScimError::new("404", None, Some(format!("Group not found: {}", id)));
        ScimResponse(StatusCode::NOT_FOUND, err).into_response()
    }
}

async fn create_group(
    State(state): State<AppState>,
    Json(payload): Json<ScimGroup>,
) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);

    let db_members: Vec<DbGroupMember> = payload
        .members
        .unwrap_or_default()
        .into_iter()
        .map(|m| DbGroupMember {
            value: m.value,
            display: m.display,
        })
        .collect();

    match state.db.create_group(payload.display_name, db_members).await {
        Ok(group) => {
            let scim_group = db_group_to_scim(group, &base_url);
            ScimResponse(StatusCode::CREATED, scim_group).into_response()
        }
        Err(e) => handle_db_error(e).into_response(),
    }
}

async fn update_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ScimGroup>,
) -> impl IntoResponse {
    let base_url = get_base_url(&state.settings);

    let db_members: Vec<DbGroupMember> = payload
        .members
        .unwrap_or_default()
        .into_iter()
        .map(|m| DbGroupMember {
            value: m.value,
            display: m.display,
        })
        .collect();

    match state.db.update_group(&id, payload.display_name, db_members).await {
        Ok(group) => {
            let scim_group = db_group_to_scim(group, &base_url);
            ScimResponse(StatusCode::OK, scim_group).into_response()
        }
        Err(e) => handle_db_error(e).into_response(),
    }
}

async fn delete_group(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.db.delete_group(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => handle_db_error(e).into_response(),
    }
}

// --- Auth Middleware ---

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, Response> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    if let Some(auth_val) = auth_header {
        if auth_val.starts_with("Bearer ") {
            let token = &auth_val["Bearer ".len()..];
            if token == state.settings.auth_token {
                return Ok(next.run(req).await);
            }
        }
    }

    let err = ScimError::new(
        "401",
        None,
        Some("Unauthorized: Invalid or missing Bearer token".to_string()),
    );
    Err(ScimResponse(StatusCode::UNAUTHORIZED, err).into_response())
}

// --- Server Launcher ---

pub async fn run(state: AppState) {
    let http_addr = format!("{}:{}", state.settings.http_host, state.settings.http_port);
    let addr: SocketAddr = http_addr.parse().expect("Failed to parse http listener address");

    // Router and routes definition
    let scim_routes = Router::new()
        .route("/Users", get(get_users).post(create_user))
        .route("/Users/:id", get(get_user).put(update_user).delete(delete_user))
        .route("/Groups", get(get_groups).post(create_group))
        .route("/Groups/:id", get(get_group).put(update_group).delete(delete_group))
        // Layer standard SCIM authentication middleware
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .nest("/scim/v2", scim_routes.clone())
        // Also nest in root in case the client requests /v2/ or just /Users
        .nest("/v2", scim_routes)
        .with_state(state);

    println!("Starting SCIM 2.0 HTTP server on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.expect("Failed to bind HTTP port");
    axum::serve(listener, app).await.expect("Failed to serve HTTP server");
}
