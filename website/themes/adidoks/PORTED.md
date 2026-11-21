# AdiDoks, ported to Zola 0.23

This is [AdiDoks](https://github.com/aaranxu/adidoks) (MIT, see `LICENSE`) at
commit `5c69827`, adapted for naml's docs:

- the templates are rewritten for Tera 2, which Zola uses since 0.23 (the
  original ones use macros, which Tera 2 replaced and Zola refuses to parse);
  the theme's macros are inlined into `base.html` and `docs/page.html`, and
  the blog, authors, math and SEO templates are gone;
- `sass/`, `static/js/`, `static/plugins/` and `static/fonts/` are the
  theme's own, untouched, except that Bootstrap's prebuilt `dist/` and `js/`
  were dropped and its entry stylesheets renamed as partials, so Zola doesn't
  compile them on their own;
- the demo images, favicons and Netlify files were removed.
