//! Telling a client it is too old: `GET /v1/meta` says how old a client may
//! be, and a change asked for by one below that is refused.

mod common;

use axum::http::{Method, StatusCode};
use common::App;
use serde_json::{Value, json};
use yuppers_backend::client_version::MinimumClientVersions;

const DATABASE: &str = "yuppers_test_client_version";

#[tokio::test]
async fn nothing_is_required_until_a_deployment_says_so() {
    let app = App::start(DATABASE).await;
    let meta = app
        .call(None, Method::GET, "/v1/meta", None, &[])
        .await
        .ok();
    assert_eq!(
        meta["minimum_client_versions"],
        json!({ "web": null, "ios": null, "android": null })
    );

    // A client of any age may change things.
    let ana = app.user("Ana").await;
    let reply = app
        .call(
            Some(&ana),
            Method::POST,
            "/v1/exchanges",
            Some(json!({ "timezone": "America/Chicago" })),
            &[("x-client-version", "ios/0.0.1")],
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

#[tokio::test]
async fn a_client_below_the_minimum_may_read_but_not_change() {
    let app = App::start_requiring(
        DATABASE,
        MinimumClientVersions {
            web: Some("2.0".to_owned()),
            ios: Some("1.4.0".to_owned()),
            android: None,
        },
    )
    .await;
    let meta = app
        .call(None, Method::GET, "/v1/meta", None, &[])
        .await
        .ok();
    assert_eq!(
        meta["minimum_client_versions"],
        json!({ "web": "2.0", "ios": "1.4.0", "android": null })
    );

    let ana = app.user("Ana").await;
    let exchange = app.draft(&ana).await;
    let create = |header: &'static str| {
        let (app, ana) = (&app, &ana);
        async move {
            app.call(
                Some(ana),
                Method::POST,
                "/v1/exchanges",
                Some(json!({ "timezone": "America/Chicago" })),
                &[("x-client-version", header)],
            )
            .await
        }
    };

    // Too old: refused before anything else is looked at, with the one code
    // for it, whether or not the request would otherwise have worked.
    create("ios/1.3.9")
        .await
        .refused(StatusCode::UPGRADE_REQUIRED, "CLIENT_TOO_OLD");
    create("web/1.9.9")
        .await
        .refused(StatusCode::UPGRADE_REQUIRED, "CLIENT_TOO_OLD");
    app.call(
        None,
        Method::POST,
        "/v1/auth/codes",
        Some(json!({ "identifier": "someone@example.test" })),
        &[("x-client-version", "ios/1.0.0")],
    )
    .await
    .refused(StatusCode::UPGRADE_REQUIRED, "CLIENT_TOO_OLD");

    // Reading is still allowed: an old client can show what there is, and
    // show the person that it must be updated.
    let reply = app
        .call(
            Some(&ana),
            Method::GET,
            &format!("/v1/exchanges/{exchange}"),
            None,
            &[("x-client-version", "ios/1.3.9")],
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);

    // At or above the minimum, a client with no minimum, a client that does
    // not say, and a header that does not read as a client: all allowed.
    for header in [
        "ios/1.4.0",
        "ios/1.10",
        "web/2.0.0",
        "android/0.1",
        "tv/0.1",
        "nonsense",
    ] {
        let reply = create(header).await;
        assert_eq!(reply.status, StatusCode::OK, "{header}: {}", reply.body);
    }
    let reply = app
        .call(
            Some(&ana),
            Method::POST,
            "/v1/exchanges",
            Some(json!({ "timezone": "America/Chicago" })),
            &[],
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);

    // The exchange was not touched by the refused requests.
    assert_eq!(app.view(&ana, &exchange).await["version"], Value::from(0));
}

#[tokio::test]
async fn a_client_below_the_minimum_may_still_sign_out_and_delete_the_account() {
    let app = App::start_requiring(
        DATABASE,
        MinimumClientVersions {
            web: None,
            ios: Some("9.0".to_owned()),
            android: None,
        },
    )
    .await;
    let old = [("x-client-version", "ios/1.0.0")];
    let ana = app.user("Ana").await;

    // Asking for a deletion code goes through.
    let reply = app
        .call(
            Some(&ana),
            Method::POST,
            "/v1/me/deletion/codes",
            Some(json!({ "channel": "EMAIL" })),
            &old,
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    // So does deleting: a wrong code is refused for being wrong, not for the
    // client's age.
    app.call(
        Some(&ana),
        Method::POST,
        "/v1/me/deletion",
        Some(json!({ "channel": "EMAIL", "code": "000000" })),
        &old,
    )
    .await
    .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");

    // And signing out.
    let reply = app
        .call(Some(&ana), Method::DELETE, "/v1/auth/session", None, &old)
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    app.call(Some(&ana), Method::GET, "/v1/me", None, &old)
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    // Anything else the same client asks to change is still refused.
    let ben = app.user("Ben").await;
    app.call(
        Some(&ben),
        Method::PATCH,
        "/v1/me",
        Some(json!({ "display_name": "Benjamin" })),
        &old,
    )
    .await
    .refused(StatusCode::UPGRADE_REQUIRED, "CLIENT_TOO_OLD");
}
