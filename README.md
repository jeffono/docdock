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

## Development

Install Node 24, pnpm, Rust via rustup, and the Visual Studio C++ Build Tools. Then run:

```sh
pnpm install
pnpm tauri dev
```

See [AGENTS.md](AGENTS.md) for contributor instructions and the commands table. Run `pnpm check` before integrating changes.

## License

[MIT](LICENSE)
