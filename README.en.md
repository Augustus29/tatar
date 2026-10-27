# Tatar

**English** · [Türkçe](README.md)

## What is Tatar?

Tatar is a lightweight desktop app that connects your Android or iPhone to your Mac or Windows computer. Its name comes from the *tatar*, the Ottoman-era courier riders who carried news from one post station to the next.

Tatar speaks the [KDE Connect](https://kdeconnect.kde.org/) protocol. The **official KDE Connect app on your phone works unchanged**; you do not need to install anything on the phone side. Tatar is not a fork of the KDE Connect desktop app. It is an independent client written from scratch.

On macOS it lives in the menu bar; on Windows it sits in the system tray. It is built with Tauri 2, React and Rust.

## Features (v1)

- **Battery status:** Shows your phone's charge level and charging state. Notifies you when the battery drops below 15% and when it is fully charged.
- **Ping and find my phone:** Sends a ping to the phone, or makes it ring.
- **Clipboard sync:** Automatic, manual or off. Keeps a clipboard history of up to 20 items. Content that password managers mark as secret is never sent and is never stored in the history.
- **File sharing in both directions:** Send files to your phone by dragging and dropping them or by picking them, and receive files from the phone. Shows progress, with cancel and "Try again" options.
- **Mute when a call comes in:** When your phone rings, Tatar mutes or lowers the computer's sound, and restores the previous volume when the call ends.
- **Global shortcuts:**
  - ⇧⌘C: send the clipboard to the phone
  - ⇧⌘F: send a file
  - ⌥⌘R: make the phone ring
  - ⇧⌘V: open the clipboard history
  - On Windows, Ctrl is used instead of ⌘ and Alt instead of ⌥ (for example Ctrl+Alt+R).
- **Troubleshooting screen:** Checks network, permission and pairing problems one by one with real checks.
- **Turkish and English interface:** Follows the system language by default and can be changed in Settings.
- **Accessibility:** Respects the system "Reduce transparency" and "Reduce motion" settings.

Features not in this version (remote input, SMS, media control, file browsing, running commands, notification mirroring, and similar) are listed in [docs/ROADMAP.md](docs/ROADMAP.md).

## Installation

Tatar is not published as an official release yet. Built files are downloaded from GitHub Actions.

### macOS (macOS 13 or later, Apple Silicon and Intel)

1. Open the repository on GitHub and go to the **Actions** tab.
2. Select the **build** workflow in the list on the left and open the latest successful (green) run.
3. In the **Artifacts** section at the bottom of the page, download **Tatar-macos**. It contains `Tatar.app.zip` and a `.dmg` file.
4. Move `Tatar.app` to your **Applications** folder.

**First launch.** The app is not signed by Apple, so macOS shows a warning the first time:

- **Right-click** `Tatar.app`, choose **Open**, then click **Open** again in the dialog.
- On macOS 15 and later, if that does not work, go to **System Settings → Privacy & Security**, and click **"Open Anyway"** at the bottom of the section.

**Permissions.**

- **Local network access:** If macOS asks for local network access when the app first opens, choose **Allow**. Without it, the phone cannot be found.
- **Keychain:** The private key used for the secure connection is stored in the macOS Keychain. When asked for a password, enter your Mac login password and choose **Always Allow**. Unsigned builds ask again after each update.

### Windows (Windows 10 and 11, x64)

1. Download **Tatar-windows** the same way as above.
2. Run the `*-setup.exe` file (or the `.msi`) from it.
3. If the **Windows SmartScreen** warning appears, click **More info**, then **Run anyway**. This warning is expected because the app is not signed.
4. If the firewall prompt appears, allow access on **private networks**. Allowing it for your home or work network is enough.

## Pairing

1. Make sure the phone and the computer are on the **same Wi-Fi network**.
2. Open the **KDE Connect** app on your phone. Tatar should appear in the list together with your computer's name.
3. Tap your computer's name and choose **Pair**.
4. The same **8-character code** appears on both the phone and the computer. Check that the two codes match exactly.
5. Accept on both sides.

If the computer does not appear in the list, open the **Troubleshooting** screen in Tatar, choose **Add by IP address** (manual add), and enter the phone's IP address.

**Firewall and ports.** Traffic between the phone and the computer needs the ports **UDP and TCP 1714–1764** to be open. Some networks, especially guest networks, prevent devices from seeing each other. In that case, ask your network administrator for access, or put both devices on the same private network.

**If the phone doesn't ring.** On Android, "Find my phone" needs KDE Connect's permissions on the phone: Settings → Apps → KDE Connect → **Notifications** (and **full-screen notifications**, if present) must be on; in KDE Connect, tap your computer → **Plugin settings** → **Ring my phone** must be enabled with its permissions granted.

## Development

### Requirements

- **Xcode Command Line Tools:** `xcode-select --install`
- **Rust:** install with [rustup](https://rustup.rs/)
- **Node.js LTS** (version 22 recommended)

### Commands

```bash
# Install dependencies
npm ci

# Run in development mode (reloads when code changes)
npm run tauri dev

# Build the packaged app and open it (macOS)
npm run app

# Rust tests
cd src-tauri && cargo test

# Format and code checks
npm run typecheck      # TypeScript type check
npm run lint           # ESLint
npm run format:check   # Prettier format check
cd src-tauri
cargo fmt --check      # Rust format check
cargo clippy --all-targets -- -D warnings
```

`npm run app` first closes a running Tatar, builds the app, and opens `src-tauri/target/release/bundle/macos/Tatar.app`.

The same checks and tests also run on GitHub Actions on every push and pull request (`.github/workflows/build.yml`).

## Privacy and security

- **The private key** is stored only in the operating system's keychain (macOS Keychain, Windows Credential Manager). It is never written to a plain file.
- **Secret clipboard content** (text that password managers mark as concealed) is never sent to the phone and never written to the clipboard history.
- **Clipboard history** is kept in memory only, with at most 20 items. The **Clear** button deletes all of it.
- **Unpaired devices** can only send a pairing request. If a paired device's certificate changes, the connection is refused.
- **A pairing request** is never accepted without your approval. The verification code is always shown.
- **Incoming files** are saved only to the download folder you choose. Path characters in file names are removed, and existing files are never overwritten (the new file is saved as `name (1).ext`).
- **Every network message** is treated as untrusted: it has a size limit, and unknown packet types are ignored.
- **Logs** never contain clipboard content, file content or keys. The "Copy diagnostic log" option does not include them either.

## License

Not yet decided.
