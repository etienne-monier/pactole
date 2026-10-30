#![allow(clippy::missing_docs_in_private_items)]

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{RawQuery, Request, State},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{Datelike, Local, NaiveDate};
use pactole_app::{
    Application, ApplicationError, BackendError, BackendOperation, BalanceQuery, ComparisonRequest,
    Diagnostic, DiagnosticsQuery, ExpensesQuery, IncomeQuery, RegisterQuery, ValuationRequest,
};
use pactole_config::{ImportConfig, ReportOptions};
use pactole_core::{
    AccountClassification, AccountGrouping, AccountName, CommodityName, DiffDirection, DiffMode,
    ExpenseGrouping, ExpenseOptions, IncomeGrouping, Period, ReferenceDate, RegisterFilter,
    ReportFormat, TransactionStatus, previous_period,
};
use serde_json::{Value, json};
use thiserror::Error;

const INDEX_HTML: &[u8] = include_bytes!("../web/dist/index.html");
const APP_JS: &[u8] = include_bytes!("../web/dist/assets/app.js");
const APP_CSS: &[u8] = include_bytes!("../web/dist/assets/app.css");

/// Options controlling the local HTTP server.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Address on which the server listens.
    pub bind: IpAddr,
    /// TCP port on which the server listens.
    pub port: u16,
    /// Optional project directory used to make diagnostic sources relative.
    pub source_root: Option<PathBuf>,
    /// Report account classification.
    pub reports: ReportOptions,
    /// Import configuration used for the uncategorized expense fallback.
    pub import: Option<ImportConfig>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            bind: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 3000,
            source_root: None,
            reports: ReportOptions::default(),
            import: None,
        }
    }
}

/// Errors raised while starting or running the HTTP server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// The listener could not be created.
    #[error("could not bind web server: {0}")]
    Bind(#[source] std::io::Error),
    /// The server stopped because of an I/O error.
    #[error("web server failed: {0}")]
    Serve(#[source] std::io::Error),
}

struct AppState<R> {
    application: Arc<Application<R>>,
    options: ServerOptions,
}

impl<R> Clone for AppState<R> {
    fn clone(&self) -> Self {
        Self {
            application: Arc::clone(&self.application),
            options: self.options.clone(),
        }
    }
}

/// Builds the versioned API and frontend router.
pub fn router<R>(application: Application<R>, options: ServerOptions) -> Router
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let state = AppState {
        application: Arc::new(application),
        options,
    };

    Router::new()
        .route("/api/v1/capabilities", get(capabilities::<R>))
        .route("/api/v1/balance", get(balance::<R>))
        .route("/api/v1/income", get(income::<R>))
        .route("/api/v1/expenses", get(expenses::<R>))
        .route("/api/v1/register", get(register::<R>))
        .route("/api/v1/diagnostics", get(diagnostics::<R>))
        .route("/", get(index))
        .route("/assets/app.js", get(app_js))
        .route("/assets/app.css", get(app_css))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(frontend_fallback)
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            validate_host::<R>,
        ))
        .with_state(state)
}

/// Starts the server and waits for Ctrl-C.
pub async fn serve<R>(
    application: Application<R>,
    options: ServerOptions,
) -> Result<(), ServerError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let address = SocketAddr::new(options.bind, options.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(ServerError::Bind)?;
    let address = listener.local_addr().map_err(ServerError::Bind)?;
    let mut options = options;
    options.bind = address.ip();
    options.port = address.port();
    if !address.ip().is_loopback() {
        eprintln!(
            "warning: the Pactole web server is exposed beyond loopback without authentication"
        );
    }
    println!("Serving Pactole web interface at http://{address}/");
    println!("API available at http://{address}/api/v1/");
    axum::serve(listener, router(application, options))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(ServerError::Serve)
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

async fn validate_host<R>(
    State(state): State<AppState<R>>,
    request: Request,
    next: Next,
) -> Response
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    if state.options.bind.is_loopback()
        && let Some(host) = request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
    {
        let allowed = [
            format!("{}:{}", state.options.bind, state.options.port),
            format!("{}", state.options.bind),
            format!("localhost:{}", state.options.port),
            "localhost".to_string(),
        ];
        if !allowed
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(host))
        {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": { "code": "host-not-allowed", "message": "host is not allowed", "details": null } })),
            )
                .into_response();
        }
    }
    next.run(request).await
}

