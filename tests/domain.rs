//! Tests der Fachlogik gegen eine echte Datenbank.
//!
//! `#[sqlx::test]` legt fuer JEDEN Test eine eigene, frisch migrierte Datenbank
//! an und raeumt sie danach weg. Die Tests beeinflussen sich also nicht
//! gegenseitig und koennen parallel laufen.
//!
//! Voraussetzung: `DATABASE_URL` zeigt auf eine erreichbare Postgres.
//! Lokal genuegt `just db-up`.

use sqlx::PgPool;
use starter::auth::{Role, scope};
use starter::domain::{api_token, note, user};
use starter::error::Error;

async fn testbenutzer(db: &PgPool, email: &str, role: Role) -> user::User {
    user::create(db, email, "Testperson", "einsehrlangespasswort", role)
        .await
        .expect("Benutzer anlegen muss klappen")
}

// --- Benutzer ---------------------------------------------------------------

#[sqlx::test]
async fn email_wird_normalisiert(db: PgPool) {
    let u = testbenutzer(&db, "  Gross.Klein@Example.COM ", Role::User).await;
    assert_eq!(u.email, "gross.klein@example.com");

    // Anmeldung muss auch mit abweichender Schreibweise gehen.
    let angemeldet =
        user::authenticate(&db, "GROSS.KLEIN@example.com", "einsehrlangespasswort").await;
    assert!(angemeldet.is_ok());
}

#[sqlx::test]
async fn doppelte_email_wird_abgewiesen(db: PgPool) {
    testbenutzer(&db, "doppelt@example.com", Role::User).await;
    let zweiter = user::create(
        &db,
        "doppelt@example.com",
        "Andere",
        "einsehrlangespasswort",
        Role::User,
    )
    .await;
    assert!(matches!(zweiter, Err(Error::Conflict(_))));
}

