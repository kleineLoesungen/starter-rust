-- Bremse gegen das Durchprobieren von Passwoertern.
--
-- Gezaehlt wird unter zwei Schluesseln gleichzeitig:
--   "email:<adresse>"  bremst Angriffe auf EIN Konto, auch von vielen Rechnern
--   "ip:<adresse>"     bremst EINEN Rechner, auch gegen viele Konten
--
-- Beide werden vor jedem Anmeldeversuch geprueft; die laengere Sperre gewinnt.

CREATE TABLE login_versuche (
    schluessel      TEXT PRIMARY KEY,
    fehlversuche    INTEGER     NOT NULL DEFAULT 0,
    gesperrt_bis    TIMESTAMPTZ,
    letzter_versuch TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Fuer das Aufraeumen alter Eintraege.
CREATE INDEX login_versuche_letzter_versuch_idx ON login_versuche (letzter_versuch);
