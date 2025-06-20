use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use config::{Config, ConfigError, Environment, File};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::net::SocketAddr;
use thiserror::Error;
use tracing::{error, info, instrument};
use tracing_subscriber;
use uuid::Uuid;
use validator::Validate;

// Add utoipa for API documentation
use utoipa::{
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
    Modify, OpenApi, ToSchema,
};
use utoipa_swagger_ui::SwaggerUi;

// --- 1. Production Configuration ---

#[derive(Debug, Deserialize, Clone)]
struct DatabaseSettings {
    url: String,
}

#[derive(Debug, Deserialize, Clone)]
struct JwtSettings {
    secret: String,
}

#[derive(Debug, Deserialize, Clone)]
struct Settings {
    server_address: String,
    database: DatabaseSettings,
    jwt: JwtSettings,
}

impl Settings {
    fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            // 1. Start with `config/default.toml`
            .add_source(File::with_name("config/default"))
            // 2. Add in environment-specific overrides (e.g., `config/production.toml`)
            .add_source(File::with_name("config/production").required(false))
            // 3. Add in settings from environment variables (e.g., `APP_DATABASE_URL=...`)
            //    with a prefix of `APP` and a separator of `_`
            // FIX: Use a single underscore separator, which is more standard.
            .add_source(Environment::with_prefix("APP").separator("_"))
            .build()?;

        s.try_deserialize()
    }
}


// --- 2. OpenAPI Documentation Setup ---

#[derive(OpenApi)]
#[openapi(
    paths(
        register_user_handler,
        login_handler,
        user_info_handler
    ),
    components(
        schemas(
            RegisterUserPayload,
            LoginPayload,
            MaskedUserInfo,
            User,
            ErrorResponse,
            LoginResponse,
            RegisterSuccessResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "tiny-bank-server", description = "Tiny Bank Server API")
    )
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        )
    }
}

// --- 3. Validation Logic ---

static IFSC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Z]{4}0[A-Z0-9]{6}$").unwrap());

/// Masks the account number, showing only the last 4 digits.
fn mask_account_number(account_number: &str) -> String {
    if account_number.len() > 4 {
        format!("************{}", &account_number[account_number.len() - 4..])
    } else {
        "****".to_string()
    }
}

// --- 4. Main Application State & Setup ---

#[derive(Clone)]
struct AppState {
    db_pool: PgPool,
    http_client: ReqwestClient,
    settings: Settings,
}

// Use the default multi-threaded runtime for production performance.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use structured JSON logging for production.
    tracing_subscriber::fmt().json().init();

    info!("[STARTUP] Loading configuration...");
    let settings = Settings::new()?;
    info!("[SUCCESS] Configuration loaded.");

    // This debug output will now show if the override is working.
    println!("\n[DEBUG] FINAL DATABASE URL: {}\n", settings.database.url);

    info!("[STARTUP] Attempting to connect to the database...");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.database.url)
        .await?;
    info!("[SUCCESS] Database pool created successfully.");

    info!("[STARTUP] Creating HTTP client...");
    let http_client = ReqwestClient::new();
    info!("[SUCCESS] HTTP client created.");

    let app_state = AppState {
        db_pool: pool,
        http_client,
        settings: settings.clone(),
    };

    info!("[STARTUP] Building application routes...");
    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/register", post(register_user_handler))
        .route("/login", post(login_handler))
        .route(
            "/auth/info",
            get(user_info_handler).route_layer(middleware::from_fn_with_state(
                app_state.clone(),
                auth_middleware,
            )),
        )
        .with_state(app_state);
    info!("[SUCCESS] Application routes built.");

    let addr: SocketAddr = settings.server_address.parse()?;
    info!("[STARTUP] Binding server to address: {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("[SUCCESS] Server bound. Starting to listen for connections...");
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

// --- 5. Error Handling ---

#[derive(Debug, Error)]
enum AppError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("External API error")]
    Reqwest(#[from] reqwest::Error),
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Authentication failed: {0}")]
    AuthError(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal server error")]
    Internal,
}

#[derive(Serialize, ToSchema)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            AppError::Sqlx(e) => {
                error!("Database error: {:?}", e);
                if let Some(db_err) = e.as_database_error() {
                    if db_err.is_unique_violation() {
                        return (
                            StatusCode::CONFLICT,
                            Json(ErrorResponse {
                                error: "Resource already exists.".to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database operation failed".to_string(),
                )
            }
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Reqwest(e) => {
                error!("External API call failed. Full error: {:?}", e);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed to communicate with external service".to_string(),
                )
            }
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()),
            AppError::AuthError(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal error occurred".to_string(),
            ),
        };

        let body = Json(ErrorResponse { error: error_message });
        (status, body).into_response()
    }
}

// --- 6. Models & Payloads ---

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RegisterUserPayload {
    #[validate(length(min = 9, max = 18, message = "Account number must be between 9 and 18 digits."))]
    account_number: String,
    #[validate(regex(path = "*IFSC_REGEX", message = "Invalid IFSC code format."))]
    ifsc: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LoginPayload {
    account_number: String,
    ifsc: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
struct User {
    id: Uuid,
    account_number: String,
    ifsc_code: String,
    bank_name: String,
    branch: String,
    address: Option<String>,
    city: Option<String>,
    state_code: Option<String>,
    routing_no: Option<String>,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MaskedUserInfo {
    id: Uuid,
    masked_account_number: String,
    ifsc_code: String,
    bank_name: String,
    branch: String,
    address: Option<String>,
    city: Option<String>,
    state_code: Option<String>,
    routing_no: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BankApiData {
    bank_name: String,
    bank_branch_name: String,
    address: String,
    city_and_pincode: String,
    country_code: String,
    network_type: String,
    routing_no: String,
    state_code: String,
}

#[derive(Debug, Deserialize)]
struct BankApiResponse {
    data: BankApiData,
}

#[derive(Serialize, ToSchema)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RegisterSuccessResponse {
    message: String,
    user_id: Uuid,
}

// --- 7. Authentication (JWT) ---

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: i64,
}

fn create_jwt(user_id: Uuid, jwt_secret: &str) -> Result<String, AppError> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("Failed to calculate expiration")
        .timestamp();

    let claims = Claims {
        sub: user_id,
        exp: expiration,
    };

    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_ref()),
    )
        .map_err(|e| {
            error!("JWT encoding failed: {:?}", e);
            AppError::AuthError("Could not create token".to_string())
        })
}

