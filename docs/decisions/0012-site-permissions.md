# 0012 — Site permissions asked in the panel

**Context:** a page asking for a permission, such as the location, got WebView2's own prompt: a
separate window in a corner of the desktop that took focus, so the panel hid and the prompt stayed
alone. Pages are untrusted ([0004](0004-untrusted-content.md)) and have no channel to Rust. WebView2
saves the answers to its prompt in the page profile by default, where the app can neither show nor
remove them.

**Decision:**

- **Who answers:** the app, for every request of the page window and none of `main`'s. Each request
  gets a deferral and always ends as Allow or Deny, never Default, so WebView2 shows no prompt; it is
  not saved in the profile, and what the profile saved before is reset to Default when the page
  window is created. The app's preferences are the only place a choice lives.
- **Kinds asked:** location, camera, microphone and clipboard read. Every other kind is denied
  without asking: notifications would appear outside the panel from a page that is hidden most of
  the time; sensors, local fonts and MIDI serve fingerprinting more than a reference panel; autoplay
  starts sound by itself; automatic downloads, file access and window management need features the
  page window does not offer; unknown and future kinds stay denied until chosen.
- **Scope:** a choice belongs to the page's registrable site and the kind, with the content blocking
  site key, so it covers subdomains, schemes and ports. A frame of another site is denied without
  asking, even where the page is allowed: a site's choice never extends to third parties it embeds,
  and Chromium already requires the page to delegate the feature to that frame. A frame of the same
  site and another origin asks for the site, and the bar names that origin.
- **Asking:** only while the page is on screen, so an answer always comes from a bar the user sees.
  A remembered choice applies at once, on screen or not. Without one, a request out of sight, or one
  still waiting when the panel or the page hides or the document changes, is denied once, not
  remembered: the page may ask again. Requests wait in order (at most eight per page window); an
  answer settles every waiting request for the same site and kind. Every answer is remembered; there
  is no "only this time".

**Consequences:**

- While a request waits, the panel stays open as for a dialog, even if focus moves to another app; Esc
  in the search bar or the shortcut still closes it, denying the request.
- A site that asks on load from a hidden page, or right after the user leaves it, gets a denial
  without a prompt. Answers given to WebView2's prompt by older versions are forgotten once.
- Removing a choice in Preferences makes the site ask again, since WebView2 kept no copy.
- On WebView2 runtimes without `SavesInProfile` or `Profile4` the prompt still never shows, but the
  runtime may keep its own copy of an answer.