async fn capabilities<R>(State(state): State<AppState<R>>) -> Json<Value>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let capabilities = state.application.capabilities();
    Json(json!({
        "capabilities": {
            "read": capabilities.supports(pactole_app::Capability::Read),
            "write": capabilities.supports(pactole_app::Capability::Write),
            "transactions": capabilities.supports(pactole_app::Capability::Transactions),
            "budgets": capabilities.supports(pactole_app::Capability::Budgets),
        }
    }))
}

async fn balance<R>(
    State(state): State<AppState<R>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let params = query_params(raw_query)?;
    check_params(
        &params,
        &["as_of", "depth", "convert_to", "at", "include_warnings"],
    )?;
    let date = optional_date(&params, "as_of")?.unwrap_or_else(|| Local::now().date_naive());
    let grouping = account_grouping(&params)?;
    let valuation = valuation(&params, date)?;
    let include_warnings = boolean(&params, "include_warnings")?;
    let reports = state.options.reports.clone();
    let application = Arc::clone(&state.application);
    let result = run_application(application, move |application| {
        application.balance(BalanceQuery {
            as_of: ReferenceDate::new(date),
            grouping,
            classification: classification(&reports),
            valuation,
        })
    })
    .await
    .map_err(|error| ApiError::from_application(error, &state.options))?;
    Ok(Json(report_response(
        result.report.render(ReportFormat::Json),
        result.diagnostics,
        &state.options,
        include_warnings,
    )))
}

async fn income<R>(
    State(state): State<AppState<R>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let params = query_params(raw_query)?;
    check_params(
        &params,
        &[
            "from",
            "to",
            "by",
            "depth",
            "convert_to",
            "at",
            "include_warnings",
        ],
    )?;
    let (period, _) = period_from(&params)?;
    let grouping = income_grouping(&params)?;
    let valuation = valuation(&params, period.end())?;
    let include_warnings = boolean(&params, "include_warnings")?;
    let reports = state.options.reports.clone();
    let application = Arc::clone(&state.application);
    let result = run_application(application, move |application| {
        application.income(IncomeQuery {
            period,
            grouping,
            classification: classification(&reports),
            valuation,
        })
    })
    .await
    .map_err(|error| ApiError::from_application(error, &state.options))?;
    Ok(Json(report_response(
        result.report.render(ReportFormat::Json),
        result.diagnostics,
        &state.options,
        include_warnings,
    )))
}

