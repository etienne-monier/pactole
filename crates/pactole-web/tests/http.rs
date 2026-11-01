use std::path::{Path, PathBuf};

use axum::{Router, body::to_bytes, http::StatusCode};
use pactole_app::Application;
use pactole_storage_fs::FsLedgerReader;
use pactole_web::{ServerOptions, router};
use serde_json::Value;
use tower::util::ServiceExt;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(path)
}

fn fixture_router(path: &str) -> Router {
    let reader = FsLedgerReader::new(fixture(path), pactole_config::ValidationOptions::default());
    router(Application::new(reader), ServerOptions::default())
}

async fn get_json(router: Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            axum::http::Request::get(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&body).unwrap_or_else(|error| {
        panic!(
            "response was not JSON ({error}): {}",
            String::from_utf8_lossy(&body)
        )
    });
    (status, value)
}

#[tokio::test]
async fn exposes_all_report_endpoints_for_the_fixture_project() {
    let endpoints = [
        "/api/v1/capabilities",
        "/api/v1/balance?as_of=2026-03-31",
        "/api/v1/balance-series?dates=2026-01-31,2026-02-28,2026-03-31",
        "/api/v1/income?from=2026-01-01&to=2026-03-31",
        "/api/v1/expenses?from=2026-01-01&to=2026-03-31",
        "/api/v1/register?account=Assets:Checking",
        "/api/v1/diagnostics",
    ];

    for endpoint in endpoints {
        let (status, body) = get_json(fixture_router("journal/ledger.pactole"), endpoint).await;
        assert_eq!(status, StatusCode::OK, "{endpoint}: {body}");
        assert!(body.is_object(), "{endpoint}: {body}");
    }
}

#[tokio::test]
async fn clean_fixture_has_no_diagnostics_and_keeps_multi_commodity_amounts() {
    let (status, diagnostics) = get_json(
        fixture_router("journal/ledger.pactole"),
        "/api/v1/diagnostics",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(diagnostics["data"]["diagnostics"], serde_json::json!([]));

    let (status, balance) = get_json(
        fixture_router("journal/ledger.pactole"),
        "/api/v1/balance?as_of=2026-03-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let amounts: Vec<&Value> = balance["data"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["lines"].as_array().into_iter().flatten())
        .flat_map(|line| line["amounts"].as_array().into_iter().flatten())
        .collect();
    assert!(
        !amounts.is_empty(),
        "the fixture should produce report amounts"
    );
    assert!(amounts.iter().all(|amount| amount["commodity"].is_string()));
}

#[tokio::test]
async fn valued_fixture_reports_expose_conversion_provenance() {
    let (status, body) = get_json(
        fixture_router("journal/ledger.pactole"),
        "/api/v1/balance?as_of=2026-03-31&convert_to=EUR&at=2026-03-31",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["valuation"]["currency"], "EUR");
    assert_eq!(body["data"]["valuation"]["date"], "2026-03-31");
    assert!(body["data"]["valuation"]["prices"].is_array());
    assert!(body["data"]["valuation"]["missing"].is_array());
}

#[tokio::test]
async fn partial_fixture_reports_include_diagnostics() {
    let (status, body) = get_json(
        fixture_router("journal-partial/ledger.pactole"),
        "/api/v1/diagnostics",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let diagnostics = body["data"]["diagnostics"]
        .as_array()
        .expect("diagnostics must be an array");
    assert!(!diagnostics.is_empty());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "analysis-incomplete"
            || diagnostic["code"] == "syntax-error"
            || diagnostic["code"] == "include-not-found"
    }));
}

#[tokio::test]
async fn invalid_report_query_returns_structured_json_error() {
    let (status, body) = get_json(
        fixture_router("journal/ledger.pactole"),
        "/api/v1/balance?as_of=not-a-date",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid-parameter");
    assert!(body["error"]["message"].is_string());
    assert!(body["error"].get("details").is_some());
}

#[tokio::test]
async fn missing_include_is_reported_through_the_api() {
    let root = std::env::temp_dir().join(format!(
        "pactole-web-missing-include-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let ledger = root.join("ledger.pactole");
    std::fs::write(&ledger, "include \"missing.pactole\"\n").unwrap();

    let reader = FsLedgerReader::new(ledger, pactole_config::ValidationOptions::default());
    let (status, body) = get_json(
        router(Application::new(reader), ServerOptions::default()),
        "/api/v1/diagnostics",
    )
    .await;

    std::fs::remove_dir_all(&root).ok();
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["data"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "include-not-found")
    );
}
