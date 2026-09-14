-- Tabelle und Spalten der Anmeldebremse auf englische Namen umstellen,
-- passend zu users, api_tokens und notes.
--
-- Als eigene Migration, weil 0003 bereits angewandt sein kann: sqlx fuehrt
-- Pruefsummen ueber angewandte Migrationen und verweigert den Start, wenn
-- sich eine davon nachtraeglich aendert.

ALTER TABLE login_versuche RENAME TO login_attempts;
ALTER TABLE login_attempts RENAME COLUMN schluessel      TO attempt_key;
ALTER TABLE login_attempts RENAME COLUMN fehlversuche    TO failures;
ALTER TABLE login_attempts RENAME COLUMN gesperrt_bis    TO locked_until;
ALTER TABLE login_attempts RENAME COLUMN letzter_versuch TO last_attempt_at;

ALTER INDEX login_versuche_pkey                RENAME TO login_attempts_pkey;
ALTER INDEX login_versuche_letzter_versuch_idx RENAME TO login_attempts_last_attempt_at_idx;