async fn expenses<R>(
    State(state): State<AppState<R>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let params = query_params(raw_query)?;
    check_params(
        &params,
        &[
            "from",
            "to",
            "by",
            "depth",
            "uncategorized",
            "inactive",
            "compare",
            "compare_from",
            "compare_to",
            "diff",
            "reverse_diff",
            "convert_to",
            "at",
            "include_warnings",
        ],
    )?;
    let (period, _) = period_from(&params)?;
    let grouping = expense_grouping(&params)?;
    let include_uncategorized = boolean(&params, "uncategorized")?;
    let include_inactive = boolean(&params, "inactive")?;
    let include_warnings = boolean(&params, "include_warnings")?;
    let uncategorized = if include_uncategorized {
        let names = state
            .options
            .reports
            .uncategorized
            .clone()
            .or_else(|| {
                state
                    .options
                    .import
                    .as_ref()
                    .map(|config| vec![config.default_counter_account.clone()])
            })
            .ok_or_else(|| ApiError::invalid("uncategorized requires configured accounts"))?;
        names
            .into_iter()
            .map(|name| {
                AccountName::new(name).map_err(|error| ApiError::invalid(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let comparison = comparison(&params, period)?;
    let valuation = valuation(&params, period.end())?;
    let reports = state.options.reports.clone();
    let application = Arc::clone(&state.application);
    let result = run_application(application, move |application| {
        application.expenses(ExpensesQuery {
            period,
            grouping,
            options: ExpenseOptions {
                uncategorized,
                include_uncategorized,
                include_inactive,
            },
            classification: classification(&reports),
            comparison,
            valuation,
        })
    })
    .await
    .map_err(|error| ApiError::from_application(error, &state.options))?;
    let data = json!({
        "current": report_value(&result.current),
        "comparison": result.comparison.as_ref().map(comparison_value).unwrap_or(Value::Null),
    });
    Ok(Json(
        json!({ "data": data, "diagnostics": diagnostics_json(&result.diagnostics, &state.options, include_warnings) }),
    ))
}

async fn register<R>(
    State(state): State<AppState<R>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let params = query_params(raw_query)?;
    check_params(
        &params,
        &[
            "account",
            "from",
            "to",
            "payee",
            "narration",
            "tag",
            "status",
            "convert_to",
            "at",
            "include_warnings",
        ],
    )?;
    let filter = RegisterFilter {
        account: params
            .get("account")
            .map(|value| {
                AccountName::new(value.clone())
                    .map_err(|error| ApiError::invalid(error.to_string()))
            })
            .transpose()?,
        from: optional_date(&params, "from")?,
        to: optional_date(&params, "to")?,
        payee: params.get("payee").cloned(),
        narration: params.get("narration").cloned(),
        tag: params.get("tag").cloned(),
        status: params
            .get("status")
            .map(|value| parse_status(value))
            .transpose()?,
    };
    let default_valuation_date = filter.to.unwrap_or_else(|| Local::now().date_naive());
    let valuation = valuation(&params, default_valuation_date)?;
    let include_warnings = boolean(&params, "include_warnings")?;
    let application = Arc::clone(&state.application);
    let result = run_application(application, move |application| {
        application.register(RegisterQuery { filter, valuation })
    })
    .await
    .map_err(|error| ApiError::from_application(error, &state.options))?;
    let entries = result.entries.into_iter().map(|entry| json!({
        "date": entry.date.to_string(),
        "status": status_name(&entry.status),
        "payee": entry.payee,
        "narration": entry.narration,
        "account": entry.account.as_str(),
        "amount": entry.amount.map(|amount| amount_json(&amount)),
        "balance": entry.balance.into_iter().map(|(commodity, number)| json!({ "commodity": commodity.as_str(), "number": number.to_string() })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>();
    Ok(Json(
        json!({ "data": { "entries": entries, "valuation": result.valuation.map(|value| value_json(&value)) }, "diagnostics": diagnostics_json(&result.diagnostics, &state.options, include_warnings) }),
    ))
}

async fn diagnostics<R>(
    State(state): State<AppState<R>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
{
    let params = query_params(raw_query)?;
    check_params(&params, &["include_warnings"])?;
    let include_warnings = boolean(&params, "include_warnings")?;
    let application = Arc::clone(&state.application);
    let result = run_application(application, move |application| {
        application.diagnostics(DiagnosticsQuery { include_warnings })
    })
    .await
    .map_err(|error| ApiError::from_application(error, &state.options))?;
    Ok(Json(
        json!({ "data": { "diagnostics": diagnostics_json(&result.diagnostics, &state.options, include_warnings) }, "diagnostics": [] }),
    ))
}

async fn index() -> Response {
    static_response(INDEX_HTML, "text/html; charset=utf-8", None)
}

async fn app_js() -> Response {
    static_response(
        APP_JS,
        "application/javascript; charset=utf-8",
        Some("no-cache"),
    )
}

async fn app_css() -> Response {
    static_response(APP_CSS, "text/css; charset=utf-8", Some("no-cache"))
}

async fn frontend_fallback(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": { "code": "not-found", "message": "route not found", "details": null } }))).into_response();
    }
    if uri.path().starts_with("/assets/") {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": { "code": "not-found", "message": "asset not found", "details": null } }))).into_response();
    }
    static_response(INDEX_HTML, "text/html; charset=utf-8", None)
}

async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({ "error": { "code": "method-not-allowed", "message": "HTTP method is not supported", "details": null } })),
    )
        .into_response()
}

fn static_response(
    body: &'static [u8],
    content_type: &'static str,
    cache_control: Option<&'static str>,
) -> Response {
    let mut response = Response::new(body.into());
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Some(value) = cache_control {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    } else {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    response
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid-parameter",
            message: message.into(),
            details: None,
        }
    }

    fn from_application(error: ApplicationError, options: &ServerOptions) -> Self {
        match error {
            ApplicationError::InvalidQuery(error) => {
                let code = match &error {
                    pactole_app::InvalidQuery::UndeclaredValuationCommodity(_) => {
                        "undeclared-valuation-commodity"
                    }
                    _ => "invalid-query",
                };
                Self {
                    status: StatusCode::BAD_REQUEST,
                    code,
                    message: error.to_string(),
                    details: None,
                }
            }
            ApplicationError::LedgerUnavailable { diagnostics } => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "ledger-unavailable",
                message: "the ledger is unavailable because its diagnostics contain errors"
                    .to_string(),
                details: Some(
                    json!({ "diagnostics": diagnostics_json(&diagnostics, options, true) }),
                ),
            },
            ApplicationError::CapabilityUnavailable { capability } => Self {
                status: StatusCode::NOT_IMPLEMENTED,
                code: "capability-unavailable",
                message: error.to_string(),
                details: Some(json!({ "capability": format!("{capability:?}").to_lowercase() })),
            },
            ApplicationError::Backend(error) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "backend-error",
                message: error.to_string(),
                details: None,
            },
            ApplicationError::Report(error) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "report-error",
                message: error.to_string(),
                details: None,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "application-error",
                message: other.to_string(),
                details: None,
            },
        }
    }
}

async fn run_application<R, T, F>(
    application: Arc<Application<R>>,
    operation: F,
) -> Result<T, ApplicationError>
where
    R: pactole_app::LedgerReader + Send + Sync + 'static,
    T: Send + 'static,
    F: FnOnce(&Application<R>) -> Result<T, ApplicationError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(&application))
        .await
        .map_err(|error| {
            ApplicationError::Backend(BackendError {
                operation: BackendOperation::LoadSnapshot,
                message: format!("application task failed: {error}"),
            })
        })?
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": { "code": self.code, "message": self.message, "details": self.details } }))).into_response()
    }
}

