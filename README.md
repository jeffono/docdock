# DocDock

English | [Português (Brasil)](README.pt-BR.md)

DocDock is an autohiding Windows sidebar for keeping Markdown notes, images and web pages close at hand. Your notes are ordinary `.md` files in a folder you choose.

- Create, edit and search notes with automatic saving; paste images into notes.
- Pin notes and web pages, browse pages in the panel, and save pages as notes.
- Organize notes in folders, move or rename them, and delete them to the Recycle Bin.
- Choose the panel side, width and theme; navigate with keyboard shortcuts.

## Installation

DocDock requires Windows 10 or 11. The first release, **0.1.0-beta.1**, is a preliminary beta: back up your notes before trying it. Download `DocDock_<version>_x64-setup.exe` from [GitHub Releases](https://github.com/jeffono/docdock/releases) and run it. It installs for the current user without administrator rights and downloads Microsoft Edge WebView2 if it is missing.

The installer is not code-signed. If Windows SmartScreen displays “Windows protected your PC”, select **More info** and then **Run anyway**. DocDock checks for updates when it starts and downloads them in the background; when one is ready, choose **Update available — restart** in the tray menu. Quitting normally does not install it.

## Getting started

Launch DocDock and choose a folder for your notes. Hold the pointer against the right edge of the screen to open the panel, or press **Ctrl+Alt+Space**, which you can change in Preferences > Shortcuts. The tray menu lets you change the side, start DocDock with Windows, install an update or quit. Use the panel pin to keep it open and the gear to change preferences. Press **Ctrl+/** for shortcuts.

## Privacy

Notes are stored locally in your chosen folder. DocDock has no telemetry. At each start it asks GitHub whether a new version exists. Web browsing and web search suggestions contact the sites and search provider you use.

### Protecting notes with Cryptomator

An optional [Cryptomator](https://cryptomator.org) vault encrypts notes for cloud sync and helps protect them if a device is stolen while the vault is locked. DocDock stores no note content outside the notes folder.

1. In Cryptomator, create a vault inside your synced cloud folder and unlock it.
2. In DocDock, choose a folder **inside the mounted vault** as your notes folder.
3. Optionally, select the vault's `vault.cryptomator` file in Preferences. The panel's **Unlock** action opens it in Cryptomator. Windows Hello unlock is configured in Cryptomator.

After updating, confirm the notes folder once with **Use this folder** (or choose it again) if prompted. **Unlock** also appears beside a note with unsaved text.

While the vault is locked, DocDock shows the notes folder as unavailable and never recreates it. Unsaved text stays in memory, with a notice, and is saved when the vault returns; keep DocDock running until it is saved. Deleting in a vault, which has no Recycle Bin, moves the note or folder to a trash inside the notes folder: it stays there, encrypted and synced with the vault, for 30 days, then DocDock deletes it. Restore items, or empty the trash to remove them at once, in Preferences > Trash. DocDock also asks Windows to hide the panel from screen capture (best effort) and keeps copies from notes out of clipboard history and cloud clipboard.

**Moving existing notes into a vault does not remove earlier plaintext versions from your cloud provider's version history or trash, or from backups.** Purge those earlier copies too, or start with new notes in the vault.

## Development

Install Node 24, pnpm, Rust via rustup, and the Visual Studio C++ Build Tools. Then run:

```sh
pnpm install
pnpm tauri dev
```

See [AGENTS.md](AGENTS.md) for contributor instructions and the commands table. Run `pnpm check` before integrating changes.

## License

[MIT](LICENSE)
