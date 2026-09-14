# Design-Guide

Wie das Aussehen der Anwendung definiert und angepasst wird.

**Die Kurzfassung:** Alles Visuelle kommt aus zwei Dateien.

| Datei | Zuständig für |
|---|---|
| `assets/css/theme.css` | Farben, Rundung, Schrift, Dichte — **die Stellschrauben** |
| `assets/css/components.css` | Wie ein Knopf, eine Karte, eine Tabelle aussieht |

Wer nur „die Farbe ändern" will, fasst ausschließlich `theme.css` an.

---

## 1. Der schnellste Weg: eine Zahl

Die gesamte Akzentfarbe der Anwendung hängt an einer einzigen Variablen:

```css
/* assets/css/theme.css */
:root {
  --brand-hue: 258;   /* ← diese Zahl */
}
```

| Wert | Ergebnis |
|---|---|
| 25 | Rot |
| 70 | Orange |
| 145 | Grün |
| 195 | Türkis |
| 258 | Indigo (Vorgabe) |
| 300 | Violett |
| 350 | Pink |

Ändern, `just css` ausführen, Seite neu laden — Knöpfe, Links, Abzeichen,
Fokusringe und selbst die leichte Tönung der Hintergründe wandern mit.

> **Drei Stellen folgen der Zahl NICHT von selbst:** die Farbe der Browser- und
> Statusleiste, die App-Icons und die Knöpfe in HTML-Mails. Browser, Telefone und
> Mailprogramme lesen kein Stylesheet — sie brauchen eine feste Farbe.
> Wer `--brand-hue` ändert, zieht deshalb nach:
>
> 1. `PWA_THEME_COLOR` in `.env` (Leiste und Mail-Knöpfe)
> 2. die Füllfarbe in `assets/icons/icon.svg` und `icon-maskable.svg`,
>    danach `./scripts/icons.sh`
>
> Den passenden Hex-Wert zeigt der Browser: im Styleguide mit den
> Entwicklerwerkzeugen die Farbe von `bg-brand` untersuchen.