fn report_response(
    rendered: String,
    diagnostics: Vec<Diagnostic>,
    options: &ServerOptions,
    include_warnings: bool,
) -> Value {
    let mut data = serde_json::from_str::<Value>(&rendered).unwrap_or_else(|_| json!({}));
    if !data.get("valuation").is_some() {
        data["valuation"] = Value::Null;
    }
    json!({ "data": data, "diagnostics": diagnostics_json(&diagnostics, options, include_warnings) })
}

fn report_value(report: &pactole_core::Report) -> Value {
    let mut value = serde_json::from_str::<Value>(&report.render(ReportFormat::Json))
        .unwrap_or_else(|_| json!({}));
    if !value.get("valuation").is_some() {
        value["valuation"] = Value::Null;
    }
    value
}

fn comparison_value(comparison: &pactole_core::ComparisonReport) -> Value {
    let mut value = serde_json::from_str::<Value>(&comparison.render(ReportFormat::Json))
        .unwrap_or_else(|_| json!({}));
    if !value.get("valuation").is_some() {
        value["valuation"] = Value::Null;
    }
    value
}

fn diagnostics_json(
    diagnostics: &[Diagnostic],
    options: &ServerOptions,
    include_warnings: bool,
) -> Vec<Value> {
    diagnostics.iter().filter(|diagnostic| include_warnings || diagnostic.is_error()).map(|diagnostic| {
        let source = diagnostic.source.as_ref().map(|source| {
            source_name(&source.0, options.source_root.as_deref())
        });
        json!({
            "severity": if diagnostic.is_error() { "error" } else { "warning" },
            "origin": origin_name(diagnostic.origin),
            "code": diagnostic.code.as_str(),
            "message": diagnostic.message,
            "source": source,
            "span": diagnostic.span.map(|span| json!({ "start": { "line": span.start.line + 1, "column": span.start.column + 1 }, "end": { "line": span.end.line + 1, "column": span.end.column + 1 }, "column_encoding": "utf8-byte" })),
        })
    }).collect()
}

fn source_name(source: &str, root: Option<&std::path::Path>) -> String {
    let path = PathBuf::from(source);
    if let Some(root) = root
        && let Ok(relative) = path.strip_prefix(root)
    {
        return relative.display().to_string();
    }
    let escapes_project = path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir));
    if escapes_project {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<source>".to_string())
    } else {
        source.to_string()
    }
}

