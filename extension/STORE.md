# Store listings

Everything the Chrome Web Store, Microsoft Edge Add-ons and Firefox Add-ons
(AMO) ask for. Build the upload packages with `node build.mjs && node pack.mjs`:

| Store | Upload | Notes |
|---|---|---|
| Chrome Web Store | `dist/kudmx-chrome.zip` | no `key` in the manifest; the store assigns an id |
| Microsoft Edge Add-ons | `dist/kudmx-chrome.zip` | same package |
| Firefox Add-ons | `dist/kudmx.xpi` | id `kudownloader@kuduy.digital` |

After the first store release, add the store's extension id so the browser
connector accepts it: **Settings › Browser › Extra extension ids** (or
`CHROME_EXTENSION_ID` in `crates/ku-proto` for a built-in default).

## One-time setup (done by the publisher)

1. **Chrome Web Store** — register at
   <https://chrome.google.com/webstore/devconsole> (one-time registration
   fee), create the item by uploading `kudmx-chrome.zip`, fill in the listing
   below, submit for review. For automatic updates from CI create an OAuth
   client and add the repository secrets `CWS_EXTENSION_ID`, `CWS_CLIENT_ID`,
   `CWS_CLIENT_SECRET`, `CWS_REFRESH_TOKEN`
   (see <https://github.com/fregante/chrome-webstore-upload-keys>).
2. **Edge Add-ons** — register at <https://partner.microsoft.com/dashboard/microsoftedge>
   (free), upload the same zip.
3. **Firefox** — create an account at <https://addons.mozilla.org/developers/>,
   generate API keys, add `AMO_JWT_ISSUER` / `AMO_JWT_SECRET` as repository
   secrets. CI then signs every release (`kudmx.signed.xpi`, unlisted — installs
   permanently from the GitHub release). For a public listing, submit
   `kudmx.xpi` once on AMO.

## Listing text

**Name:** KuDownloader

**Short description (≤132 chars):**
Send downloads, videos and links to KuDownloader — a fast download manager with resume, queues and video downloads.

**Category:** Productivity (Chrome) · Download management (Firefox)

**Description:**

> KuDownloader integration for your browser.
>
> • Downloads you start in the browser open KuDownloader's "Download File"
>   window: pick the folder, then download with multiple connections, pause
>   and resume, queues and schedules.
> • A KuDownload button appears on videos on web pages — choose a quality and
>   the video downloads in the app (yt-dlp).
> • Right-click a link, image or page: "Download with KuDownloader",
>   "Download all links…".
> • Works with the KuDownloader desktop app for Windows, macOS and Linux
>   (free, open source): https://github.com/gmsakibursabbir/ku-download-manager
>
> The extension only talks to KuDownloader on your own computer (native
> messaging). It does not collect, sell or send your data anywhere else.

**Single purpose:** Hand downloads, media and links from the browser to the
KuDownloader desktop application installed on the same computer.

## Permission justifications

| Permission | Why |
|---|---|
| `downloads` | Take over a download the user starts (when enabled) and cancel the browser copy so KuDownloader downloads it instead. |
| `nativeMessaging` | The only channel to the KuDownloader app on the same computer. |
| `cookies` | Send the site's cookies for the file being downloaded, so downloads that need a signed-in session work in the app. Only for the download's own site, only when a download is handed over. |
| `contextMenus` | "Download with KuDownloader" and "Download all links" in the right-click menu. |
| `storage` | Remember the extension's own settings (pause interception, per-site choices). |
| `scripting`, `tabs` | Read the links of the current page for "Download all links", and show the video download button on the active tab. |
| `webRequest` | Notice media streams and file responses (e.g. `Content-Disposition: attachment`) so they can be offered for download; requests are not modified or blocked. |
| `notifications` | Tell the user when a hand-over fails (e.g. the app is not installed). |
| Host permission `<all_urls>` | The video button and link collection must work on any site the user visits; nothing is read or sent unless the user uses the extension. |

**Remote code:** none — all code is in the package.

**Data use:** No user data is collected or transmitted to the developer or
third parties. Download URLs, the page address and the download's cookies are
passed only to the KuDownloader app on the same device.

**Privacy policy URL:** <https://gmsakibursabbir.github.io/ku-download-manager/privacy.html>

## Screenshots (1280×800)

`docs/screenshots/` — main window, download popup, progress window, video
button, browser integration.

## Data collection answers

- **Firefox (manifest):** `data_collection_permissions: { required: ["none"] }`.
  The extension sends page addresses, media links and site cookies only to the
  KuDownloader app on the same computer (native messaging); nothing goes to
  the developer or third parties. Requires Firefox 140+ (ESR 140 included).
- **Chrome Web Store privacy tab:** no user data is collected or transmitted
  off the device. Permission justifications:
  - *downloads* — hand browser downloads to KuDownloader.
  - *nativeMessaging* — talk to the KuDownloader app on this computer.
  - *cookies* — send the site's cookies with a download so sign-in-only files work.
  - *contextMenus* — "Download with KuDownloader" on links, media and pages.
  - *storage* — remember the take-over switch and per-site choices.
  - *scripting* and host permissions — show the KuDownload button on videos
    and collect a page's links when asked.
  - *webRequest* — detect the video and audio streams a page plays.
  - *tabs* — know which page a download or media stream came from.
  - *notifications* — report when a hand-off fails.
