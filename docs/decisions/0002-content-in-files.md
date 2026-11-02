# 0002 — Content in ordinary markdown files

**Context:** references are long-term personal data and must not be locked into the app.

**Decision:** store content as `.md` files and attachments in a user folder. Any database is only a
disposable index.

**Consequences:**

- Content remains readable without the app and can be versioned or synchronized by other tools.
- The app must handle external changes and atomic writes.
- Features that depend on metadata use frontmatter in the file itself.