fn query_params(raw_query: Option<String>) -> Result<HashMap<String, String>, ApiError> {
    let mut params = HashMap::new();
    if let Some(raw_query) = raw_query {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            if key.is_empty() {
                return Err(ApiError::invalid("query parameter names cannot be empty"));
            }
            params.insert(key.into_owned(), value.into_owned());
        }
    }
    Ok(params)
}

fn check_params(params: &HashMap<String, String>, allowed: &[&str]) -> Result<(), ApiError> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ApiError::invalid(format!(
                "unknown query parameter `{key}`"
            )));
        }
    }
    Ok(())
}

fn origin_name(origin: pactole_app::DiagnosticOrigin) -> &'static str {
    match origin {
        pactole_app::DiagnosticOrigin::Syntax => "syntax",
        pactole_app::DiagnosticOrigin::Lowering => "lowering",
        pactole_app::DiagnosticOrigin::Include => "include",
        pactole_app::DiagnosticOrigin::Analysis => "analysis",
        pactole_app::DiagnosticOrigin::Business => "business",
    }
}

fn classification(options: &ReportOptions) -> AccountClassification {
    let mut classification = AccountClassification::default();
    if let Some(roots) = &options.assets {
        classification = classification.with_assets(roots.clone());
    }
    if let Some(roots) = &options.liabilities {
        classification = classification.with_liabilities(roots.clone());
    }
    if let Some(roots) = &options.equity {
        classification = classification.with_equity(roots.clone());
    }
    if let Some(roots) = &options.income {
        classification = classification.with_income(roots.clone());
    }
    if let Some(roots) = &options.expenses {
        classification = classification.with_expenses(roots.clone());
    }
    classification
}

fn account_grouping(params: &HashMap<String, String>) -> Result<AccountGrouping, ApiError> {
    match positive_usize(params, "depth")? {
        Some(depth) => Ok(AccountGrouping::Depth(
            std::num::NonZeroUsize::new(depth).unwrap_or(std::num::NonZeroUsize::MIN),
        )),
        None => Ok(AccountGrouping::Full),
    }
}

fn income_grouping(params: &HashMap<String, String>) -> Result<IncomeGrouping, ApiError> {
    match params.get("by").map(String::as_str).unwrap_or("account") {
        "account" => Ok(IncomeGrouping::Account(account_grouping(params)?)),
        "payee" => reject_depth(params).map(|_| IncomeGrouping::Payee),
        "tag" => reject_depth(params).map(|_| IncomeGrouping::Tag),
        value => Err(ApiError::invalid(format!("invalid by value `{value}`"))),
    }
}

fn expense_grouping(params: &HashMap<String, String>) -> Result<ExpenseGrouping, ApiError> {
    match params.get("by").map(String::as_str).unwrap_or("account") {
        "account" => Ok(ExpenseGrouping::Account(account_grouping(params)?)),
        "payee" => reject_depth(params).map(|_| ExpenseGrouping::Payee),
        "tag" => reject_depth(params).map(|_| ExpenseGrouping::Tag),
        "month" => reject_depth(params).map(|_| ExpenseGrouping::Month),
        value => Err(ApiError::invalid(format!("invalid by value `{value}`"))),
    }
}

fn reject_depth(params: &HashMap<String, String>) -> Result<(), ApiError> {
    if params.contains_key("depth") {
        Err(ApiError::invalid(
            "depth can only be used with account grouping",
        ))
    } else {
        Ok(())
    }
}

fn period_from(params: &HashMap<String, String>) -> Result<(Period, NaiveDate), ApiError> {
    let to = optional_date(params, "to")?.unwrap_or_else(|| Local::now().date_naive());
    let from = optional_date(params, "from")?
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(to.year(), 1, 1).unwrap_or(to));
    Period::new(from, to)
        .map(|period| (period, to))
        .map_err(|error| ApiError::invalid(error.to_string()))
}

