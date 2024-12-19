<div style="display: flex; align-items: center; justify-content: center; flex-direction: column;">
  <img src="https://gitlab.opencode.de/bwi/bundesmessenger/info/-/raw/main/images/logo.png?inline=false" alt="BundesMessenger Logo" width="128" height="128">
  <h2>BundesMessenger - Matrix Rust SDK</h2>
</div>

----

BundesMessenger - Matrix Rust SDK ist ein Matrix Client-Server SDK basierend
auf [Matrix Rust SDK](https://github.com/matrix-org/matrix-rust-sdk).
Das SDK wird verwendet
von [BundesMessenger X Android](https://gitlab.opencode.de/bwi/bundesmessenger/clients/bundesmessenger-x-android.git)
und [BundesMessenger X iOS](https://gitlab.opencode.de/bwi/bundesmessenger/clients/bundesmessenger-x-ios.git).

## Repository

https://gitlab.opencode.de/bwi/bundesmessenger/clients/bundesmessenger-matrix-rust-sdk.git

## Fehler und Verbesserungsvorschläge

https://gitlab.opencode.de/bwi/bundesmessenger/clients/bundesmessenger-matrix-rust-sdk/-/issues

## Struktur

Die Struktur des SDKs orientiert sich primär an
der [Struktur des Matrix-Rust-SDK](https://github.com/matrix-org/matrix-rust-sdk?tab=readme-ov-file#project-structure).

Daneben enthält dieses Rust-SDK noch weitere Crates:

* **matirx-sdk-base-bwi** - Alle Bundesmessenger-Erweiterungen, welche keine Abhängigkeiten zu den bestehenden
  matirx-sdk crates haben.
* **matirx-sdk-bwi** - Alle Bundesmessenger-Erweiterungen, welche Abhängigkeiten zu der matirx-sdk crate haben.

## Abhängigkeiten

[Matrix Rust SDK](https://github.com/matrix-org/matrix-rust-sdk)

## Für Entwickler

### Commit-Hooks

Durch das erstmalige ausführen von `cargo test` werden die Git-Hooks automatisch initialisiert.
Ob die Initialisierung erfolgreich war, kann mit `less .git/hooks/pre-commit` überprüft werden.

Wenn der Output `.git/hooks/pre-commit: No such file or directory` lautet, so muss zuerst ein _.git/hooks_-Verzeichnis
mittels `mkdir .git/hooks` erzeugt werden.
Anschließend können mittels `rusty-hook init` die hooks initialisiert werden.

### Für Android

Das Rust-SDK wird mittels eines *.aar Archives in den Android Messenger X eingebunden.
Zur Erstellung dieses Archives wird folgender Befehl im root dieses Projektes ausgeführt:

```./android-scripts/build.sh -p . -t $TARGET_ARCHITECTURE $PROFILE```

Dabei ist `$TARGET_ARCHITECTURE` die Zielarchitektur (z.B. `aarch64-linux-android`, `i686-linux-android` oder
`armv7-linux-androideabi`).
`$PROFILE` kann dabei durch `-r` ersetzt werden, wenn es sich um einen Build für ein Release handeln soll.

Das entstandene *.aar Archiv kann dann von der Android-App verwendet werden.
Genauere Informationen dazu können dem _BundesMessenger X Android_ Projekt entnommen werden.

### Für iOS

Das Rust-SDK wird mittels eines GitSubmoduls eingebunden.
Anschießend wird ein Swift-Package erzeugt, welches von XCode angesprochen werden kann.
Um ein Swift-Package zu erzeugen, steht folgender Befehlt zu Verfügung:

```xtask swift build-framework -t $TARGET_ARCHITECTURE --profile $PROFILE```

Dabei ist `$TARGET_ARCHITECTURE` die Zielarchitektur (z.B. `aarch64-apple-ios`, `aarch64-apple-ios-sim` oder
`x86_64-apple-ios`).
Für `$PROFILE` stehen dabei `bwibuild` (schneller Build) und `bwidbg` (Build für Debugging) zu Verfügung.
Anschließend kann das Rust-SDK über die generierte Package.swift lokal eingebunden werden.

## Rechtliches

Die Lizenz des BundesMessenger - Matrix Rust SDK ist die [Apache License Version 2.0](./LICENSE).

### Copyright

- [BWI GmbH](https://messenger.bwi.de/copyright)
- [Matrix](https://matrix.org/)
