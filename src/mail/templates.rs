//! Mail-Vorlagen. Ein Struct je Mail, genau wie bei den Seiten.
//!
//! Neue Mail:
//!   1. Template unter `templates/mail/` anlegen, von `mail/base.html` erben
//!   2. Struct hier ergaenzen — mit `brand` fuer Name und Farbe im Rahmen
//!   3. Versenden mit `Mail::to(..).subject(..).text(..).template(&vorlage)?`
//!
//! Die Textfassung wird bewusst von Hand geschrieben und nicht aus dem HTML
//! erzeugt: Automatisch umgewandelter Text liest sich fast immer schlecht.

use askama::Template;

use crate::config::Branding;

#[derive(Template)]
#[template(path = "mail/testmail.html")]
pub struct TestMail {
    pub brand: Branding,
    pub recipient_name: String,
    pub triggered_by: String,
    pub link: String,
}

impl TestMail {
    /// Die Textfassung zur HTML-Vorlage.
    pub fn text(&self) -> String {
        format!(
            "Hallo {},\n\n\
             diese Testmail bestätigt, dass der E-Mail-Versand von {} funktioniert.\n\n\
             Zur Anwendung: {}\n\n\
             Ausgelöst von {}.",
            self.recipient_name, self.brand.name, self.link, self.triggered_by
        )
    }
}
