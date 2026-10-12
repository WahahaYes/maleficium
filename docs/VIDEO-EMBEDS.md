# Embedding a YouTube or Vimeo video

A paper bundle can show a YouTube or Vimeo video inside an interactive figure. There is no embed widget type. You write a small `html` widget that frames the provider's player, and you declare the player's origin so that one widget may frame it.

If you can ship the video file, use `\interactivevideo` with a local file instead. The bundle then stays offline and hashed, with no third party involved. The rest of this page is about what you give up by not doing that.

## The widget

Make a folder next to your main file, for example `widgets/talk/`, with an `index.html`:

```html
<!doctype html>
<title>Talk</title>
<style>
  html,
  body {
    margin: 0;
    height: 100%;
  }
  iframe {
    border: 0;
    width: 100%;
    height: 100%;
  }
</style>
<iframe
  src="https://www.youtube-nocookie.com/embed/VIDEO_ID"
  title="Conference talk"
  allow="encrypted-media; picture-in-picture"
></iframe>
```

For Vimeo, use `https://player.vimeo.com/video/VIDEO_ID?dnt=1` and declare `https://player.vimeo.com`.

Then declare the origin, in `widget.json`, in the macro, or both. The two lists are merged.

In `widgets/talk/widget.json`:

```json
{ "csp": { "frameDomains": ["https://www.youtube-nocookie.com"] } }
```

Or as a macro option:

```latex
\begin{figure}
\centering
\interactive[poster=figures/talk.png, height=6cm, alt={Recorded talk},
  framedomains=https://www.youtube-nocookie.com]{widgets/talk/}
\caption{The talk. Video: \url{https://www.youtube.com/watch?v=VIDEO_ID}}
\end{figure}
```

Separate several origins with spaces.

Give the widget a `poster=`. The app renders posters with declared origins unreachable, so an automatic poster of this widget would show an empty frame.

### Resource and connect origins

`resourcedomains=` (or `resourceDomains` in `widget.json`) declares origins the widget may load images, scripts, styles, fonts and media from. `connectDomains` is declared in `widget.json` only.

These two lists take effect only in a folder bundle. In a single-file bundle each widget runs inside the reader page and is held to its policy too, which allows declared frame origins and nothing else from outside. If a widget needs files from another origin there, export the folder profile, or put the files in the widget's folder.

## What the declaration does

- **It covers one widget.** The origins go into that widget's own content security policy. No other widget in the bundle can use them. The exported bundle enforces this in Chromium, Firefox and WebKit.
- **The origins are part of the approval.** An `html` widget runs in the app only after you approve it in View > Widgets, which shows its declared origins. Changing the folder's files or its origins, in `widget.json` or the macro, asks for approval again. Auto-approval never approves an origin the last approval did not include.
- **One approval per folder.** If two documents in a project use the same folder with different macro origins, approving one leaves the other unapproved. To avoid the back and forth, give each document its own copy of the folder, or declare the origins in `widget.json` so both share them.
- **Preview in Browser and exports are not gated.** Preview in Browser opens a single-file bundle in your browser, and every `html` widget in it runs there, approved or not. An exported bundle runs wherever someone opens it.
- **The PDF is not affected.** It shows the poster.

## Which origins are accepted

An origin is `https://`, a lowercase host name or IPv4 address, and an optional port, for example `https://www.youtube-nocookie.com` or `https://media.example.org:8443`. The widget list and the export both refuse anything else, including:

- other schemes (`http:`, `data:`, `blob:`, `javascript:`)
- wildcards (`https://*.example.org`) and CSP keywords (`'self'`, `*`)
- paths, queries, fragments and credentials (`https://example.org/embed`, `https://user@example.org`)
- uppercase hosts, trailing dots, and ports outside 1-65535 or written with leading zeros

A widget can declare at most 16 origins, counting every list in both places (`MAX_ORIGINS` in `src-tauri/core/src/widgets.rs`).

## Before you publish

**Referrer and sandbox.** Every widget frame is sandboxed with scripts allowed and no origin of its own, and sends no referrer. A player framed inside it inherits that: no cookies or storage, no pop-ups, no fullscreen, no `Referer` header. Some providers refuse embeds under these conditions. YouTube, for example, has shown "Error 153" for embeds that send no referrer. Maleficium's tests do not load a real YouTube or Vimeo player, so check your embed in the browsers your readers use. Keep the poster and a plain link to the video in the caption, so readers still get the video if the player refuses to run.

**Tracking.** The player loads as soon as a reader opens the page. The provider then sees the reader's IP address and browser. `youtube-nocookie.com` and Vimeo's `dnt=1` reduce cookies but do not stop tracking. A bundle with no declared origins makes no request outside itself; an embed gives that up for this figure. Say so if your venue or readers care.

**Link rot.** The provider can remove the video, make it private, block it in some regions or change its embed rules at any time, and the bundle cannot tell. The PDF and the poster stay. If the licence allows it, archive a copy of the video somewhere that keeps files long term, and cite that copy in the caption.

**Browsers are not egress-proof.** A content security policy limits what a page may load. It does not govern every connection the browser makes: preconnects, prerenders and blocked frames can still open connections. So declared origins limit what a widget can load and show, not every packet the reader's machine sends. Inside the app, on Linux, the editor window and the poster renderer turn speculative loading off. On macOS and Windows the webview offers no setting for it, so the app sets nothing there.
