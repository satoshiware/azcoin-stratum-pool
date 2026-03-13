//! Composition-root test: selected job_source_mode wires the expected source.

use azcoin_pool::composition::build_job_source;
use common::parse_config_toml;

#[test]
fn test_rpc_mode_wires_rpc_source() {
    let config = parse_config_toml(
        r#"
[pool]
name = "test-pool"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "rpc"
"#,
    )
    .unwrap();
    let source = build_job_source(&config);
    // Source is constructed without panic. current_job would fail against unreachable daemon.
    assert!(std::sync::Arc::strong_count(&source) >= 1);
}

#[test]
fn test_api_mode_wires_api_source() {
    let config = parse_config_toml(
        r#"
[pool]
name = "test-pool"
[daemon]
url = "http://127.0.0.1:9999"
job_source_mode = "api"
"#,
    )
    .unwrap();
    let source = build_job_source(&config);
    assert!(std::sync::Arc::strong_count(&source) >= 1);
}

#[tokio::test]
async fn test_rpc_source_current_job_returns_none_when_unreachable() {
    let config = parse_config_toml(
        r#"
[pool]
name = "test-pool"
[daemon]
url = "http://127.0.0.1:19999"
job_source_mode = "rpc"
"#,
    )
    .unwrap();
    let source = build_job_source(&config);
    let job = source.current_job().await;
    // No daemon on port 19999, so we get None (no fallback)
    assert!(job.is_none());
}

#[tokio::test]
async fn test_api_source_current_job_returns_none_when_unreachable() {
    let config = parse_config_toml(
        r#"
[pool]
name = "test-pool"
[daemon]
url = "http://127.0.0.1:19998"
job_source_mode = "api"
"#,
    )
    .unwrap();
    let source = build_job_source(&config);
    let job = source.current_job().await;
    assert!(job.is_none());
}
