# Vendored Mermaid (M11ac Part 2)

The Markdown Preview draws ```` ```mermaid ```` blocks with this file, offline, in a hidden
WebKit view (`src/mermaid_render.rs`). It is embedded in the fuxx binary with `include_str!`;
nothing is fetched at run time.

- **Package:** `mermaid` (npm), **version 12.2.0** (published 2026-10-13)
- **File:** `dist/mermaid.min.js` from the package, unchanged
- **Source URL:** https://registry.npmjs.org/mermaid/-/mermaid-12.2.0.tgz
  (also served as https://cdn.jsdelivr.net/npm/mermaid@12.2.0/dist/mermaid.min.js, the same bytes)
- **Tarball integrity (npm):** `sha512-w7nr/SFrLO7BaVTs3dQTvYSV6nYSBj0Yy9DjBhsIJtAcHqRzXSq4Pg32ujHZ/AdWsAMNEvvhbm5dt9E2tOiHzw==`
- **SHA-256 of `mermaid.min.js`:** `a8a6a6f805b074a8261a2825c86f99c8054acb3b35f50bb7e24a4f9f2c3c3b9f`
- **Size:** 5,494,801 bytes
- **Licence:** MIT (`LICENSE`, copied from the package). The bundle also carries code from its
  dependencies (d3, dagre, DOMPurify, lodash-es, khroma, cytoscape and others) under their own
  licences, kept in the file's `/*! … */` banners.

To update: download the new release's tarball, check its `integrity` against the npm registry,
copy `package/dist/mermaid.min.js` and `package/LICENSE` here, and update this file and the
`MERMAID_JS_SHA256` test in `src/mermaid.rs`.