fn comparison(
    params: &HashMap<String, String>,
    period: Period,
) -> Result<Option<ComparisonRequest>, ApiError> {
    let requested = params.contains_key("compare")
        || params.contains_key("compare_from")
        || params.contains_key("compare_to");
    if !requested {
        return Ok(None);
    }
    let previous = match params.get("compare").map(String::as_str) {
        Some("previous") => previous_period(period)
            .ok_or_else(|| ApiError::invalid("cannot calculate the previous period"))?,
        Some("absolute") | None => {
            let from = required_date(params, "compare_from")?;
            let to = required_date(params, "compare_to")?;
            Period::new(from, to).map_err(|error| ApiError::invalid(error.to_string()))?
        }
        Some(value) => {
            return Err(ApiError::invalid(format!(
                "invalid compare value `{value}`"
            )));
        }
    };
    Ok(Some(ComparisonRequest {
        period: previous,
        direction: if boolean(params, "reverse_diff")? {
            DiffDirection::PreviousMinusCurrent
        } else {
            DiffDirection::CurrentMinusPrevious
        },
        mode: match params.get("diff").map(String::as_str).unwrap_or("absolute") {
            "absolute" => DiffMode::Absolute,
            "percentage" | "percent" => DiffMode::Percent,
            value => return Err(ApiError::invalid(format!("invalid diff value `{value}`"))),
        },
    }))
}

fn valuation(
    params: &HashMap<String, String>,
    default_date: NaiveDate,
) -> Result<Option<ValuationRequest>, ApiError> {
    match params.get("convert_to") {
        Some(target) => Ok(Some(ValuationRequest {
            target: CommodityName::new(target.clone())
                .map_err(|error| ApiError::invalid(error.to_string()))?,
            at: optional_date(params, "at")?.unwrap_or(default_date),
        })),
        None if params.contains_key("at") => Err(ApiError::invalid("at requires convert_to")),
        None => Ok(None),
    }
}

fn optional_date(
    params: &HashMap<String, String>,
    name: &str,
) -> Result<Option<NaiveDate>, ApiError> {
    params
        .get(name)
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|error| ApiError::invalid(format!("invalid {name}: {error}")))
        })
        .transpose()
}

fn required_date(params: &HashMap<String, String>, name: &str) -> Result<NaiveDate, ApiError> {
    optional_date(params, name)?.ok_or_else(|| ApiError::invalid(format!("{name} is required")))
}

fn positive_usize(params: &HashMap<String, String>, name: &str) -> Result<Option<usize>, ApiError> {
    params
        .get(name)
        .map(|value| {
            let parsed = value
                .parse::<usize>()
                .map_err(|_| ApiError::invalid(format!("{name} must be a positive integer")))?;
            if parsed == 0 {
                Err(ApiError::invalid(format!("{name} must be positive")))
            } else {
                Ok(parsed)
            }
        })
        .transpose()
}

fn boolean(params: &HashMap<String, String>, name: &str) -> Result<bool, ApiError> {
    match params.get(name).map(String::as_str) {
        None => Ok(false),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => Err(ApiError::invalid(format!(
            "{name} must be true or false, got `{value}`"
        ))),
    }
}

fn parse_status(value: &str) -> Result<TransactionStatus, ApiError> {
    match value {
        "cleared" => Ok(TransactionStatus::Cleared),
        "pending" => Ok(TransactionStatus::Pending),
        "uncleared" => Ok(TransactionStatus::Uncleared),
        _ => Err(ApiError::invalid(format!("invalid status `{value}`"))),
    }
}

fn status_name(status: &TransactionStatus) -> &'static str {
    match status {
        TransactionStatus::Cleared => "cleared",
        TransactionStatus::Pending => "pending",
        TransactionStatus::Uncleared => "uncleared",
    }
}

fn amount_json(amount: &pactole_core::Amount) -> Value {
    json!({ "commodity": amount.commodity.as_str(), "number": amount.number.to_string() })
}

