# Overlay Deep-Dive Vorbereitung (2026-02-26)

## Kontext
- Gemeldetes Problem: HAL und KITT wirken seit letztem Update "falsch skaliert", insbesondere die maximale Breite in Pixeln.
- Zusatzfrage: Ob das neue Refinement-Overlay die normalen Overlay-Dimensionen ueberschreibt.

## Was ich bereits verifiziert habe

### 1) Refinement-Overlay ueberschreibt die Breitenlogik nicht direkt
- Die Breiten-/Radiusberechnung bleibt in `window.setOverlayLevel(...)`:
  - `overlay.html:424` bis `overlay.html:435`
- KITT-Dimensionen werden weiterhin nur ueber `window.setKittDimensions(...)` gesetzt:
  - `overlay.html:393` bis `overlay.html:411`
- Das Refinement-Overlay setzt nur Dataset/Appearance (Preset, Farbe, Speed, Range):
  - `overlay.html:371` bis `overlay.html:391`
  - `overlay.html:468` bis `overlay.html:490`

Kurz: Kein direkter Codepfad, der `kittMinWidth/kittMaxWidth/dotMaxRadius` durch Refinement ersetzt.

### 2) Backend-Settings werden in einem Rutsch angewendet
- Zentrale Anwendung:
  - `src-tauri/src/overlay.rs:174` bis `src-tauri/src/overlay.rs:220`
- Enthalten sind sowohl Refinement-Parameter als auch KITT/Dot-Dimensionen in derselben Eval-Chain.
- Overlay-Ready-Reapply existiert:
  - `src-tauri/src/lib.rs:2714` bis `src-tauri/src/lib.rs:2725`

### 3) Audio-Level ist weiterhin der harte Multiplikator fuer "wie nah an max"
- Levelpfad:
  - `src-tauri/src/audio.rs:275` bis `src-tauri/src/audio.rs:335`
- Wenn der geclampte Level selten nahe `1.0` kommt, erreicht die Anzeige nie die eingestellte Maximalbreite, auch wenn Slider korrekt sind.

## Relevante Commits seit der Regression
- `2c998c6`: Overlay-Defaults geaendert (u.a. KITT max width 200 -> 700, Radius/Fall/Rise angepasst).
- `da3bbbf`: Overlay-Fenstergroesse erweitert (Platz fuer Transcribe-Indikator).
- `ce7f276`: Refinement-Indikator in `overlay.html` + Settings-Plumbing.

Diese drei Commits sind der beste Startpunkt fuer den morgigen Vergleich.

## Aktuelle Hypothesen (priorisiert)
1. Wahrnehmung vs. Logik: Max-Werte sind gesetzt, aber Live-Level erreicht selten hohe Werte -> wirkt wie "max greift nicht".
2. Window-Footprint-Aenderung (da3bbbf) veraendert visuelle Referenz (Element wirkt relativ kleiner/groesser als erwartet).
3. Refinement-Indikator beeinflusst die Wahrnehmung (insb. bei KITT), aber nicht direkt die KITT/HAL-Rechenpfade.
4. Edge-Case bei Settings-Rennen (weniger wahrscheinlich, wegen overlay:ready-Reapply, aber pruefbar).

## Debug-Plan fuer morgen (konkret)

### A) Harte Trennung: "Dimension kaputt" vs "Level zu niedrig"
1. In Overlay-DevTools manuell testen:
   - `window.setOverlayStyle('kitt')`
   - `window.setKittDimensions(20, 800, 13)`
   - `window.setOverlayLevel(1.0)`
2. Erwartung:
   - KITT-Bar muss exakt sichtbar auf den Max-Wert gehen.
3. Wenn ja: Problem ist nicht die Breitenformel, sondern Upstream-Level/Mapping.

### B) Live-Level Distribution sichtbar machen
1. Kurzzeit-Logging fuer 10-20s:
   - Eingehende Level (`clamped`) aus `OverlayLevelEmitter`.
   - Optional 95th percentile fuer Session ausgeben.
2. Ziel:
   - Fakten, ob `level` typischerweise z.B. nur bei 0.2-0.4 liegt.

### C) Refinement-Einfluss isolieren
1. Gleicher Audio-Input in 2 Runs:
   - Run 1: Refinement-Indikator aus.
   - Run 2: Refinement-Indikator an (gleiches Preset/Range).
2. Vergleichen:
   - Berechnete KITT/Dot-Werte muessen identisch sein.
   - Nur visuelle Zusatzebene darf unterschiedlich sein.

### D) Fenstergeometrie validieren
1. Loggen bei `apply_overlay_settings`:
   - `style`, `kitt_min_width`, `kitt_max_width`, `max_radius`, berechnetes `window width/height`.
2. Ziel:
   - Sicherstellen, dass keine ungewollte harte Begrenzung/Clipping im Window entsteht.

## Schnellantwort auf die Frage "ueberschreibt Refinement was?"
- Nach aktuellem Code-Stand: Nein, nicht direkt.
- Es setzt zusaetzliche visuelle Layer-Parameter, aber nicht die KITT/HAL-Grunddimensionen.
- Ein indirekter visueller Eindruck ist moeglich (mehr Overlays gleichzeitig), aber keine direkte Width-Override-Logik gefunden.

