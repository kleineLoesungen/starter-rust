//! Tests ueber den vollstaendigen HTTP-Stack.
//!
//! Getestet wird genau die Anwendung aus `starter::app::build` — mitsamt
//! Sessions, CSRF-Pruefung und allen Schichten. Kein nachgebauter Router.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use sqlx::PgPool;
use starter::auth::{Role, scope};
use starter::config::{Branding, Config};
use starter::domain::{api_token, user};
use starter::mail::Mailer;
use tower::ServiceExt;

/// Baut die Anwendung fuer einen Test.
async fn anwendung(db: PgPool) -> axum::Router {
    let config = Config::for_tests();
    let (router, aufraeumen) = starter::app::build(db, config)
        .await
        .expect("Anwendung baubar");
    aufraeumen.abort();
    router
}

async fn text(antwort: axum::response::Response) -> String {
    let bytes = antwort.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

async fn admin(db: &PgPool, email: &str) -> user::User {
    user::create(
        db,
        email,
        "Testperson",
        "einsehrlangespasswort",
        Role::Admin,
    )
    .await
    .unwrap()
}

/// Stellt ein Maschinen-Token mit genau den angegebenen Berechtigungen aus.
async fn token_mit(db: &PgPool, email: &str, scopes: &[&str]) -> String {
    let a = admin(db, email).await;
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    api_token::create(db, a.id, "Test", &scopes, None)
        .await
        .unwrap()
        .plaintext
}

fn mit_token(request: axum::http::request::Builder, token: &str) -> axum::http::request::Builder {
    request.header(header::AUTHORIZATION, format!("Bearer {token}"))
}

/// Meldet sich an und liefert den Cookie-Kopf fuer weitere Anfragen.
async fn anmelden(app: &axum::Router, email: &str, passwort: &str) -> String {
    let antwort = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header("sec-fetch-site", "same-origin")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "email={email}&password={passwort}&next=/dashboard"
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        antwort.status(),
        StatusCode::SEE_OTHER,
        "Anmeldung fehlgeschlagen"
    );

    let cookie = antwort
        .headers()
        .get(header::SET_COOKIE)
        .expect("Session-Cookie muss gesetzt werden")
        .to_str()
        .unwrap();

    // Nur der Name=Wert-Teil vor dem ersten Semikolon wird zurueckgeschickt.
    cookie.split(';').next().unwrap().to_string()
}

// --- Oeffentliche Routen ----------------------------------------------------

#[sqlx::test]
async fn zustandspruefung_ist_ohne_anmeldung_erreichbar(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    assert!(text(antwort).await.contains("\"status\":\"ok\""));
}

#[sqlx::test]
async fn styleguide_ist_oeffentlich(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/styleguide").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::OK);
}

// --- Session-geschuetzte Routen ---------------------------------------------

#[sqlx::test]
async fn ohne_anmeldung_wird_zum_login_umgeleitet(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/notes").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
    let ziel = antwort
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    // Das urspruengliche Ziel muss erhalten bleiben.
    assert_eq!(ziel, "/login?next=%2Fnotes");
}

#[sqlx::test]
async fn kontoseite_verlangt_anmeldung(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/account").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
}

// --- Geschlossene Selbstregistrierung ---------------------------------------

#[sqlx::test]
async fn registrierung_ist_offen_solange_kein_konto_existiert(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/register").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::OK);
}

