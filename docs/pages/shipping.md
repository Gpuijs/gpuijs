# Packaging for macOS

`gpuijs build` turns an app directory into a self-contained `.app` bundle. The user doesn't need Bun, Node or Rust installed.

```sh
bunx gpuijs build .                                                   # unsigned, for this machine
bunx gpuijs build . --sign "Developer ID Application: Jane Doe (TEAMID)"
```

The result is `<app-dir>/.gpuijs/<Name>.app`.

## Name, identifier and version

They come from the app's `package.json`:

```json
{
  "name": "my-app",
  "version": "1.0.0",
  "gpuijs": { "name": "My App", "id": "com.example.myapp" }
}
```

| Field | Becomes | Default |
| --- | --- | --- |
| `gpuijs.name` | The bundle name, `CFBundleName`, `CFBundleDisplayName`, the executable name and `app.getName()` | `productName`, then `name`, then the directory name |
| `gpuijs.id` | `CFBundleIdentifier`, and the name of the data and cache directories | `dev.gpuijs.<name>` |
| `version` | `CFBundleVersion` and `CFBundleShortVersionString` | `0.0.0` |

The name can't contain `/` or `:`. The id may only use letters, digits, `.` and `-`. The build stops with an error otherwise.

The window's size, minimum size and title bar style come from `new Window(...)` in your code, as in development.

## What's in the bundle

```text
My App.app/
└── Contents/
    ├── Info.plist          name, id, version, NSHighResolutionCapable
    ├── MacOS/
    │   └── My App          the release host (from gpuijs-darwin-*)
    └── Resources/
        ├── app             the app compiled into a standalone Bun executable
        ├── app.css         the compiled stylesheet
        ├── public/         a copy of the app's public/ directory
        └── gpuijs.json     { "name", "id" }, read by the host at launch
```

At launch, the host finds `gpuijs.json` next to it, starts `Resources/app` as its JavaScript sidecar, and gives it the paths below. The bundle is built in production mode (`NODE_ENV=production`) and minified. Classes from `index.html`'s `<body>` are compiled into it.

## Where the app keeps things

| What | Where |
| --- | --- |
| `localStorage` | `~/Library/Application Support/<id>/localStorage.json` |
| Downloaded remote images | `~/Library/Caches/<id>/` |
| Output from the host and the app, errors included | `~/Library/Logs/<Name>.log`, replaced each launch |

Apps launched from Finder have no terminal, so `console.log` and uncaught errors go to the log file. Check it first when a packaged app misbehaves:

```sh
tail -f ~/Library/Logs/"My App.log"
```

## Signing

`--sign <identity>` signs the app with the **hardened runtime**, which notarization requires, and the entitlements in `js/bin/entitlements.plist`. Those entitlements allow JIT and unsigned executable memory, which Bun's JavaScriptCore needs. The sidecar (`Resources/app`) is signed first, then the bundle that seals it.

List your signing identities with:

```sh
security find-identity -v -p codesigning
```

Use a **Developer ID Application** identity for apps you distribute outside the App Store. An **Apple Development** identity is enough for testing on your own machines.

## Notarizing

Gatekeeper blocks downloaded apps that aren't notarized. Notarize the signed app, then staple the ticket:

```sh
cd examples/my-app/.gpuijs
ditto -c -k --keepParent "My App.app" app.zip
xcrun notarytool submit app.zip --keychain-profile <profile> --wait
xcrun stapler staple "My App.app"
```

Create the keychain profile once with `xcrun notarytool store-credentials`.

## Distributing

Zip the stapled app (`ditto -c -k --keepParent`) or put it on a disk image (`hdiutil create -srcfolder "My App.app" -volname "My App" MyApp.dmg`), then sign and notarize the disk image too if you ship one.

## Not yet supported

- **App icons.** No `.icns` is generated yet. Add one yourself as `Contents/Resources/AppIcon.icns`, add `CFBundleIconFile` to `Info.plist`, and re-sign.
- **Universal binaries.** The host is built for the machine's architecture. For Apple silicon and Intel, build on each, or build the host with `--target` and `lipo`, and point `GPUIJS_HOST` at the result.
- **Auto-update, file associations, URL schemes** and other `Info.plist` keys. Add them by hand after building, and re-sign.
- **Windows and Linux.**

## Build options

| Variable | Effect |
| --- | --- |
| `GPUIJS_HOST` | Package this host binary instead of the installed `gpuijs-darwin-*` one (or, in a gpuijs checkout, a fresh `cargo build --release`) |
