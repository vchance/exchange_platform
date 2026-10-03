use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::Value;
use time::macros::datetime;

use super::objects::{describe_refusal, insert_request, patch_request, token_request};
use crate::wallet::pass;
use crate::wallet::testkit::{CLIENT_EMAIL, CREDENTIALS, ISSUER_ID};

/// The claims of a JWT whose RS256 signature holds under the test service
/// account's public key, and its header.
pub(crate) fn verified(jwt: &str) -> (Value, Value) {
    let (signed, signature) = jwt.rsplit_once('.').expect("three parts");
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &CREDENTIALS.google_public_der)
        .verify(
            signed.as_bytes(),
            &URL_SAFE_NO_PAD.decode(signature).unwrap(),
        )
        .expect("the signature holds");
    let (header, claims) = signed.split_once('.').unwrap();
    let decode = |part: &str| -> Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
    };
    (decode(header), decode(claims))
}

#[test]
fn the_save_link_is_a_jwt_signed_by_the_service_account_naming_only_the_object() {
    let wallet = crate::wallet::apple::tests::wallet();
    let issuer = wallet.google.as_ref().unwrap();
    let url = issuer
        .save_url(
            "0123abcd",
            "https://app.test",
            datetime!(2026-10-03 12:00 UTC),
        )
        .unwrap();
    let jwt = url.strip_prefix(super::SAVE_URL).expect("a save link");
    let (header, claims) = verified(jwt);

    assert_eq!(header["alg"], "RS256");
    assert_eq!(header["kid"], "0123456789abcdef");
    assert_eq!(claims["iss"], CLIENT_EMAIL);
    assert_eq!(claims["aud"], "google");
    assert_eq!(claims["typ"], "savetowallet");
    assert_eq!(claims["origins"][0], "https://app.test");
    // Only the object's ID and class: no face, so no link can create one.
    assert_eq!(
        claims["payload"],
        serde_json::json!({ "genericObjects": [{
            "id": format!("{ISSUER_ID}.0123abcd"),
            "classId": format!("{ISSUER_ID}.yuppers_agreement"),
        }] })
    );

    // The object as created, here a void one.
    let model = pass::void(wallet.wording.language("es"));
    let object = issuer.object(&model, "0123abcd");
    assert_eq!(object["id"], format!("{ISSUER_ID}.0123abcd"));
    assert_eq!(object["classId"], issuer.class()["id"]);
    assert_eq!(object["state"], "INACTIVE");
    assert_eq!(object["header"]["defaultValue"]["language"], "es");
    assert_eq!(
        object["header"]["defaultValue"]["value"],
        "Ya no está en uso"
    );
    assert_eq!(object["linksModuleData"]["uris"], serde_json::json!([]));

    // A different key's signature does not hold.
    let mut forged = jwt.to_owned();
    forged.replace_range(forged.len() - 4.., "AAAA");
    let (signed, signature) = forged.rsplit_once('.').unwrap();
    assert!(
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &CREDENTIALS.google_public_der)
            .verify(
                signed.as_bytes(),
                &URL_SAFE_NO_PAD.decode(signature).unwrap_or_default()
            )
            .is_err()
    );
}

#[test]
fn the_access_token_request_is_a_signed_assertion_for_the_wallet_scope_only() {
    let wallet = crate::wallet::apple::tests::wallet();
    let account = &wallet.google.as_ref().unwrap().account;
    let request = token_request(account, datetime!(2026-10-03 12:00 UTC)).unwrap();
    assert_eq!(request.uri(), "https://oauth2.googleapis.com/token");
    assert_eq!(
        request.headers()["content-type"],
        "application/x-www-form-urlencoded"
    );
    let body = http_body_util::BodyExt::collect(request.into_body());
    let body = futures_body(body);
    let assertion = body
        .strip_prefix("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion=")
        .expect("the JWT bearer grant");
    let (_, claims) = verified(assertion);
    assert_eq!(claims["scope"], super::objects::SCOPE);
    assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
    assert_eq!(
        claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
        3600
    );
}

/// The whole body of a request built in memory.
fn futures_body(
    collect: http_body_util::combinators::Collect<http_body_util::Full<hyper::body::Bytes>>,
) -> String {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let bytes = runtime.block_on(collect).unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn an_update_patches_the_one_object_by_its_id() {
    let object = serde_json::json!({ "state": "ACTIVE" });
    let request = patch_request(
        super::objects::API_ORIGIN,
        "token",
        &format!("{ISSUER_ID}.0123abcd"),
        &object,
    )
    .unwrap();
    assert_eq!(request.method(), hyper::Method::PATCH);
    assert_eq!(
        request.uri().to_string(),
        format!(
            "https://walletobjects.googleapis.com/walletobjects/v1/genericObject/{ISSUER_ID}.0123abcd"
        )
    );
    assert_eq!(request.headers()["authorization"], "Bearer token");
    assert!(patch_request(super::objects::API_ORIGIN, "token", "../classes", &object).is_err());
}

#[test]
fn creating_posts_the_class_or_object_by_kind() {
    let object = serde_json::json!({ "id": format!("{ISSUER_ID}.0123abcd"), "state": "ACTIVE" });
    let request = insert_request(
        super::objects::API_ORIGIN,
        "token",
        "genericObject",
        &object,
    )
    .unwrap();
    assert_eq!(request.method(), hyper::Method::POST);
    assert_eq!(
        request.uri().to_string(),
        "https://walletobjects.googleapis.com/walletobjects/v1/genericObject"
    );
    assert_eq!(request.headers()["authorization"], "Bearer token");
    let class = serde_json::json!({ "id": format!("{ISSUER_ID}.yuppers_agreement") });
    assert!(insert_request(super::objects::API_ORIGIN, "token", "genericClass", &class).is_ok());
    assert!(insert_request(super::objects::API_ORIGIN, "token", "offerObject", &class).is_err());
    let bad = serde_json::json!({ "id": "../x" });
    assert!(insert_request(super::objects::API_ORIGIN, "token", "genericObject", &bad).is_err());
}

#[test]
fn a_refusal_keeps_the_status_and_googles_codes_never_its_message() {
    use hyper::StatusCode;
    let api = br#"{"error": {"code": 400, "message": "Invalid header value Ben Ortiz owes 400.00",
        "status": "INVALID_ARGUMENT", "errors": [{"reason": "invalidArgument",
        "message": "Ben Ortiz owes 400.00", "domain": "global"}]}}"#;
    let said = describe_refusal(StatusCode::BAD_REQUEST, api);
    assert_eq!(said, "HTTP 400, error INVALID_ARGUMENT invalidArgument");
    let token =
        br#"{"error": "invalid_grant", "error_description": "Invalid JWT Signature for wallet@x"}"#;
    assert_eq!(
        describe_refusal(StatusCode::BAD_REQUEST, token),
        "HTTP 400, error invalid_grant"
    );
    // Anything that is not a short code is left out.
    let odd = br#"{"error": {"status": "has spaces, Ben Ortiz", "errors": [{"reason": "x"}]}}"#;
    assert_eq!(
        describe_refusal(StatusCode::FORBIDDEN, odd),
        "HTTP 403, error x"
    );
    assert_eq!(
        describe_refusal(StatusCode::BAD_GATEWAY, b"<html>Ben Ortiz</html>"),
        "HTTP 502"
    );
}