#[sqlx::test]
async fn registrierungsseite_ist_nach_dem_ersten_konto_zu(db: PgPool) {
    admin(&db, "erster@example.com").await;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(Request::get("/register").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
    assert_eq!(antwort.headers().get(header::LOCATION).unwrap(), "/login");
}

#[sqlx::test]
async fn registrierungsformular_laesst_sich_nicht_von_hand_abschicken(db: PgPool) {
    // Der entscheidende Test: Das Ausblenden im Template reicht nicht,
    // die Pruefung muss im Handler stehen.
    admin(&db, "erster@example.com").await;
    let app = anwendung(db.clone()).await;

    let antwort = app
        .oneshot(
            Request::post("/register")
                .header("sec-fetch-site", "same-origin")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "display_name=Eindringling&email=fremd@example.com&password=einsehrlangespasswort",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
    assert_eq!(antwort.headers().get(header::LOCATION).unwrap(), "/login");

    // Und es darf wirklich kein zweites Konto entstanden sein.
    assert_eq!(user::count(&db).await.unwrap(), 1);
}

// --- Token-Verwaltung ist Administratoren vorbehalten -----------------------

#[sqlx::test]
async fn token_seite_ist_fuer_normale_benutzer_gesperrt(db: PgPool) {
    let u = user::create(
        &db,
        "normal@example.com",
        "Normal",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();
    let app = anwendung(db).await;

    // Ohne Session: Umleitung. Mit Session als Nicht-Admin: 403.
    // Hier ohne Session — die Rollenpruefung selbst deckt der Domain-Test ab.
    let antwort = app
        .oneshot(Request::get("/admin/tokens").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
    assert_eq!(u.role, Role::User);
}

// --- Token-Authentifizierung ------------------------------------------------

#[sqlx::test]
async fn api_ohne_token_meldet_401_mit_hinweis(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/api/v1/notes").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
    // RFC 6750 verlangt diesen Kopf bei einer 401 auf Bearer-Schutz.
    assert!(antwort.headers().contains_key(header::WWW_AUTHENTICATE));
}

#[sqlx::test]
async fn api_mit_erfundenem_token_meldet_401(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(
            Request::get("/api/v1/notes")
                .header(header::AUTHORIZATION, "Bearer sk_aabbccdd_erfunden")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn token_endpunkt_zeigt_die_eigenen_berechtigungen(db: PgPool) {
    let token = token_mit(&db, "info@example.com", &[scope::NOTES_READ]).await;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::get("/api/v1/token"), &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    let body = text(antwort).await;
    assert!(body.contains("notes:read"));
    // Hinter einem Token steht kein Benutzer — es darf auch keiner auftauchen.
    assert!(!body.contains("info@example.com"));
}

// --- Scopes -----------------------------------------------------------------

#[sqlx::test]
async fn lesescope_erlaubt_lesen(db: PgPool) {
    let token = token_mit(&db, "lesen@example.com", &[scope::NOTES_READ]).await;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::get("/api/v1/notes"), &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    assert_eq!(text(antwort).await, r#"{"data":[]}"#);
}

#[sqlx::test]
async fn lesescope_erlaubt_kein_schreiben(db: PgPool) {
    let a = admin(&db, "nurlesen@example.com").await;
    let scopes = vec![scope::NOTES_READ.to_string()];
    let token = api_token::create(&db, a.id, "Nur lesen", &scopes, None)
        .await
        .unwrap()
        .plaintext;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::post("/api/v1/notes"), &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"user_id":"{}","title":"Verboten"}}"#,
                    a.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::FORBIDDEN);
    let body = text(antwort).await;
    assert!(body.contains("insufficient_scope"));
    // Der Client soll erfahren, welche Berechtigung ihm fehlt.
    assert!(body.contains("notes:write"));
}

#[sqlx::test]
async fn falscher_scope_hilft_nicht(db: PgPool) {
    // Ein Token mit users:read darf trotzdem keine Notizen lesen.
    let token = token_mit(&db, "quer@example.com", &[scope::USERS_READ]).await;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::get("/api/v1/notes"), &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn schreibscope_legt_notiz_fuer_angegebenen_benutzer_an(db: PgPool) {
    let a = admin(&db, "schreiben@example.com").await;
    let scopes = vec![scope::NOTES_WRITE.to_string()];
    let token = api_token::create(&db, a.id, "Schreiber", &scopes, None)
        .await
        .unwrap()
        .plaintext;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::post("/api/v1/notes"), &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"user_id":"{}","title":"Aus dem Test","body":"Inhalt"}}"#,
                    a.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::CREATED);
    let body = text(antwort).await;
    assert!(body.contains("Aus dem Test"));
    // Der Besitzer wird ausdruecklich mitgegeben, nicht aus dem Token abgeleitet.
    assert!(body.contains(&a.id.to_string()));
}

#[sqlx::test]
async fn benutzerliste_braucht_ihren_eigenen_scope(db: PgPool) {
    let token = token_mit(&db, "leute@example.com", &[scope::USERS_READ]).await;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::get("/api/v1/users"), &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    let body = text(antwort).await;
    assert!(body.contains("leute@example.com"));
    // Passwortdaten duerfen die Schnittstelle niemals verlassen.
    assert!(!body.contains("password_hash"));
    assert!(!body.contains("$argon2"));
}

#[sqlx::test]
async fn api_weist_ungueltige_eingabe_mit_400_ab(db: PgPool) {
    let a = admin(&db, "validierung@example.com").await;
    let scopes = vec![scope::NOTES_WRITE.to_string()];
    let token = api_token::create(&db, a.id, "Schreiber", &scopes, None)
        .await
        .unwrap()
        .plaintext;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::post("/api/v1/notes"), &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"user_id":"{}","title":"   "}}"#,
                    a.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
    assert!(text(antwort).await.contains("bad_request"));
}