fn value_json(value: &pactole_core::Valuation) -> Value {
    json!({
        "currency": value.currency.as_str(),
        "date": value.date.to_string(),
        "prices": value.prices.iter().map(|price| json!({
            "base": price.base.as_str(),
            "quote": price.quote.as_str(),
            "rate": price.rate.to_string(),
            "date": price.date.to_string(),
        })).collect::<Vec<_>>(),
        "missing": value.missing.iter().map(|commodity| commodity.as_str()).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use pactole_app::{BackendCapabilities, BackendError, LedgerReader, LedgerSnapshot, Revision};
    use tower::util::ServiceExt;

    struct EmptyReader;

    impl LedgerReader for EmptyReader {
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities::read_only()
        }

        fn snapshot(&self) -> Result<LedgerSnapshot, BackendError> {
            Ok(LedgerSnapshot::Ready {
                journal: pactole_core::Journal {
                    entries: Vec::new(),
                },
                revision: Revision("test".to_string()),
                diagnostics: Vec::new(),
            })
        }
    }

    fn test_router() -> Router {
        router(Application::new(EmptyReader), ServerOptions::default())
    }

    #[tokio::test]
    async fn serves_the_frontend_and_capabilities() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );

        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/capabilities")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["capabilities"]["read"], true);
        assert_eq!(value["capabilities"]["write"], false);
    }

    #[tokio::test]
    async fn serves_frontend_routes_but_keeps_api_errors_as_json() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );

        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/missing")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");

        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/assets/missing.js")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn validates_report_query_parameters() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/balance?as_of=not-a-date")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "invalid-parameter");
    }

    #[tokio::test]
    async fn rejects_unexpected_loopback_hosts() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::HOST, "evil.example")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "host-not-allowed");
    }

    #[tokio::test]
    async fn rejects_unknown_query_parameters_and_methods() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/balance?bogus=1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "invalid-parameter");

        let response = test_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/balance")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    }

    #[tokio::test]
    async fn keeps_expense_and_diagnostic_response_shapes_stable() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/expenses")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(value["data"]["current"].is_object());
        assert_eq!(value["data"]["comparison"], Value::Null);
        assert_eq!(value["diagnostics"], json!([]));

        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/diagnostics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["diagnostics"], json!([]));
        assert_eq!(value["diagnostics"], json!([]));
    }

    #[tokio::test]
    async fn maps_unavailable_ledgers_to_structured_errors() {
        struct UnavailableReader;

        impl LedgerReader for UnavailableReader {
            fn capabilities(&self) -> BackendCapabilities {
                BackendCapabilities::read_only()
            }

            fn snapshot(&self) -> Result<LedgerSnapshot, BackendError> {
                Ok(LedgerSnapshot::Unavailable {
                    diagnostics: vec![Diagnostic {
                        source: Some(pactole_app::SourceId(
                            "/home/user/private/journal.pactole".to_string(),
                        )),
                        span: None,
                        severity: pactole_app::DiagnosticSeverity::Error,
                        origin: pactole_app::DiagnosticOrigin::Business,
                        code: pactole_core::DiagnosticCode::AccountNotOpen,
                        message: "account is not open".to_string(),
                    }],
                })
            }
        }

        let response = router(
            Application::new(UnavailableReader),
            ServerOptions::default(),
        )
        .oneshot(
            Request::builder()
                .uri("/api/v1/balance")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "ledger-unavailable");
        assert_eq!(
            value["error"]["details"]["diagnostics"][0]["source"],
            "journal.pactole"
        );
        assert_eq!(
            value["error"]["details"]["diagnostics"][0]["origin"],
            "business"
        );
    }

    #[test]
    fn keeps_diagnostic_sources_project_relative() {
        assert_eq!(
            source_name(
                "/project/journal.pactole",
                Some(std::path::Path::new("/project"))
            ),
            "journal.pactole"
        );
        assert_eq!(
            source_name("/elsewhere/journal.pactole", None),
            "journal.pactole"
        );
        assert_eq!(source_name("../journal.pactole", None), "journal.pactole");
        assert_eq!(
            source_name("accounts/checking.pactole", None),
            "accounts/checking.pactole"
        );
    }

    #[tokio::test]
    async fn serves_a_real_fixture_through_the_application_boundary() {
        let reader = pactole_storage_fs::FsLedgerReader::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/journal/ledger.pactole"),
            pactole_config::ValidationOptions::default(),
        );
        let response = router(Application::new(reader), ServerOptions::default())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/balance?as_of=2026-10-30")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["scope"]["kind"], "as_of");
        assert!(value["data"]["sections"].is_array());
    }
}
