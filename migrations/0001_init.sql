-- Grundschema: Benutzer, Rollen, API-Tokens und eine Beispielressource.
-- Benoetigt PostgreSQL 13+ (wegen gen_random_uuid() im Kern).

-- Rollen. Reihenfolge = Rangfolge, siehe Role::rank() in src/auth/role.rs.
CREATE TYPE user_role AS ENUM ('user', 'moderator', 'admin');

-- updated_at automatisch pflegen.
CREATE FUNCTION set_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,   -- immer kleingeschrieben gespeichert
    display_name  TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,          -- Argon2id
    role          user_role   NOT NULL DEFAULT 'user',
    is_active     BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- API-Tokens. Ausgegeben wird "sk_<token_id>_<secret>".
-- Gespeichert wird nur token_id (Klartext, zum Nachschlagen) und der
-- SHA-256-Hash des Geheimnisses. Das Geheimnis selbst ist nach der Ausgabe weg.
CREATE TABLE api_tokens (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    token_id     TEXT        NOT NULL UNIQUE,
    token_hash   TEXT        NOT NULL,
    scopes       TEXT[]      NOT NULL DEFAULT '{}',
    last_used_at TIMESTAMPTZ,
    expires_at   TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX api_tokens_user_id_idx ON api_tokens (user_id);

-- Beispielressource. Zeigt das komplette Muster: Domain-Service, HTML-Routen
-- mit htmx und JSON-API. Zum Anpassen umbenennen oder loeschen.
CREATE TABLE notes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX notes_user_id_created_at_idx ON notes (user_id, created_at DESC);

CREATE TRIGGER notes_updated_at BEFORE UPDATE ON notes
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