#[sqlx::test]
async fn zu_kurzes_passwort_wird_abgewiesen(db: PgPool) {
    let ergebnis = user::create(&db, "kurz@example.com", "Test", "kurz", Role::User).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn falsches_passwort_meldet_unauthorized(db: PgPool) {
    testbenutzer(&db, "pw@example.com", Role::User).await;
    let ergebnis = user::authenticate(&db, "pw@example.com", "falschespasswort123").await;
    assert!(matches!(ergebnis, Err(Error::Unauthorized)));
}

#[sqlx::test]
async fn unbekanntes_konto_meldet_dasselbe_wie_falsches_passwort(db: PgPool) {
    // Wichtig: Der Fehler darf nicht verraten, ob es das Konto gibt.
    let ergebnis = user::authenticate(&db, "gibtesnicht@example.com", "irgendeinpasswort").await;
    assert!(matches!(ergebnis, Err(Error::Unauthorized)));
}

#[sqlx::test]
async fn gesperrtes_konto_kann_sich_nicht_anmelden(db: PgPool) {
    let u = testbenutzer(&db, "gesperrt@example.com", Role::User).await;
    user::set_active(&db, u.id, false).await.unwrap();

    let ergebnis = user::authenticate(&db, "gesperrt@example.com", "einsehrlangespasswort").await;
    assert!(matches!(ergebnis, Err(Error::Forbidden)));
}

#[sqlx::test]
async fn passwort_wird_niemals_im_klartext_gespeichert(db: PgPool) {
    let u = testbenutzer(&db, "hash@example.com", Role::User).await;
    assert!(!u.password_hash.contains("einsehrlangespasswort"));
    assert!(u.password_hash.starts_with("$argon2"));
}

// --- Suche ------------------------------------------------------------------

#[sqlx::test]
async fn suche_findet_name_und_email_unabhaengig_von_gross_klein(db: PgPool) {
    user::create(
        &db,
        "anna.beispiel@example.com",
        "Anna Beispiel",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();
    user::create(
        &db,
        "bernd@example.com",
        "Bernd Muster",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();

    let nach_name = user::list(&db, Some("anna")).await.unwrap();
    assert_eq!(nach_name.len(), 1);
    assert_eq!(nach_name[0].display_name, "Anna Beispiel");

    let nach_email = user::list(&db, Some("BERND@")).await.unwrap();
    assert_eq!(nach_email.len(), 1);

    // Leerer Begriff und None liefern beide alles.
    assert_eq!(user::list(&db, Some("   ")).await.unwrap().len(), 2);
    assert_eq!(user::list(&db, None).await.unwrap().len(), 2);
}

#[sqlx::test]
async fn suche_behandelt_prozentzeichen_als_text(db: PgPool) {
    // Ohne Maskierung waere "%" ein Platzhalter und wuerde alles finden.
    user::create(
        &db,
        "normal@example.com",
        "Ohne Sonderzeichen",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();
    user::create(
        &db,
        "rabatt@example.com",
        "50% Rabatt",
        "einsehrlangespasswort",
        Role::User,
    )
    .await
    .unwrap();

    let treffer = user::list(&db, Some("%")).await.unwrap();
    assert_eq!(
        treffer.len(),
        1,
        "Nur der Name mit echtem Prozentzeichen zaehlt"
    );
    assert_eq!(treffer[0].display_name, "50% Rabatt");

    // Dasselbe fuer den Unterstrich.
    assert!(user::list(&db, Some("_")).await.unwrap().is_empty());
}

// --- Rollen -----------------------------------------------------------------

#[sqlx::test]
async fn rollenrang_wird_gespeichert_und_geladen(db: PgPool) {
    let u = testbenutzer(&db, "mod@example.com", Role::Moderator).await;
    assert_eq!(u.role, Role::Moderator);
    assert!(u.can(Role::User));
    assert!(u.can(Role::Moderator));
    assert!(!u.can(Role::Admin));

    let hochgestuft = user::set_role(&db, u.id, Role::Admin).await.unwrap();
    assert!(hochgestuft.can(Role::Admin));
}

// --- API-Tokens (Maschinen-Clients) ----------------------------------------

#[sqlx::test]
async fn token_authentifiziert_und_wird_nur_gehasht_gespeichert(db: PgPool) {
    let admin = testbenutzer(&db, "token@example.com", Role::Admin).await;
    let scopes = vec![scope::NOTES_READ.to_string()];
    let erzeugt = api_token::create(&db, admin.id, "Testtoken", &scopes, None)
        .await
        .unwrap();

    // Der Klartext darf nirgends in der Datenbank auftauchen.
    assert!(!erzeugt.token.token_hash.contains(&erzeugt.plaintext));

    let geprueft = api_token::authenticate(&db, &erzeugt.plaintext)
        .await
        .unwrap();
    assert_eq!(geprueft.id, erzeugt.token.id);
    // Nachvollziehbarkeit: Wer hat es ausgestellt?
    assert_eq!(geprueft.created_by, Some(admin.id));
}

#[sqlx::test]
async fn token_ohne_scope_wird_abgelehnt(db: PgPool) {
    // Ein Token ohne Berechtigung koennte nichts und waere fast immer ein Versehen.
    let admin = testbenutzer(&db, "leer@example.com", Role::Admin).await;
    let ergebnis = api_token::create(&db, admin.id, "Nutzlos", &[], None).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn token_mit_unbekanntem_scope_wird_abgelehnt(db: PgPool) {
    // Sonst entstuende ein Token, das wegen eines Tippfehlers still nichts darf.
    let admin = testbenutzer(&db, "tippfehler@example.com", Role::Admin).await;
    let scopes = vec!["notes:reed".to_string()];
    let ergebnis = api_token::create(&db, admin.id, "Vertippt", &scopes, None).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn token_kennt_nur_seine_eigenen_scopes(db: PgPool) {
    let admin = testbenutzer(&db, "scopes@example.com", Role::Admin).await;
    let scopes = vec![scope::NOTES_READ.to_string()];
    let erzeugt = api_token::create(&db, admin.id, "Nur lesen", &scopes, None)
        .await
        .unwrap();

    assert!(erzeugt.token.has_scope(scope::NOTES_READ));
    assert!(!erzeugt.token.has_scope(scope::NOTES_WRITE));
    assert!(!erzeugt.token.has_scope(scope::USERS_READ));
}

#[sqlx::test]
async fn widerrufenes_token_wird_abgewiesen(db: PgPool) {
    let admin = testbenutzer(&db, "widerruf@example.com", Role::Admin).await;
    let scopes = vec![scope::NOTES_READ.to_string()];
    let erzeugt = api_token::create(&db, admin.id, "Weg damit", &scopes, None)
        .await
        .unwrap();

    api_token::revoke(&db, erzeugt.token.id).await.unwrap();

    let ergebnis = api_token::authenticate(&db, &erzeugt.plaintext).await;
    assert!(matches!(ergebnis, Err(Error::Unauthorized)));
}

#[sqlx::test]
async fn abgelaufenes_token_wird_abgewiesen(db: PgPool) {
    let admin = testbenutzer(&db, "abgelaufen@example.com", Role::Admin).await;
    let scopes = vec![scope::NOTES_READ.to_string()];
    let gestern = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    let erzeugt = api_token::create(&db, admin.id, "Alt", &scopes, Some(gestern))
        .await
        .unwrap();

    let ergebnis = api_token::authenticate(&db, &erzeugt.plaintext).await;
    assert!(matches!(ergebnis, Err(Error::Unauthorized)));
}

#[sqlx::test]
async fn token_ueberlebt_das_loeschen_seines_erstellers(db: PgPool) {
    // Tokens gehoeren der Installation, nicht der Person. Ein ausscheidender
    // Administrator darf keine Maschinen-Zugaenge mit ins Grab nehmen.
    let admin = testbenutzer(&db, "geht@example.com", Role::Admin).await;
    // Zweiter Admin, damit das Loeschen nicht am Letzter-Admin-Schutz scheitert.
    testbenutzer(&db, "bleibt@example.com", Role::Admin).await;

    let scopes = vec![scope::NOTES_READ.to_string()];
    let erzeugt = api_token::create(&db, admin.id, "Dienst", &scopes, None)
        .await
        .unwrap();

    user::delete(&db, admin.id).await.unwrap();

    let geprueft = api_token::authenticate(&db, &erzeugt.plaintext)
        .await
        .unwrap();
    assert_eq!(geprueft.created_by, None);
    assert!(geprueft.has_scope(scope::NOTES_READ));
}

// --- Schutz des letzten Administrators --------------------------------------

#[sqlx::test]
async fn letzter_admin_kann_nicht_degradiert_werden(db: PgPool) {
    let admin = testbenutzer(&db, "einziger@example.com", Role::Admin).await;
    let ergebnis = user::set_role(&db, admin.id, Role::User).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn letzter_admin_kann_nicht_gesperrt_werden(db: PgPool) {
    let admin = testbenutzer(&db, "einziger2@example.com", Role::Admin).await;
    let ergebnis = user::set_active(&db, admin.id, false).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn letzter_admin_kann_nicht_geloescht_werden(db: PgPool) {
    let admin = testbenutzer(&db, "einziger3@example.com", Role::Admin).await;
    let ergebnis = user::delete(&db, admin.id).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

#[sqlx::test]
async fn mit_zweitem_admin_ist_alles_erlaubt(db: PgPool) {
    let a = testbenutzer(&db, "admin-a@example.com", Role::Admin).await;
    testbenutzer(&db, "admin-b@example.com", Role::Admin).await;

    // Jetzt gibt es einen Ersatz, also darf a degradiert werden.
    let herabgestuft = user::set_role(&db, a.id, Role::User).await.unwrap();
    assert_eq!(herabgestuft.role, Role::User);
}

#[sqlx::test]
async fn ein_gesperrter_admin_zaehlt_nicht_als_ersatz(db: PgPool) {
    let aktiv = testbenutzer(&db, "aktiv@example.com", Role::Admin).await;
    let gesperrt = testbenutzer(&db, "inaktiv@example.com", Role::Admin).await;
    user::set_active(&db, gesperrt.id, false).await.unwrap();

    // Der gesperrte Admin kann sich nicht anmelden, ist also kein Ersatz.
    assert_eq!(user::count_active_admins(&db).await.unwrap(), 1);
    let ergebnis = user::set_role(&db, aktiv.id, Role::User).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}

// --- Profil aendern ---------------------------------------------------------

#[sqlx::test]
async fn profil_wird_geaendert_und_email_normalisiert(db: PgPool) {
    let u = testbenutzer(&db, "alt@example.com", Role::User).await;
    let neu = user::update_profile(&db, u.id, "  Neuer Name  ", " NEU@Example.com ")
        .await
        .unwrap();

    assert_eq!(neu.display_name, "Neuer Name");
    assert_eq!(neu.email, "neu@example.com");
}

#[sqlx::test]
async fn profil_mit_fremder_email_wird_abgewiesen(db: PgPool) {
    testbenutzer(&db, "belegt@example.com", Role::User).await;
    let b = testbenutzer(&db, "frei@example.com", Role::User).await;

    let ergebnis = user::update_profile(&db, b.id, "Name", "belegt@example.com").await;
    assert!(matches!(ergebnis, Err(Error::Conflict(_))));
}

#[sqlx::test]
async fn geaendertes_passwort_gilt_sofort(db: PgPool) {
    let u = testbenutzer(&db, "wechsel@example.com", Role::User).await;
    user::change_password(&db, u.id, "eingenauessehrlangespasswort")
        .await
        .unwrap();

    assert!(
        user::authenticate(&db, "wechsel@example.com", "einsehrlangespasswort")
            .await
            .is_err()
    );
    assert!(
        user::authenticate(&db, "wechsel@example.com", "eingenauessehrlangespasswort")
            .await
            .is_ok()
    );
}

// --- Notizen: Datentrennung -------------------------------------------------

#[sqlx::test]
async fn notizen_sind_zwischen_benutzern_getrennt(db: PgPool) {
    let a = testbenutzer(&db, "eigner@example.com", Role::User).await;
    let b = testbenutzer(&db, "fremder@example.com", Role::User).await;

    let eingabe = note::NoteInput {
        title: "Geheim".into(),
        body: "Nur fuer A".into(),
    };
    let notiz = note::create(&db, a.id, &eingabe).await.unwrap();

    // B sieht sie nicht in der Liste ...
    assert!(note::list_for_user(&db, b.id).await.unwrap().is_empty());

    // ... und bekommt beim direkten Zugriff 404 statt 403, damit die Existenz
    // fremder IDs nicht bestaetigt wird.
    let ergebnis = note::get_owned(&db, b.id, notiz.id).await;
    assert!(matches!(ergebnis, Err(Error::NotFound)));

    // Auch Aendern und Loeschen greifen nicht.
    assert!(
        note::update_owned(&db, b.id, notiz.id, &eingabe)
            .await
            .is_err()
    );
    assert!(note::delete_owned(&db, b.id, notiz.id).await.is_err());

    // Fuer A ist alles unveraendert da.
    assert_eq!(note::list_for_user(&db, a.id).await.unwrap().len(), 1);
}

#[sqlx::test]
async fn leerer_titel_wird_abgewiesen(db: PgPool) {
    let u = testbenutzer(&db, "titel@example.com", Role::User).await;
    let eingabe = note::NoteInput {
        title: "   ".into(),
        body: String::new(),
    };
    assert!(matches!(
        note::create(&db, u.id, &eingabe).await,
        Err(Error::BadRequest(_))
    ));
}

#[sqlx::test]
async fn benutzer_loeschen_entfernt_seine_notizen(db: PgPool) {
    // Absicherung der ON DELETE CASCADE-Regel aus der Migration.
    let u = testbenutzer(&db, "kaskade@example.com", Role::User).await;
    let eingabe = note::NoteInput {
        title: "Bleibt nicht".into(),
        body: String::new(),
    };
    note::create(&db, u.id, &eingabe).await.unwrap();

    user::delete(&db, u.id).await.unwrap();

    let (notizen,): (i64,) = sqlx::query_as("SELECT count(*) FROM notes")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(notizen, 0);
}

#[sqlx::test]
async fn systemweite_notiz_funktionen_ignorieren_den_besitzer(db: PgPool) {
    // Diese Fassung bedient die Maschinen-Schnittstelle. Sie darf ueber alle
    // Besitzer hinweg arbeiten — der Zugriff wird ueber Scopes geregelt.
    let a = testbenutzer(&db, "sys-a@example.com", Role::User).await;
    let b = testbenutzer(&db, "sys-b@example.com", Role::User).await;

    let von_a = note::create(
        &db,
        a.id,
        &note::NoteInput {
            title: "A".into(),
            body: String::new(),
        },
    )
    .await
    .unwrap();
    note::create(
        &db,
        b.id,
        &note::NoteInput {
            title: "B".into(),
            body: String::new(),
        },
    )
    .await
    .unwrap();

    assert_eq!(note::list_all(&db, None).await.unwrap().len(), 2);
    assert_eq!(note::list_all(&db, Some(a.id)).await.unwrap().len(), 1);
    assert_eq!(note::get(&db, von_a.id).await.unwrap().title, "A");
}

#[sqlx::test]
async fn notiz_fuer_unbekannten_benutzer_wird_freundlich_abgewiesen(db: PgPool) {
    // Kommt vor, wenn die Schnittstelle eine erfundene user_id schickt.
    let eingabe = note::NoteInput {
        title: "Ins Leere".into(),
        body: String::new(),
    };
    let ergebnis = note::create(&db, uuid::Uuid::new_v4(), &eingabe).await;
    assert!(matches!(ergebnis, Err(Error::BadRequest(_))));
}
