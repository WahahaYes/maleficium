# Embedding a YouTube or Vimeo video

A paper bundle can show a video hosted by YouTube or Vimeo inside an interactive figure. There is no embed widget type. You write a small custom `html` widget that frames the provider's player, and you declare the player's origin so the exporter lets that one widget frame it.

If you can ship the video file, use `\interactivevideo` with a local file instead. That keeps the bundle offline and hashed, with no third party involved. Everything below is about the cost of leaving that default.

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

Then declare the origin. Use one of the two places below, or both; the two lists are merged.

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

`resourcedomains=` (or `resourceDomains` in `widget.json`) works the same way, for images, scripts, styles, fonts and media the widget loads from another origin. `connectDomains` can be declared in `widget.json` only. Put several origins in one option by separating them with spaces.

Give the widget a `poster=`. The app renders posters with every declared origin turned off, so an automatic poster of this widget would show an empty frame.

## What the declaration does

- **One widget, one policy.** The origins go into that widget's own content security policy (`frame-src` for `frameDomains`). No other widget in the bundle gets them, so another widget cannot frame your player's origin.
- **The reader allows only the union.** A single-file bundle runs each widget inside the reader page, and that page's policy applies too. Its `frame-src` is exactly the set of frame origins your widgets declared, and nothing else. A folder or hosted bundle loads each widget as a document of its own, so the reader page needs no frame origins and its policy stays `frame-src 'self'`.
- **The origins are part of the approval.** An `html` widget runs only after you approve it in View > Widgets, which shows the declared origins. The approval covers the folder's files and the declared origins together. Changing either one, in `widget.json` or in the macro option, makes the widget need approval again. Auto-approval never covers an origin the last approval did not include.
- **The PDF is not affected.** The PDF shows the poster. The declaration matters only in the exported bundle.

## Which origins are accepted

An origin is `https://`, a lowercase host name or IPv4 address, and an optional port, for example `https://www.youtube-nocookie.com` or `https://media.example.org:8443`. The widget list refuses anything else, and so does the export:

- schemes other than `https` (`http:`, `data:`, `blob:`, `javascript:`, `ws:`)
- wildcards (`https://*.example.org`) and CSP keywords (`'self'`, `*`)
- paths, queries, fragments and credentials (`https://example.org/embed`, `https://user@example.org`)
- uppercase hosts, empty or over-long labels, trailing dots, port 0, ports above 65535, and ports with leading zeros

A widget can declare at most 16 origins, counting every list and both places together.

## Before you publish

**Referrer and sandbox.** Every widget frame is sandboxed with scripts allowed and no origin of its own, and it sends no referrer. A player framed inside it inherits that sandbox: it gets no cookies or storage, cannot open pop-ups, and cannot go fullscreen. The request for the player carries no `Referer` header. Some providers refuse embeds under these conditions. YouTube, for example, has shown "Error 153" when an embed sends no referrer. Maleficium's tests use local stand-in servers and have never loaded a real YouTube or Vimeo player. Check your embed in the browsers your readers use. Keep the poster and a plain link to the video in the caption, so readers still get the video if the player refuses to run.

**Tracking.** The player loads as soon as a reader opens the page. The provider then sees the reader's IP address, browser and the page the request came from, as far as the browser reveals it. `youtube-nocookie.com` and Vimeo's `dnt=1` cut down on cookies, but they do not stop tracking. A bundle with no declared origins makes no request outside itself. A bundle with an embed loses that property for this figure. Say so if your venue or readers care.

**Link rot.** The provider can remove the video, make it private, block it in some regions or change its embed rules at any time. The bundle cannot detect any of this. The PDF of record and the poster stay. If the licence allows it, put a copy of the video in an archive that keeps files long term, and cite that copy in the caption.

**Browsers are not egress-proof.** A content security policy tells the browser what a page may load. It is not a network firewall. Browsers make some connections that the policy does not govern. WebKit can open preconnect TCP connections under `default-src 'none'`. Chromium can open a connection for a prerender, or for a frame the policy then blocks. So the declared origins limit what a widget can load and show, not every packet the reader's machine sends.