// --- CSRF -------------------------------------------------------------------

#[sqlx::test]
async fn veraendernde_anfrage_von_fremder_herkunft_wird_abgewiesen(db: PgPool) {
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            Request::post("/login")
                .header("sec-fetch-site", "cross-site")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("email=a@b.de&password=x"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn veraendernde_anfrage_von_eigener_herkunft_geht_durch(db: PgPool) {
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            Request::post("/login")
                .header("sec-fetch-site", "same-origin")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=unbekannt@example.com&password=falsch&next=/",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Nicht 403: Die Herkunftspruefung laesst sie durch, erst die Anmeldung scheitert.
    assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn die_json_api_braucht_keine_herkunftspruefung(db: PgPool) {
    // Ein Bearer-Token wird vom Browser nie automatisch mitgeschickt,
    // deshalb ist die API kein CSRF-Ziel.
    let a = admin(&db, "csrf@example.com").await;
    let scopes = vec![scope::NOTES_WRITE.to_string()];
    let token = api_token::create(&db, a.id, "Schreiber", &scopes, None)
        .await
        .unwrap()
        .plaintext;
    let app = anwendung(db).await;

    let antwort = app
        .oneshot(
            mit_token(Request::post("/api/v1/notes"), &token)
                .header(header::CONTENT_TYPE, "application/json")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from(format!(
                    r#"{{"user_id":"{}","title":"Von woanders"}}"#,
                    a.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::CREATED);
}

// --- Formular mit Mehrfachauswahl -------------------------------------------

#[sqlx::test]
async fn token_formular_nimmt_mehrere_berechtigungen_entgegen(db: PgPool) {
    // Regressionstest. Mehrere Kontrollkaestchen mit demselben Namen senden
    // den Schluessel wiederholt. `axum::extract::Form` kann das nicht in ein
    // Vec einlesen und antwortet mit 422 — deshalb benutzt der Handler
    // `axum_extra::extract::Form`. Ohne diesen Test faellt ein Rueckbau nicht auf.
    admin(&db, "formular@example.com").await;
    let app = anwendung(db.clone()).await;
    let cookie = anmelden(&app, "formular@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .clone()
        .oneshot(
            Request::post("/admin/tokens")
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "name=Zwei+Rechte&expires_in_days=90&scopes=notes%3Aread&scopes=notes%3Awrite",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        antwort.status(),
        StatusCode::OK,
        "Formular mit zwei Scopes muss durchgehen"
    );

    let tokens = api_token::list(&db).await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].has_scope(scope::NOTES_READ));
    assert!(tokens[0].has_scope(scope::NOTES_WRITE));
}

#[sqlx::test]
async fn token_formular_ohne_berechtigung_wird_abgewiesen(db: PgPool) {
    admin(&db, "ohnerecht@example.com").await;
    let app = anwendung(db.clone()).await;
    let cookie = anmelden(&app, "ohnerecht@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .clone()
        .oneshot(
            Request::post("/admin/tokens")
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("name=Nutzlos&expires_in_days=90"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
    assert!(api_token::list(&db).await.unwrap().is_empty());
}

// --- Rollen in der Oberflaeche ----------------------------------------------

#[sqlx::test]
async fn normaler_benutzer_kommt_nicht_an_die_verwaltung(db: PgPool) {
    admin(&db, "chef@example.com").await;
    user::create(
        &db,
        "normal@example.com",
        "Normal",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();

    let app = anwendung(db).await;
    let cookie = anmelden(&app, "normal@example.com", "einsehrlangespasswort").await;

    for pfad in ["/admin/tokens", "/admin/users"] {
        let antwort = app
            .clone()
            .oneshot(
                Request::get(pfad)
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            antwort.status(),
            StatusCode::FORBIDDEN,
            "{pfad} muss gesperrt sein"
        );
    }

    // Die eigene Kontoseite steht dagegen jedem offen.
    let antwort = app
        .oneshot(
            Request::get("/account")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::OK);
}

#[sqlx::test]
async fn administrator_erreicht_die_verwaltung(db: PgPool) {
    admin(&db, "chefin@example.com").await;
    let app = anwendung(db).await;
    let cookie = anmelden(&app, "chefin@example.com", "einsehrlangespasswort").await;

    for pfad in ["/admin/tokens", "/admin/users", "/account", "/moderation"] {
        let antwort = app
            .clone()
            .oneshot(
                Request::get(pfad)
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            antwort.status(),
            StatusCode::OK,
            "{pfad} muss erreichbar sein"
        );
    }
}

// --- Mobilfaehigkeit --------------------------------------------------------

#[sqlx::test]
async fn navigation_ist_auf_beiden_bildschirmgroessen_vollstaendig(db: PgPool) {
    // Auf schmalen Bildschirmen sitzt die Navigation unten, der Rest hinter
    // „Mehr". Fehlt eines davon, sind Konto und Abmelden auf dem Telefon nicht
    // mehr erreichbar — von aussen unsichtbar, deshalb dieser Test.
    admin(&db, "mobil@example.com").await;
    let app = anwendung(db).await;
    let cookie = anmelden(&app, "mobil@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .oneshot(
            Request::get("/dashboard")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    let html = text(antwort).await;

    assert!(html.contains(r#"class="tabbar"#), "Untere Leiste fehlt");
    assert!(html.contains("data-menu-toggle"), "Knopf für „Mehr“ fehlt");
    assert!(html.contains(r#"id="main-menu""#), "Menü fehlt");

    // Kopfzeile: Navigation für breite Bildschirme.
    let kopf_start = html.find("<header").expect("Kopfzeile vorhanden");
    let kopf_ende = html.find("</header>").expect("Kopfzeile geschlossen");
    let kopf = &html[kopf_start..kopf_ende];
    assert!(
        kopf.contains(r#"href="/admin/tokens""#),
        "Navigationseintrag fehlt in der Kopfzeile"
    );

    // Menü: dieselben Einträge plus Konto und Abmelden.
    let menue_start = html.find(r#"id="main-menu""#).expect("Menü vorhanden");
    let menue = &html[menue_start..];
    assert!(
        menue.contains(r#"href="/admin/tokens""#),
        "Navigationseintrag fehlt im Menü"
    );
    assert!(
        menue.contains("nav-link-mobile"),
        "Mobile Navigationseinträge fehlen"
    );
    assert!(menue.contains(r#"href="/account""#), "Konto fehlt im Menü");
    assert!(
        menue.contains(r#"action="/logout""#),
        "Abmelden fehlt im Menü"
    );
}

// --- Bremse gegen Passwort-Raten --------------------------------------------

/// Ein Anmeldeversuch mit falschem Passwort.
async fn fehlversuch(app: &axum::Router, email: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post("/login")
                .header("sec-fetch-site", "same-origin")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "email={email}&password=falschespasswort&next=/"
                )))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[sqlx::test]
async fn drei_versuche_stehen_frei_zur_verfuegung(db: PgPool) {
    // Vertipper sollen nicht sofort ausgesperrt werden.
    admin(&db, "tippt@example.com").await;
    let app = anwendung(db).await;

    for nr in 1..=3 {
        let antwort = fehlversuch(&app, "tippt@example.com").await;
        assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
        assert!(
            !text(antwort).await.contains("Zu viele Fehlversuche"),
            "Versuch {nr} von 3 darf noch nicht gebremst werden"
        );
    }
}

#[sqlx::test]
async fn nach_zwei_fehlversuchen_klappt_das_richtige_passwort_noch(db: PgPool) {
    admin(&db, "zweimal@example.com").await;
    let app = anwendung(db).await;

    fehlversuch(&app, "zweimal@example.com").await;
    fehlversuch(&app, "zweimal@example.com").await;

    let cookie = anmelden(&app, "zweimal@example.com", "einsehrlangespasswort").await;
    assert!(cookie.contains(starter::app::SESSION_COOKIE));
}

#[sqlx::test]
async fn der_vierte_versuch_wird_gebremst(db: PgPool) {
    admin(&db, "raten@example.com").await;
    let app = anwendung(db).await;

    for _ in 1..=3 {
        fehlversuch(&app, "raten@example.com").await;
    }

    let antwort = fehlversuch(&app, "raten@example.com").await;
    assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
    let html = text(antwort).await;
    assert!(
        html.contains("Zu viele Fehlversuche"),
        "Nach drei Fehlversuchen muss der naechste gebremst werden"
    );
    // Die verbleibende Zeit gehoert in die Meldung, sonst taeppt man im Dunkeln.
    assert!(
        html.contains("Sekunden"),
        "Die Wartezeit muss genannt werden"
    );
}

#[sqlx::test]
async fn waehrend_der_sperre_hilft_auch_das_richtige_passwort_nicht(db: PgPool) {
    // Sonst waere die Bremse wirkungslos: Wer das Passwort errraet, kaeme
    // trotz laufender Sperre sofort hinein.
    admin(&db, "gesperrt@example.com").await;
    let app = anwendung(db).await;

    for _ in 1..=3 {
        fehlversuch(&app, "gesperrt@example.com").await;
    }

    let antwort = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header("sec-fetch-site", "same-origin")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=gesperrt@example.com&password=einsehrlangespasswort&next=/",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::UNAUTHORIZED);
    assert!(text(antwort).await.contains("Zu viele Fehlversuche"));
}

#[sqlx::test]
async fn erfolgreiche_anmeldung_loescht_die_vorgeschichte(db: PgPool) {
    admin(&db, "vergesslich@example.com").await;
    let app = anwendung(db.clone()).await;

    fehlversuch(&app, "vergesslich@example.com").await;
    fehlversuch(&app, "vergesslich@example.com").await;
    anmelden(&app, "vergesslich@example.com", "einsehrlangespasswort").await;

    let (offen,): (i64,) = sqlx::query_as("SELECT count(*) FROM login_attempts")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        offen, 0,
        "Nach erfolgreicher Anmeldung darf nichts stehen bleiben"
    );

    // Und der Zaehler beginnt wieder bei null.
    for _ in 1..=3 {
        let antwort = fehlversuch(&app, "vergesslich@example.com").await;
        assert!(!text(antwort).await.contains("Zu viele Fehlversuche"));
    }
}

#[sqlx::test]
async fn die_sperre_gilt_nur_fuer_das_betroffene_konto(db: PgPool) {
    // Ohne Verbindungsinformation zaehlt nur der Kontoschluessel — ein anderes
    // Konto darf davon nicht betroffen sein.
    admin(&db, "opfer@example.com").await;
    admin(&db, "unbeteiligt@example.com").await;
    let app = anwendung(db).await;

    for _ in 1..=4 {
        fehlversuch(&app, "opfer@example.com").await;
    }

    let cookie = anmelden(&app, "unbeteiligt@example.com", "einsehrlangespasswort").await;
    assert!(cookie.contains(starter::app::SESSION_COOKIE));
}

// --- Fehlerseite -------------------------------------------------------------

#[sqlx::test]
async fn fehlerseite_gibt_eingaben_nur_maskiert_aus(db: PgPool) {
    // Regressionstest fuer ein reflektiertes XSS: Die Fehlerseite wird ohne
    // Askama gebaut, und manche Meldungen enthalten die Eingabe des Benutzers
    // ("Unbekannte Rolle: …"). Ohne Maskierung liefe eingeschleustes Skript.
    let a = admin(&db, "xss@example.com").await;
    let app = anwendung(db).await;
    let cookie = anmelden(&app, "xss@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .oneshot(
            Request::post(format!("/admin/users/{}", a.id))
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "display_name=X&email=xss@example.com&role=%3Cscript%3Ealert(1)%3C%2Fscript%3E",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
    let html = text(antwort).await;
    assert!(
        !html.contains("<script>alert(1)</script>"),
        "Eingabe ungefiltert in der Seite"
    );
    assert!(
        html.contains("&lt;script&gt;"),
        "Eingabe muss maskiert erscheinen"
    );
}

// --- Installierbare App (PWA) ----------------------------------------------

#[sqlx::test]
async fn manifest_nennt_name_farben_und_vorhandene_icons(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .clone()
        .oneshot(
            Request::get("/manifest.webmanifest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    assert_eq!(
        antwort.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/manifest+json"
    );

    let manifest: serde_json::Value = serde_json::from_str(&text(antwort).await).unwrap();
    assert_eq!(manifest["name"], Branding::default().name);
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["theme_color"], "#3370c7");

    // Jedes eingetragene Icon muss auch wirklich ausgeliefert werden — ein
    // Tippfehler im Pfad macht die App sonst still nicht installierbar.
    let icons = manifest["icons"].as_array().expect("icons ist eine Liste");
    assert!(icons.iter().any(|i| i["sizes"] == "192x192"));
    assert!(icons.iter().any(|i| i["sizes"] == "512x512"));
    for icon in icons {
        let pfad = icon["src"].as_str().unwrap();
        let antwort = app
            .clone()
            .oneshot(Request::get(pfad).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(antwort.status(), StatusCode::OK, "Icon fehlt: {pfad}");
    }
}

#[sqlx::test]
async fn service_worker_liegt_im_wurzelpfad_und_wird_nicht_zwischengespeichert(db: PgPool) {
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/sw.js").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::OK);
    assert!(
        antwort
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/javascript")
    );
    // Sonst haengt nach einem Deployment der alte Worker fest.
    assert_eq!(
        antwort.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache"
    );

    let skript = text(antwort).await;
    assert!(
        !skript.contains("__VERSION__"),
        "Cache-Version wurde nicht eingesetzt"
    );
    // Seiten duerfen nie aus dem Speicher kommen, nur die Offline-Seite als Ersatz.
    assert!(skript.contains(r#"request.mode === "navigate""#));
    assert!(skript.contains(r#"caches.match("/offline")"#));
}

#[sqlx::test]
async fn offline_seite_ist_ohne_anmeldung_erreichbar(db: PgPool) {
    // Der Service Worker legt sie beim Installieren ab — dabei ist niemand
    // angemeldet. Eine Umleitung zum Login waere hier ein stiller Fehler.
    let app = anwendung(db).await;
    let antwort = app
        .oneshot(Request::get("/offline").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::OK);
    assert!(text(antwort).await.contains("Keine Verbindung"));
}

#[sqlx::test]
async fn jede_seite_verweist_auf_manifest_und_leistenfarbe(db: PgPool) {
    let app = anwendung(db).await;
    let html = text(
        app.oneshot(Request::get("/login").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
    assert!(html.contains(r##"<meta name="theme-color" content="#3370c7">"##));
    assert!(html.contains("apple-touch-icon"));
}

// --- Systemseite und Mail ---------------------------------------------------

#[sqlx::test]
async fn systemseite_ist_nur_fuer_administratoren(db: PgPool) {
    admin(&db, "chef@example.com").await;
    user::create(
        &db,
        "normal@example.com",
        "Normal",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();
    let app = anwendung(db).await;

    let normal = anmelden(&app, "normal@example.com", "einsehrlangespasswort").await;
    let antwort = app
        .clone()
        .oneshot(
            Request::get("/admin/system")
                .header(header::COOKIE, &normal)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::FORBIDDEN);

    let chef = anmelden(&app, "chef@example.com", "einsehrlangespasswort").await;
    let antwort = app
        .oneshot(
            Request::get("/admin/system")
                .header(header::COOKIE, &chef)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(antwort.status(), StatusCode::OK);
}

#[sqlx::test]
async fn systemseite_zeigt_mailserver_aber_nie_das_passwort(db: PgPool) {
    admin(&db, "chefin@example.com").await;
    let mut config = Config::for_tests();
    config.mail.smtp_url = Some("smtps://versand:streng-geheim-42@mail.example.com:465".into());
    let (app, aufraeumen) = starter::app::build(db, config).await.unwrap();
    aufraeumen.abort();

    let cookie = anmelden(&app, "chefin@example.com", "einsehrlangespasswort").await;
    let html = text(
        app.oneshot(
            Request::get("/admin/system")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;

    assert!(html.contains("mail.example.com:465"));
    assert!(html.contains("versand"));
    assert!(html.contains("Passwort gesetzt"));
    assert!(
        !html.contains("streng-geheim-42"),
        "Das SMTP-Passwort darf nie angezeigt werden"
    );
}

/// Anwendung mit einem Mailer, der Mails sammelt statt verschickt.
async fn anwendung_mit_postfach(
    db: PgPool,
) -> (
    axum::Router,
    std::sync::Arc<std::sync::Mutex<Vec<starter::mail::Mail>>>,
) {
    let (mailer, postfach) = Mailer::in_memory();
    let (app, aufraeumen) = starter::app::build_with_mailer(db, Config::for_tests(), mailer)
        .await
        .unwrap();
    aufraeumen.abort();
    (app, postfach)
}

#[sqlx::test]
async fn testmail_kommt_mit_text_und_html_an(db: PgPool) {
    admin(&db, "postbote@example.com").await;
    let (app, postfach) = anwendung_mit_postfach(db).await;
    let cookie = anmelden(&app, "postbote@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .oneshot(
            Request::post("/admin/system/testmail")
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("to=empfaenger%40example.com"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
    let ziel = antwort
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ziel, "/admin/system?mail=sent");
    assert!(
        !ziel.contains("empfaenger"),
        "Die Adresse gehoert nicht in die URL"
    );

    let mails = postfach.lock().unwrap();
    assert_eq!(mails.len(), 1);
    let mail = &mails[0];
    assert_eq!(mail.recipient(), "empfaenger@example.com");
    assert!(mail.subject_line().contains(&Branding::default().name));
    assert!(mail.text_body().contains("funktioniert"));
    let html = mail.html_body().expect("HTML-Fassung vorhanden");
    assert!(html.contains("#3370c7"), "Markenfarbe fehlt im Mail-Layout");
}

#[sqlx::test]
async fn testmail_an_ungueltige_adresse_wird_abgewiesen(db: PgPool) {
    admin(&db, "sorgfalt@example.com").await;
    let (app, postfach) = anwendung_mit_postfach(db).await;
    let cookie = anmelden(&app, "sorgfalt@example.com", "einsehrlangespasswort").await;

    let antwort = app
        .oneshot(
            Request::post("/admin/system/testmail")
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("to=keine-adresse"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
    assert!(text(antwort).await.contains("Versand fehlgeschlagen"));
    assert!(postfach.lock().unwrap().is_empty());
}
