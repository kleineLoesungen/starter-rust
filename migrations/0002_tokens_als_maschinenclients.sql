-- App-Tokens sind Maschinen-Clients, keine Benutzer-Zugaenge.
--
-- Vorher: Ein Token uebernahm Identitaet und Rolle des Benutzers, dem es
-- gehoerte. Damit konnte ein Token alles, was die Person konnte.
--
-- Jetzt: Ein Token gehoert niemandem. `created_by` haelt nur fest, WER es
-- ausgestellt hat, und darf leer werden, wenn dieses Konto spaeter verschwindet.
-- Was ein Token darf, steht ausschliesslich in `scopes`.

ALTER TABLE api_tokens DROP CONSTRAINT api_tokens_user_id_fkey;
ALTER TABLE api_tokens RENAME COLUMN user_id TO created_by;
ALTER TABLE api_tokens ALTER COLUMN created_by DROP NOT NULL;

-- SET NULL statt CASCADE: Ein ausscheidender Administrator darf nicht die
-- Maschinen-Zugaenge mit ins Grab nehmen.
ALTER TABLE api_tokens ADD CONSTRAINT api_tokens_created_by_fkey
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL;

ALTER INDEX api_tokens_user_id_idx RENAME TO api_tokens_created_by_idx;

-- Hinweis: Bereits ausgestellte Tokens haben `scopes = '{}'` und damit ab
-- sofort keine Rechte mehr. Das ist Absicht — sie stammen aus dem alten
-- Modell, in dem Rechte woanders herkamen. Neu ausstellen.