**Ausprobieren ohne Neustart:** Der [Styleguide](http://localhost:3000/styleguide)
hat Regler für Farbton, Farbkraft und Rundung. Wenn es passt, den Wert in
`theme.css` eintragen.

Zwei weitere Stellschrauben in derselben Datei:

```css
--brand-chroma: 0.15;    /* 0 = grau, 0.10 = zurückhaltend, 0.20 = kräftig */
--ui-radius: 0.625rem;   /* 0 = kantig, 0.5rem = weich, 1.5rem = Pillen */
```

---

## 2. Warum OKLCH statt Hex

Farben sind als `oklch(Helligkeit Farbkraft Farbton)` angegeben:

```css
--ui-brand: oklch(0.55 var(--brand-chroma) var(--brand-hue));
```

Der erste Wert ist die **wahrgenommene** Helligkeit. Das hat einen praktischen
Nutzen: Zwei Farben mit derselben Zahl wirken gleich hell — unabhängig vom
Farbton. Bei Hex-Werten ist das nicht so; `#0000FF` (Blau) wirkt viel dunkler
als `#FFFF00` (Gelb), obwohl beide „voll gesättigt" sind.

Deshalb bleiben hier die Kontraste erhalten, wenn man den Farbton dreht. Man
kann von Indigo auf Gelb wechseln, ohne dass Text auf farbigem Grund plötzlich
unlesbar wird.

---

## 3. Farbrollen statt Farbnamen

Im Markup steht nie eine Farbe, sondern immer eine **Rolle**:

```html
<div class="bg-surface-raised text-content border-border">   ✅
<div class="bg-white text-gray-900 border-gray-200">         ❌
```

Der Unterschied zeigt sich im Dunkelmodus: Die erste Zeile dreht sich
automatisch mit, die zweite bleibt weiß.

### Verfügbare Rollen

**Flächen** — von hinten nach vorn:

| Rolle | Verwendung |
|---|---|
| `surface` | Seitenhintergrund |
| `surface-raised` | Karten, Kopfzeile, Eingabefelder |
| `surface-sunken` | Tabellenkopf, Vertiefungen, Fußzeilen |

**Linien:** `border` (normal), `border-strong` (betont)

**Text** — nach Wichtigkeit:

| Rolle | Verwendung |
|---|---|
| `content` | Fließtext, Überschriften |
| `muted` | Beschriftungen, Nebeninformation |
| `subtle` | Zeitstempel, Platzhalter |

**Marke:** `brand`, `brand-hover`, `brand-content` (Text auf Markenfläche),
`brand-subtle` (zurückhaltende Markenfläche)

**Zustände:** `success`, `warning`, `danger` — jeweils auch als `-subtle`
für die zugehörige Hintergrundfläche.

Jede Rolle gibt es als `bg-`, `text-` und `border-`. Dazu `rounded-ui` für die
projektweit einheitliche Eckenrundung und `ring-focus` für Fokusringe.

> Die Zustandsfarben haben **feste** Farbtöne. Grün bleibt grün, auch wenn die
> Marke auf Rot steht — eine Erfolgsmeldung in Markenrot wäre irreführend.

---

## 4. Hell und Dunkel

Drei Zustände, ohne Zutun korrekt:

1. **Systemeinstellung** (Vorgabe) — folgt `prefers-color-scheme`
2. **Ausdrücklich hell** — `<html data-theme="light">`
3. **Ausdrücklich dunkel** — `<html data-theme="dark">`

Der Umschalter in der Kopfzeile setzt `data-theme` und merkt sich die Wahl im
`localStorage`. Ein kleines Skript im `<head>` setzt sie vor dem ersten
Zeichnen, damit es beim Laden nicht kurz hell aufblitzt.

**Für eigene Komponenten ist nichts zu tun** — solange nur Rollen benutzt
werden, stimmt der Dunkelmodus von allein.

---

## 5. Komponenten

Komponenten sind CSS-Klassen, kein Template-Baukasten:

```html
<button class="btn btn-primary">Speichern</button>
<div class="card"><div class="card-body">…</div></div>
<span class="badge badge-success">aktiv</span>
```

Das ist bewusst so gewählt: Es gibt keine Baustein-Schnittstelle, die man
kennen müsste, und das Aussehen aller Knöpfe ändert man an genau einer Stelle.

### Bestand

| Gruppe | Klassen |
|---|---|
| Knöpfe | `btn` + `btn-primary` / `btn-secondary` / `btn-ghost` / `btn-danger`; Größen `btn-sm` / `btn-lg` / `btn-icon` / `btn-block` |
| Formulare | `field`, `label`, `input`, `textarea`, `select`, `checkbox`, `hint`, `error`, `input-error` |
| Karten | `card`, `card-header`, `card-title`, `card-body`, `card-footer` |
| Abzeichen | `badge` + `badge-neutral` / `-brand` / `-success` / `-warning` / `-danger` |
| Hinweise | `alert` + `alert-info` / `-success` / `-warning` / `-danger` |
| Tabellen | `table-wrap` (scrollt waagerecht) + `table` |
| Navigation | `nav-link`, `nav-link-active` |
| Sonstiges | `empty`, `code`, `code-block`, `toast`, `spinner`, `container-app`, `stack`, `row` |

Alle im Betrieb zu sehen unter [/styleguide](http://localhost:3000/styleguide).

### Eine neue Komponente anlegen

In `assets/css/components.css` innerhalb von `@layer components`:

```css
.stat {
  @apply rounded-ui border border-border bg-surface-raised p-4;
}
.stat-value { @apply text-3xl font-semibold tabular-nums text-content; }
.stat-label { @apply mt-1 text-sm text-muted; }
```

Dann `just css`. Innerhalb von `@apply` **nur** semantische Tokens benutzen —
sonst bricht der Dunkelmodus.

**Wann eine Klasse, wann Utilities direkt?**
Erscheint dieselbe Utility-Kette ein drittes Mal, wird sie eine Klasse. Einmalige
Layouts (Grid-Anordnung einer bestimmten Seite) bleiben im Markup.

---

## 6. Schrift und Dichte

```css
--ui-font-sans: ui-sans-serif, system-ui, …;   /* Systemschrift, lädt nichts nach */
--ui-font-mono: ui-monospace, …;
--ui-control-height: 2.5rem;                   /* Höhe von Knöpfen und Feldern */
--ui-content-width: 72rem;                     /* Breite von .container-app */
```

`--ui-control-height` ist die schnellste Stellschraube für die gefühlte Dichte:
`2.25rem` wirkt kompakt und werkzeugartig, `2.75rem` großzügig.

**Eine Webfont einbinden:** In `templates/layout/base.html` das `<link>` ergänzen
und in `theme.css` `--ui-font-sans` davorsetzen. Immer eine echte Systemschrift
als Rückfallebene lassen.

---

## 7. Schmale Bildschirme

Das Kit ist für beide Fälle gedacht: am Schreibtisch und auf dem Telefon. Der
Anspruch auf schmalen Bildschirmen ist **voll benutzbar** — alles erreichbar,
nichts abgeschnitten, keine waagerechte Scrollleiste — ohne dabei eine eigene
Mobilfassung zu pflegen. Eine Codebasis, zwei Erscheinungsformen.

### Die Navigation wandert nach unten

Ab der Breite `lg` (1024 px) steht die Navigation waagerecht in der Kopfzeile.
Darunter sitzt sie als Leiste **am unteren Rand** — dort sind beim Halten des
Geräts die Daumen, oben käme man nur mit Umgreifen hin.

In die Leiste passen drei bis vier Einträge; alles Weitere steckt hinter
**„Mehr"**, das ein Blatt von unten hereinfährt. Es schließt sich bei Escape,
bei einem Tipp auf den abgedunkelten Hintergrund und beim Verbreitern des
Fensters.

Die Leiste hält mit `env(safe-area-inset-bottom)` Abstand zum Balken, den
iPhones ohne Home-Knopf unten einblenden — sonst liegt der letzte Eintrag
darunter und ist nicht zu treffen.

**Ein neuer Menüpunkt wird nur einmal eingetragen.** Die Einträge stehen im
Makro `nav_links` in `templates/layout/app.html`, das zweimal mit
unterschiedlichen Klassen aufgerufen wird:

```jinja
{% macro nav_links(layout, class_name) %}
  <a href="/reports" class="{{ class_name }} {% if layout.is_active("/reports") %}nav-link-active{% endif %}">Berichte</a>
{% endmacro %}
```

`nav-link` ist die Fassung für die Kopfzeile, `nav-link-mobile` die für das
Blatt — volle Breite und mindestens 44 px hoch, die übliche Mindestgröße für
eine Fläche, die mit dem Finger getroffen wird.

**Die untere Leiste ist bewusst nicht an das Makro gekoppelt.** Sie zeigt eine
Auswahl als Abkürzung und steht als eigenes Markup in `app.html` — mit Symbol
pro Eintrag. Wer einen Punkt auch dort haben will, trägt ihn zusätzlich ein.
Vollständig ist immer das Blatt hinter „Mehr"; niemand verliert also einen
Menüpunkt, nur weil er ihn nicht in die Leiste aufgenommen hat.

> Die Umschaltung liegt bei `lg`, nicht bei `md`: Mit sechs Einträgen für
> Administratoren braucht die Kopfzeile rund 820 px. Wer weitere Einträge
> hinzufügt, prüft die Breite bei 1024 px — reicht sie nicht, auf `xl`
> verschieben (alle `lg:`-Präfixe in `app.html` und die `64rem` im Skript).

### Zeitstempel

Aus der Datenbank kommt alles in UTC. Angezeigt werden soll aber die Ortszeit
des Betrachters — samt Sommerzeit, und auch für jemanden in einer anderen
Zeitzone. Deshalb gibt der Server nur den maschinenlesbaren Wert aus und lässt
den Browser rechnen:

```jinja
{% call ui::datetime(note.created_at) %}{% endcall %}
{% call ui::date(u.created_at) %}{% endcall %}
```

Daraus wird `<time datetime="…Z">13.09.2026, 06:46</time>`; ein kleines Skript
in `base.html` ersetzt den Text durch die Ortszeit und hängt die vollständige
Angabe als Tooltip an. Ohne JavaScript bleibt die UTC-Fassung stehen.

**Nie `{{ value.date_time() }}` direkt ins Template schreiben** — das ist die
UTC-Fassung und damit im Sommer zwei Stunden daneben.

### Der häufigste Layout-Fehler auf schmalen Bildschirmen

Ein Grid- oder Flex-Element hat standardmäßig `min-width: auto` und schrumpft
damit **nie** unter die Mindestbreite seines Inhalts. Steckt etwas Breites darin
— ein langer Befehl in einem Codeblock, eine URL ohne Leerzeichen — sprengt die
Spalte das Layout, obwohl der Inhalt selbst scrollen könnte.

```html
<div class="grid gap-6 lg:grid-cols-2">
  <div class="min-w-0">…</div>   <!-- ✅ darf schrumpfen, Inhalt scrollt -->
  <div>…</div>                   <!-- ❌ drückt über den Rand -->
</div>
```

`.card` bringt `min-w-0` bereits mit. Bei eigenen Grid-Spalten ohne Kartenklasse
muss man es selbst setzen. Elemente mit `overflow-x: auto` — etwa `.table-wrap` —
sind nicht betroffen, dort greift die Regel nicht.

### Was schon von allein passt

- **Zweispaltige Seitenlayouts** brechen über `lg:grid-cols-…` auf schmalen
  Bildschirmen automatisch untereinander.
- **Tabellen** stecken in `.table-wrap` und scrollen dort waagerecht, statt die
  Seite zu verbreitern.
- **Formulare und Karten** sind ohnehin fließend.

### Prüfen

Die Entwicklerwerkzeuge des Browsers auf 375 px stellen und durchklicken. Der
verlässlichste Einzelwert:

```js
document.documentElement.scrollWidth > document.documentElement.clientWidth
```

Ist das `true`, gibt es irgendwo einen waagerechten Überlauf.

---

## 8. Name, Icon und installierte App

Was außerhalb der Seite erscheint — Browser-Tab, Startbildschirm, Mails —
kommt nicht aus `theme.css`, sondern aus Umgebungsvariablen und Dateien:

| Was | Wo |
|---|---|
| Name in Titel, Kopfzeile, installierter App | `APP_NAME` |
| Name unter dem Icon (≈ 12 Zeichen) | `APP_SHORT_NAME` |
| Farbe der Browser-/Statusleiste, Mail-Knöpfe | `PWA_THEME_COLOR` |
| Hintergrund beim Start der App | `PWA_BACKGROUND_COLOR` |
| Icon | `assets/icons/*.svg` → `./scripts/icons.sh` |

Zwei Icon-Fassungen, weil Android Icons je nach Gerät rund, eckig oder als
Tropfen zuschneidet:

- `icon.svg` — mit abgerundeten Ecken, für Browser und Desktop
- `icon-maskable.svg` — vollflächig ohne Rundung, Motiv nur in den mittleren
  80 %, damit beim Zuschneiden nichts verloren geht

In Templates stehen Name und Farbe als `layout.app_name` und
`layout.theme_color` bereit, in Rust über `templates::branding()`.

Was gerade gilt, zeigt die Seite **System** unter `/admin/system`.

---

## 9. Barrierefreiheit

Eingebaut und bitte nicht entfernen:

- **Fokusring** — `:focus-visible` bekommt projektweit einen sichtbaren Ring.
  Nie `outline: none` ohne Ersatz.
- **Reduzierte Bewegung** — bei `prefers-reduced-motion` werden Übergänge
  praktisch abgeschaltet.
- **Kontrast** — die Helligkeitswerte der Rollen sind so gewählt, dass
  Text/Hintergrund die WCAG-AA-Schwelle hält. Wer sie ändert, sollte
  nachmessen (Browser-Entwicklerwerkzeuge zeigen das Kontrastverhältnis an).
- **Meldungen** — der Toast-Bereich ist `role="status"` mit `aria-live="polite"`,
  wird also vorgelesen.

---

## 10. Änderungen prüfen

Nach jeder Anpassung den [Styleguide](http://localhost:3000/styleguide) öffnen und
durchsehen — in beiden Modi. Er zeigt jede Komponente im echten Zustand. Was
dort gut aussieht, sieht überall gut aus.