fn decode_jwt(token: &str, jwt_secret: &str) -> Result<Claims, AppError> {
    jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_ref()),
        &jsonwebtoken::Validation::default(),
    )
        .map(|data| data.claims)
        .map_err(|e| {
            error!("JWT decoding/validation failed: {:?}", e);
            AppError::AuthError(format!("Invalid token: {}", e))
        })
}

#[instrument(skip_all)]
async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let token = if let Some(auth_header) = auth_header {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            token
        } else {
            return Err(AppError::AuthError("Invalid Authorization header format".to_string()));
        }
    } else {
        return Err(AppError::AuthError("Missing Authorization header".to_string()));
    };

    let claims = decode_jwt(token, &state.settings.jwt.secret)?;

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(claims.sub)
        .fetch_optional(&state.db_pool)
        .await?
        .ok_or_else(|| AppError::AuthError("User from token not found".to_string()))?;

    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

// --- 8. API Handlers ---

/// Register a new user
#[utoipa::path(
    post,
    path = "/register",
    request_body = RegisterUserPayload,
    responses(
        (status = 201, description = "User registered successfully", body = RegisterSuccessResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Account number already exists", body = ErrorResponse),
        (status = 502, description = "External API error", body = ErrorResponse)
    )
)]
#[axum::debug_handler]
#[instrument(skip_all)]
async fn register_user_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterUserPayload>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(|e| {
        AppError::Validation(e.to_string().replace('\n', ", "))
    })?;

    let existing_user = sqlx::query("SELECT id FROM users WHERE account_number = $1")
        .bind(&payload.account_number)
        .fetch_optional(&state.db_pool)
        .await?;

    if existing_user.is_some() {
        return Err(AppError::Conflict("Account number already registered.".to_string()));
    }

    info!("Attempting to call external API for IFSC: {}", payload.ifsc);
    let api_url = "https://api.bulkpe.in/api/validateIFSCStatic";

    let api_response = state
        .http_client
        .post(api_url)
        .json(&json!({ "ifsc": &payload.ifsc }))
        .send()
        .await?;


    if !api_response.status().is_success() {
        let status = api_response.status();
        let error_body = api_response.text().await.unwrap_or_default();
        error!(
            "External API returned a non-success status: {}. Body: {}",
            status, error_body
        );
        return Err(AppError::Validation("The provided IFSC code is not valid or could not be verified by the bank API.".to_string()));
    }

    let bank_response: BankApiResponse = api_response.json().await?;
    let bank_data = bank_response.data;
    info!("Successfully fetched bank details for {}: {}", payload.ifsc, bank_data.bank_name);

    let new_user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, account_number, ifsc_code, bank_name, branch, address, city, state_code, routing_no)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
        .bind(Uuid::new_v4())
        .bind(&payload.account_number)
        .bind(&payload.ifsc)
        .bind(&bank_data.bank_name)
        .bind(&bank_data.bank_branch_name)
        .bind(&bank_data.address)
        .bind(&bank_data.city_and_pincode)
        .bind(&bank_data.state_code)
        .bind(&bank_data.routing_no)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error() {
                if db_err.is_unique_violation() {
                    return AppError::Conflict("Account number already registered.".to_string());
                }
            }
            AppError::Sqlx(e)
        })?;

    info!("New user registered with ID: {}", new_user.id);

    Ok((
        StatusCode::CREATED,
        Json(RegisterSuccessResponse {
            message: "User registered successfully.".to_string(),
            user_id: new_user.id,
        }),
    ))
}


/// Log in a user
#[utoipa::path(
    post,
    path = "/login",
    request_body = LoginPayload,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    )
)]
#[axum::debug_handler]
#[instrument(skip_all)]
async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE account_number = $1")
        .bind(&payload.account_number)
        .fetch_optional(&state.db_pool)
        .await?
        .ok_or(AppError::InvalidCredentials)?;

    if user.ifsc_code != payload.ifsc {
        return Err(AppError::InvalidCredentials);
    }

    let token = create_jwt(user.id, &state.settings.jwt.secret)?;

    info!("User {} logged in successfully.", user.id);

    Ok(Json(LoginResponse { token }))
}


/// Get authenticated user's info
#[utoipa::path(
    get,
    path = "/auth/info",
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "User info retrieved successfully", body = MaskedUserInfo),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
#[axum::debug_handler]
#[instrument(skip_all)]
async fn user_info_handler(
    axum::Extension(user): axum::Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let masked_info = MaskedUserInfo {
        id: user.id,
        masked_account_number: mask_account_number(&user.account_number),
        ifsc_code: user.ifsc_code,
        bank_name: user.bank_name,
        branch: user.branch,
        address: user.address,
        city: user.city,
        state_code: user.state_code,
        routing_no: user.routing_no,
    };

    info!("Returning info for user {}", user.id);

    Ok(Json(masked_info))
}
